//! What search returns for each file: where in it the query matched, with lines and a snippet.

use std::{
    collections::{BTreeSet, HashMap},
    fs,
    ops::Range,
    path::Path,
    sync::Mutex,
};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{Candidate, Signal, highlights::Highlighter};
use crate::index::{self, Index};

/// About this long, in bytes, a snippet is at most.
const SNIPPET_BYTES: usize = 240;
/// About this many bytes of text before the first of the query's words lead into a snippet.
const SNIPPET_LEAD: usize = 60;

/// A file, with where it matched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SearchResult {
    /// The source folder the file is in, as registered.
    pub source: String,
    /// The file's path in its source, `/`-separated.
    pub path: String,
    pub title: String,
    pub kind: Kind,
    pub score: f32,
    /// The stages that found the file.
    pub why: BTreeSet<Signal>,
    /// The file's best matching parts, up to three, best first. Never empty.
    pub matches: Vec<Match>,
}

/// What kind of file a result is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Markdown,
    Text,
    Image,
    Audio,
    Video,
}

/// One part of a file that matched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Match {
    /// How well this part matches on its own. Orders a result's matches; not comparable with the result's score.
    pub score: f32,
    /// The headings above the part, outermost first. Empty for text before the first heading.
    pub heading_path: Vec<String>,
    pub location: Location,
    /// The part's text around the query's words, for text files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet: Option<Snippet>,
}

/// Where a match is in its file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Location {
    /// A span of a text file, in UTF-8 bytes from the start of the file.
    Text {
        byte_start: u64,
        byte_end: u64,
        /// The span's first line, from 1. Missing when the file changed since it was indexed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line_start: Option<u64>,
        /// The span's last line, inclusive. Missing when the file changed since it was indexed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line_end: Option<u64>,
    },
    /// A stretch of an audio or video file, in milliseconds from its start.
    Time { start_ms: u64, end_ms: u64 },
    /// The whole file, such as an image.
    Whole,
}

/// A piece of a match's text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Snippet {
    pub text: String,
    /// The query's words in `text`, in order. Empty for a part found only by meaning.
    pub highlights: Vec<Highlight>,
}

/// A range of a snippet's text, in UTF-8 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Highlight {
    pub start: u64,
    pub end: u64,
}

/// A chunk as the index has it.
struct Chunk {
    source: String,
    path: String,
    title: String,
    heading_path: String,
    span: Range<usize>,
    text: String,
    highlights: Vec<Range<usize>>,
}

/// The results for `candidates`, in order, each with its chunks as matches. Marks `query`'s words in the snippets when
/// given. A candidate whose chunk is no longer indexed is left out.
pub(crate) fn results(
    index: &Mutex<Index>,
    candidates: &[Candidate],
    query: Option<&str>,
) -> rusqlite::Result<Vec<SearchResult>> {
    let chunks_of = |candidate: &Candidate| -> Vec<(i64, f32)> {
        if candidate.chunks.is_empty() {
            vec![(candidate.chunk_id, candidate.score)]
        } else {
            candidate.chunks.clone()
        }
    };
    let chunk_ids: Vec<i64> = candidates
        .iter()
        .flat_map(|candidate| chunks_of(candidate).into_iter().map(|(id, _)| id))
        .collect();
    let mut chunks = read_chunks(&index::lock(index), &chunk_ids, query)?;

    // Lines come from the files themselves, read without holding the index.
    let mut files: HashMap<(String, String), Option<Vec<u8>>> = HashMap::new();
    Ok(candidates
        .iter()
        .filter_map(|candidate| {
            let best = chunks.get(&candidate.chunk_id)?;
            let (source, path, title) =
                (best.source.clone(), best.path.clone(), best.title.clone());
            let file = files
                .entry((source.clone(), path.clone()))
                .or_insert_with(|| fs::read(Path::new(&source).join(&path)).ok());
            let matches = chunks_of(candidate)
                .into_iter()
                .filter_map(|(chunk_id, score)| {
                    Some(text_match(
                        chunks.remove(&chunk_id)?,
                        score,
                        file.as_deref(),
                    ))
                })
                .collect();

            Some(SearchResult {
                source,
                path,
                title,
                kind: Kind::Markdown,
                score: candidate.score,
                why: candidate.why.clone(),
                matches,
            })
        })
        .collect())
}

