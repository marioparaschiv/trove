use std::{ops::Range, sync::Arc};

use rusqlite::Transaction;
use sha2::{Digest, Sha256};

use super::{FileProcessor, IndexerError, NoteFile, frontmatter::Frontmatter};
use crate::{
    embedder::{CONTEXT_TOKENS, Embedder, Input},
    index::keyword_text,
};

const HEADING_SEPARATOR: &str = " > ";

/// Splits a note into one chunk per heading section, and a section too long for the model's context into chunks of
/// whole paragraphs, then writes `chunks` and `chunks_fts`. Counting tokens loads the model, so a
/// note whose chunks the sync store holds takes them from there.
pub struct Chunker {
    embedder: Arc<Embedder>,
}

impl Chunker {
    /// Part of `vector_version`: bump it whenever chunk texts, boundaries or document titles change.
    pub const VERSION: u32 = 1;

    pub fn new(embedder: Arc<Embedder>) -> Self {
        Self { embedder }
    }

    /// Splits `section` at paragraph boundaries into pieces whose prompts fit the context, each with its token count.
    /// A single paragraph longer than the context stays whole, and the embedder cuts it.
    fn fit(
        &self,
        text: &str,
        title: &str,
        section: Range<usize>,
    ) -> Result<Vec<(Range<usize>, usize)>, IndexerError> {
        let count = |span: &Range<usize>| {
            self.embedder.count_tokens(Input::Document {
                title,
                text: &text[span.clone()],
            })
        };

        let whole = count(&section)?;
        if whole <= CONTEXT_TOKENS as usize {
            return Ok(vec![(section, whole)]);
        }

        let mut pieces = Vec::new();
        let mut current: Option<(Range<usize>, usize)> = None;
        for paragraph in paragraphs(text, section) {
            if let Some((span, tokens)) = current.take() {
                let joined = span.start..paragraph.end;
                let joined_tokens = count(&joined)?;
                if joined_tokens <= CONTEXT_TOKENS as usize {
                    current = Some((joined, joined_tokens));
                    continue;
                }

                pieces.push((span, tokens));
            }

            let tokens = count(&paragraph)?;
            current = Some((paragraph, tokens));
        }
        pieces.extend(current);

        Ok(pieces)
    }

    fn split(&self, note: &NoteFile<'_>) -> Result<Vec<Chunk>, IndexerError> {
        let mut chunks = Vec::new();
        for section in sections(note.text) {
            let title = document_title(note.title, &section.heading_path);
            for (span, token_count) in self.fit(note.text, &title, section.span)? {
                chunks.push(Chunk {
                    heading_path: section.heading_path.clone(),
                    span,
                    token_count: Some(token_count),
                });
            }
        }

        Ok(chunks)
    }
}

/// The title in a chunk's document prompt: its heading path, or the note title for text before the first heading.
pub(crate) fn document_title(note_title: &str, heading_path: &str) -> String {
    if heading_path.is_empty() {
        note_title.to_owned()
    } else {
        heading_path.replace('\n', HEADING_SEPARATOR)
    }
}

struct Section {
    heading_path: String,
    span: Range<usize>,
}

