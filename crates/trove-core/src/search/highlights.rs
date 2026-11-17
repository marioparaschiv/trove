//! Where a query's words are in a chunk's text as written, for a result's snippet.

use std::{collections::HashSet, ops::Range};

use rusqlite::{Connection, OptionalExtension};

use super::words::{self, Pattern};
use crate::index::keyword_text;

/// Marks the words search by words matched in its stored text, which `highlight()` returns between them.
const OPEN: char = '\u{1}';
const CLOSE: char = '\u{2}';

/// Finds a query's words in chunk texts the way search by words found them: each word as FTS5 matches it (stemmed, as
/// a prefix, or the closest indexed word for a misspelt one) and each run of Chinese, Japanese or Thai inside longer
/// text.
pub(super) struct Highlighter {
    /// The FTS5 query over `chunks_fts`, `None` when no word is matched that way.
    words: Option<String>,
    /// Runs of characters, normalised and lowercased.
    runs: Vec<String>,
}

impl Highlighter {
    pub fn new(conn: &Connection, query: &str) -> rusqlite::Result<Self> {
        let mut word_patterns = Vec::new();
        let mut runs = Vec::new();
        for pattern in words::patterns(conn, &words::terms(query))? {
            match pattern {
                Pattern::Words(pattern) => word_patterns.push(pattern),
                Pattern::Substring(run) => runs.push(run),
            }
        }

        Ok(Self {
            words: (!word_patterns.is_empty()).then(|| word_patterns.join(" OR ")),
            runs,
        })
    }

    /// The byte ranges of the query's words in `text`, chunk `chunk_id`'s text as written, in order. Empty when the
    /// chunk has none of them.
    pub fn ranges(
        &self,
        conn: &Connection,
        chunk_id: i64,
        text: &str,
    ) -> rusqlite::Result<Vec<Range<usize>>> {
        let matched = match &self.words {
            Some(words) => matched_words(conn, chunk_id, words)?,
            None => HashSet::new(),
        };
        if matched.is_empty() && self.runs.is_empty() {
            return Ok(Vec::new());
        }

        let mut ranges = Vec::new();
        for (span, word) in tokens(text) {
            let normalized = keyword_text::normalized(word).to_lowercase();
            if matched.contains(&normalized) {
                ranges.push(span);
                continue;
            }

            let mut runs_in_word: Vec<Range<usize>> = self
                .runs
                .iter()
                .flat_map(|run| {
                    normalized
                        .match_indices(run.as_str())
                        .map(|(start, run)| start..start + run.len())
                })
                .collect();
            if runs_in_word.is_empty() {
                continue;
            }
            // Offsets into the normalised word hold in the word as written only when normalising left it alone.
            if normalized == word {
                runs_in_word.sort_by_key(|run| run.start);
                ranges.extend(
                    merged(runs_in_word).map(|run| span.start + run.start..span.start + run.end),
                );
            } else {
                ranges.push(span);
            }
        }

        Ok(ranges)
    }
}

/// The words of chunk `chunk_id`'s stored text that the FTS5 query `words` matches, lowercased.
fn matched_words(
    conn: &Connection,
    chunk_id: i64,
    words: &str,
) -> rusqlite::Result<HashSet<String>> {
    let marked: Option<String> = conn
        .prepare_cached(
            "SELECT highlight(chunks_fts, 2, char(1), char(2)) FROM chunks_fts
             WHERE chunks_fts MATCH ?1 AND rowid = ?2",
        )?
        .query_row((words, chunk_id), |row| row.get(0))
        .optional()?;

    Ok(marked
        .iter()
        .flat_map(|marked| marked.split(OPEN).skip(1))
        .filter_map(|part| part.split_once(CLOSE))
        .map(|(word, _)| word.to_lowercase())
        .collect())
}

/// The words of `text` as keyword search splits its normalised text, with their byte ranges. A character normalising
/// drops (a soft hyphen, a zero-width joiner) stays inside its word, as it vanishes from the stored text.
fn tokens(text: &str) -> impl Iterator<Item = (Range<usize>, &str)> {
    let mut start = None;
    let mut chars = text.char_indices().chain([(text.len(), ' ')]);

    std::iter::from_fn(move || {
        for (at, c) in chars.by_ref() {
            let in_word = at < text.len()
                && (keyword_text::is_word_char(c)
                    || keyword_text::normalized(c.encode_utf8(&mut [0; 4])).is_empty());
            match (in_word, start) {
                (true, None) => start = Some(at),
                (false, Some(from)) => {
                    start = None;
                    return Some((from..at, &text[from..at]));
                }
                _ => {}
            }
        }

        None
    })
}

/// Sorted ranges with overlapping ones joined.
fn merged(ranges: Vec<Range<usize>>) -> impl Iterator<Item = Range<usize>> {
    let mut joined: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        match joined.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => joined.push(range),
        }
    }

    joined.into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_split_as_keyword_search_splits_them_and_keep_dropped_characters_inside() {
        let words: Vec<(Range<usize>, &str)> =
            tokens("Boiler-service, co\u{00AD}operate!").collect();

        assert_eq!(
            words,
            [
                (0..6, "Boiler"),
                (7..14, "service"),
                (16..27, "co\u{00AD}operate")
            ]
        );
    }
}
