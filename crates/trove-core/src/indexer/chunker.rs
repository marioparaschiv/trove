use std::ops::Range;

use rusqlite::Transaction;
use sha2::{Digest, Sha256};

use super::{FileProcessor, NoteFile};
use crate::index::IndexError;

/// Splits a note into one chunk per heading section and writes `chunks` and `chunks_fts`.
pub struct Chunker;

impl Chunker {
    /// Part of `vector_version`: bump it whenever chunk texts or boundaries change.
    pub const VERSION: u32 = 1;
}

struct Section {
    heading_path: String,
    span: Range<usize>,
}

impl FileProcessor for Chunker {
    fn process(&self, tx: &Transaction<'_>, note: &NoteFile<'_>) -> Result<(), IndexError> {
        tx.execute("DELETE FROM chunks WHERE file_id = ?1", [note.id])?;

        let mut insert_chunk = tx.prepare(
            "INSERT INTO chunks (file_id, ordinal, heading_path, span_start, span_end, text_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        let mut insert_fts = tx.prepare(
            "INSERT INTO chunks_fts (rowid, title, heading_path, text) VALUES (?1, ?2, ?3, ?4)",
        )?;

        for (ordinal, section) in sections(note.text).into_iter().enumerate() {
            let text = &note.text[section.span.clone()];
            let text_hash = Sha256::digest(text.as_bytes());

            let chunk_id = insert_chunk.insert((
                note.id,
                ordinal,
                &section.heading_path,
                section.span.start,
                section.span.end,
                text_hash.as_slice(),
            ))?;
            insert_fts.execute((chunk_id, note.title, &section.heading_path, text))?;
        }

        Ok(())
    }
}

fn sections(text: &str) -> Vec<Section> {
    let body_start = frontmatter_end(text);
    let mut sections = Vec::new();
    let mut headings: Vec<(usize, &str)> = Vec::new();
    let mut fence: Option<Fence> = None;
    let mut section_start = body_start;
    let mut line_start = body_start;

    for line in text[body_start..].split_inclusive('\n') {
        let line_end = line_start + line.len();
        let content = line.trim_end();

        if let Some(marker) = Fence::parse(content) {
            fence = match fence {
                None => Some(marker),
                Some(open) if marker.closes(&open) => None,
                open => open,
            };
        } else if fence.is_none()
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
    let raw = &text[span.clone()];
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return;
    }

    let start = span.start + (raw.len() - raw.trim_start().len());
    let heading_path = headings
        .iter()
        .map(|(_, title)| *title)
        .collect::<Vec<_>>()
        .join("\n");

    sections.push(Section {
        heading_path,
        span: start..start + trimmed.len(),
    });
}

/// Byte offset where the body starts, after a leading `---` frontmatter block if there is one.
fn frontmatter_end(text: &str) -> usize {
    let mut lines = text.split_inclusive('\n');
    let Some(first) = lines.next() else {
        return 0;
    };
    if first.trim_end() != "---" {
        return 0;
    }

    let mut offset = first.len();
    for line in lines {
        offset += line.len();
        if matches!(line.trim_end(), "---" | "...") {
            return offset;
        }
    }

    0
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

struct Fence {
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

    fn closes(&self, open: &Fence) -> bool {
        self.marker == open.marker && self.len >= open.len && !self.has_info
    }
}