impl FileProcessor for Chunker {
    fn process(&self, tx: &Transaction<'_>, note: &NoteFile<'_>) -> Result<(), IndexerError> {
        tx.execute("DELETE FROM chunks WHERE file_id = ?1", [note.id])?;

        let mut insert_chunk = tx.prepare(
            "INSERT INTO chunks (file_id, ordinal, heading_path, span_start, span_end, token_count, text_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        let mut insert_fts = tx.prepare(keyword_text::INSERT)?;

        let chunks = match note.stored_chunks {
            Some(stored) => stored
                .iter()
                .map(|chunk| Chunk {
                    heading_path: chunk.heading_path.clone(),
                    span: chunk.span.clone(),
                    token_count: None,
                })
                .collect(),
            None => self.split(note)?,
        };

        for (ordinal, chunk) in chunks.iter().enumerate() {
            let text = &note.text[chunk.span.clone()];
            let text_hash = Sha256::digest(text.as_bytes());

            let chunk_id = insert_chunk.insert((
                note.id,
                ordinal,
                &chunk.heading_path,
                chunk.span.start,
                chunk.span.end,
                chunk.token_count,
                text_hash.as_slice(),
            ))?;
            keyword_text::insert(
                &mut insert_fts,
                chunk_id,
                note.title,
                &chunk.heading_path,
                text,
            )?;
        }

        Ok(())
    }
}

struct Chunk {
    heading_path: String,
    span: Range<usize>,
    token_count: Option<usize>,
}

fn sections(text: &str) -> Vec<Section> {
    let body_start = Frontmatter::find(text).map_or(0, |frontmatter| frontmatter.end);
    let mut sections = Vec::new();
    let mut headings: Vec<(usize, &str)> = Vec::new();
    let mut fence: Option<Fence> = None;
    let mut section_start = body_start;
    let mut line_start = body_start;

    for line in text[body_start..].split_inclusive('\n') {
        let line_end = line_start + line.len();
        let content = line.trim_end();

        if !Fence::track(&mut fence, content)
            && let Some((level, title)) = atx_heading(content)
        {
            push_section(&mut sections, text, &headings, section_start..line_start);
            headings.retain(|(open_level, _)| *open_level < level);
            headings.push((level, title));
            section_start = line_end;
        }

        line_start = line_end;
    }

    push_section(&mut sections, text, &headings, section_start..text.len());

    sections
}

fn push_section(
    sections: &mut Vec<Section>,
    text: &str,
    headings: &[(usize, &str)],
    span: Range<usize>,
) {
    let Some(span) = trimmed(text, span) else {
        return;
    };

    let heading_path = headings
        .iter()
        .map(|(_, title)| *title)
        .collect::<Vec<_>>()
        .join("\n");

    sections.push(Section { heading_path, span });
}

/// `span` without surrounding whitespace, or `None` when it is all whitespace.
fn trimmed(text: &str, span: Range<usize>) -> Option<Range<usize>> {
    let raw = &text[span.clone()];
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let start = span.start + (raw.len() - raw.trim_start().len());
    Some(start..start + trimmed.len())
}

/// The spans of the runs of non-blank lines in `section`.
fn paragraphs(text: &str, section: Range<usize>) -> Vec<Range<usize>> {
    let mut paragraphs = Vec::new();
    let mut paragraph_start = section.start;
    let mut line_start = section.start;

    for line in text[section.clone()].split_inclusive('\n') {
        let line_end = line_start + line.len();
        if line.trim().is_empty() {
            paragraphs.extend(trimmed(text, paragraph_start..line_start));
            paragraph_start = line_end;
        }

        line_start = line_end;
    }
    paragraphs.extend(trimmed(text, paragraph_start..section.end));

    paragraphs
}

/// An ATX heading (`## Title`) with at most three spaces of indent.
fn atx_heading(line: &str) -> Option<(usize, &str)> {
    let unindented = line.trim_start_matches(' ');
    if line.len() - unindented.len() > 3 {
        return None;
    }

    let level = unindented.bytes().take_while(|&byte| byte == b'#').count();
    if !(1..=6).contains(&level) {
        return None;
    }

    let rest = &unindented[level..];
    if !(rest.is_empty() || rest.starts_with([' ', '\t'])) {
        return None;
    }

    let title = rest.trim();
    let without_closing = title.trim_end_matches('#');
    let title = if without_closing.is_empty() || without_closing.ends_with([' ', '\t']) {
        without_closing.trim_end()
    } else {
        title
    };

    Some((level, title))
}

pub(super) struct Fence {
    marker: u8,
    len: usize,
    has_info: bool,
}

impl Fence {
    fn parse(line: &str) -> Option<Self> {
        let unindented = line.trim_start_matches(' ');
        if line.len() - unindented.len() > 3 {
            return None;
        }

        let marker = *unindented.as_bytes().first()?;
        if marker != b'`' && marker != b'~' {
            return None;
        }

        let len = unindented
            .bytes()
            .take_while(|&byte| byte == marker)
            .count();
        if len < 3 {
            return None;
        }

        Some(Self {
            marker,
            len,
            has_info: !unindented[len..].trim().is_empty(),
        })
    }

    /// Follows fenced code from line to line: updates `open` for `line` and returns whether the line is a fence or
    /// inside one.
    pub(super) fn track(open: &mut Option<Fence>, line: &str) -> bool {
        let Some(marker) = Self::parse(line) else {
            return open.is_some();
        };

        *open = match open.take() {
            None => Some(marker),
            Some(current) if marker.closes(&current) => None,
            current => current,
        };

        true
    }

    fn closes(&self, open: &Fence) -> bool {
        self.marker == open.marker && self.len >= open.len && !self.has_info
    }
}