/// The chunks `chunk_ids` by id, with `query`'s words found in each when given.
fn read_chunks(
    index: &Index,
    chunk_ids: &[i64],
    query: Option<&str>,
) -> rusqlite::Result<HashMap<i64, Chunk>> {
    let conn = index.connection();
    let highlighter = query
        .map(|query| Highlighter::new(conn, query))
        .transpose()?;
    let mut statement = conn.prepare(
        "SELECT c.id, s.path, f.path, f.title, c.heading_path, c.span_start, c.span_end,
                coalesce(t.original, t.text)
         FROM chunks c
         JOIN files f ON f.id = c.file_id
         JOIN sources s ON s.id = f.source_id
         JOIN chunks_fts t ON t.rowid = c.id
         WHERE c.id IN (SELECT value FROM json_each(?1))",
    )?;
    let rows = statement.query_map([serde_json::Value::from(chunk_ids).to_string()], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            Chunk {
                source: row.get(1)?,
                path: row.get(2)?,
                title: row.get(3)?,
                heading_path: row.get(4)?,
                span: row.get(5)?..row.get(6)?,
                text: row.get(7)?,
                highlights: Vec::new(),
            },
        ))
    })?;

    rows.map(|row| {
        let (chunk_id, mut chunk) = row?;
        if let Some(highlighter) = &highlighter {
            chunk.highlights = highlighter.ranges(conn, chunk_id, &chunk.text)?;
        }
        Ok((chunk_id, chunk))
    })
    .collect()
}

/// A text chunk as a match, with its lines read from `file` when it still has the chunk's text at its span.
fn text_match(chunk: Chunk, score: f32, file: Option<&[u8]>) -> Match {
    let lines = file
        .filter(|file| file.get(chunk.span.clone()) == Some(chunk.text.as_bytes()))
        .map(|file| lines(file, chunk.span.clone()));

    Match {
        score,
        heading_path: if chunk.heading_path.is_empty() {
            Vec::new()
        } else {
            chunk.heading_path.split('\n').map(str::to_owned).collect()
        },
        location: Location::Text {
            byte_start: chunk.span.start as u64,
            byte_end: chunk.span.end as u64,
            line_start: lines.as_ref().map(|lines| lines.start),
            line_end: lines.map(|lines| lines.end),
        },
        snippet: Some(snippet(&chunk.text, &chunk.highlights)),
    }
}

/// The first and last line of `span` in `file`, from 1, both inclusive.
fn lines(file: &[u8], span: Range<usize>) -> Range<u64> {
    let line_of = |at: usize| 1 + file[..at].iter().filter(|&&byte| byte == b'\n').count() as u64;
    let last = span.end.saturating_sub(1).max(span.start);

    line_of(span.start)..line_of(last)
}

/// A piece of `text` of up to about [`SNIPPET_BYTES`] around its first highlight (its start when it has none), cut at
/// spaces where it can be, with the highlights inside it.
fn snippet(text: &str, highlights: &[Range<usize>]) -> Snippet {
    let anchor = highlights.first().cloned().unwrap_or(0..0);
    let mut start = 0;
    if anchor.start > SNIPPET_LEAD {
        start = text.floor_char_boundary(anchor.start - SNIPPET_LEAD);
        if let Some(space) = text[start..anchor.start].find(char::is_whitespace) {
            start += space;
        }
    }
    let mut end = text.floor_char_boundary((start + SNIPPET_BYTES).min(text.len()));
    let keep = anchor.end.clamp(start, end);
    if end < text.len()
        && let Some(space) = text[keep..end].rfind(char::is_whitespace)
    {
        end = keep + space;
    }
    let piece = &text[start..end];
    start += piece.len() - piece.trim_start().len();
    end = start + piece.trim().len();

    Snippet {
        text: text[start..end].to_owned(),
        highlights: highlights
            .iter()
            .filter(|range| range.start >= start && range.end <= end)
            .map(|range| Highlight {
                start: (range.start - start) as u64,
                end: (range.end - start) as u64,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_count_from_one_and_end_on_the_spans_last_byte() {
        let file = b"# Title\n\nFirst line\nsecond line\n\n## Next\n";

        assert_eq!(lines(file, 0..7), 1..1);
        assert_eq!(lines(file, 9..31), 3..4);
        // A span ending in its newline ends on that line.
        assert_eq!(lines(file, 9..32), 3..4);
    }

    #[test]
    fn a_snippet_starts_at_the_text_without_highlights_and_leads_into_the_first_one() {
        let short = snippet("Feed the starter daily.", &[]);
        assert_eq!(short.text, "Feed the starter daily.");
        assert!(short.highlights.is_empty());

        let text = format!(
            "{} boiler pressure {}",
            "word ".repeat(40),
            "tail ".repeat(60)
        );
        let at = text.find("boiler").unwrap();
        let long = snippet(&text, &[at..at + 6, at + 7..at + 15]);
        assert!(long.text.len() <= SNIPPET_BYTES, "{}", long.text);
        assert!(long.text.starts_with("word "), "{}", long.text);
        assert!(long.text.ends_with("tail"), "{}", long.text);
        let marked: Vec<&str> = long
            .highlights
            .iter()
            .map(|range| &long.text[range.start as usize..range.end as usize])
            .collect();
        assert_eq!(marked, ["boiler", "pressure"]);
    }

    #[test]
    fn a_snippet_cuts_inside_a_character_at_its_start() {
        let text = format!("{}é target", "é".repeat(40));
        let at = text.find("target").unwrap();

        let target = at..at + 6;
        let piece = snippet(&text, std::slice::from_ref(&target));
        let range = piece.highlights[0];
        assert_eq!(
            &piece.text[range.start as usize..range.end as usize],
            "target"
        );
    }
}
