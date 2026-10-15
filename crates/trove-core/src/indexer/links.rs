use std::collections::BTreeSet;

use rusqlite::Transaction;

use super::{FileProcessor, IndexerError, NoteFile, chunker::Fence, frontmatter::Frontmatter};
use crate::graph::{EdgeKind, NodeKey};

/// Starting weights from the spec; the eval decides the final ones.
const LINK_WEIGHT: f32 = 1.0;
const PROPERTY_LINK_WEIGHT: f32 = 1.0;
const TAG_WEIGHT: f32 = 0.5;

/// Writes a note's own edges: wikilinks (with heading and block targets), embeds and markdown links in the body as
/// links, tags in the body and the `tags` property as tags, and links in any other property as property links. Code
/// is skipped. Targets are stored as written and resolved when the graph is built.
pub struct LinkParser;

impl FileProcessor for LinkParser {
    fn process(&self, tx: &Transaction<'_>, note: &NoteFile<'_>) -> Result<(), IndexerError> {
        let src = NodeKey::Note(note.id);
        tx.execute(
            "DELETE FROM edges WHERE src = ?1 AND kind IN (?2, ?3, ?4)",
            (&src, EdgeKind::Link, EdgeKind::PropertyLink, EdgeKind::Tag),
        )?;

        let mut body = Found::default();
        let body_start = Frontmatter::find(note.text).map_or(0, |frontmatter| frontmatter.end);
        let mut fence = None;
        for line in note.text[body_start..].lines() {
            if !Fence::track(&mut fence, line.trim_end()) {
                scan(&without_code_spans(line), &mut body);
            }
        }

        let mut properties = Found::default();
        let mut statement = tx.prepare(
            "SELECT j.fullkey = '$.tags' OR j.fullkey LIKE '$.tags[%', j.value
             FROM files f, json_tree(f.properties) j
             WHERE f.id = ?1 AND j.type = 'text'",
        )?;
        let mut rows = statement.query([note.id])?;
        while let Some(row) = rows.next()? {
            let value: String = row.get(1)?;
            if row.get(0)? {
                properties
                    .tags
                    .extend(tag(value.trim().trim_start_matches('#')));
            } else {
                scan(&value, &mut properties);
            }
        }

        let mut insert = tx.prepare(
            "INSERT OR IGNORE INTO edges (src, dst, kind, weight) VALUES (?1, ?2, ?3, ?4)",
        )?;
        let edges = [
            (body.links, EdgeKind::Link, LINK_WEIGHT),
            (
                properties.links,
                EdgeKind::PropertyLink,
                PROPERTY_LINK_WEIGHT,
            ),
        ];
        for (targets, kind, weight) in edges {
            for target in targets {
                insert.execute((&src, NodeKey::Link(target), kind, weight))?;
            }
        }
        for tag in body.tags.into_iter().chain(properties.tags) {
            insert.execute((&src, NodeKey::Tag(tag), EdgeKind::Tag, TAG_WEIGHT))?;
        }

        Ok(())
    }
}

#[derive(Default)]
struct Found {
    /// Targets as written: a path, then optionally `#heading` or `#^block`.
    links: BTreeSet<String>,
    /// Lowercased, without `#`.
    tags: BTreeSet<String>,
}

/// `line` with each inline code span replaced by a space.
fn without_code_spans(line: &str) -> String {
    line.split('`').step_by(2).collect::<Vec<_>>().join(" ")
}

/// Collects the links and tags in one line of text.
fn scan(text: &str, found: &mut Found) {
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        let consumed = if let Some(inner) = rest.strip_prefix("[[") {
            wikilink(inner, found)
        } else if let Some(inner) = rest.strip_prefix('[') {
            markdown_link(inner, found)
        } else if let Some(inner) = rest.strip_prefix('#')
            && text[..at]
                .chars()
                .next_back()
                .is_none_or(char::is_whitespace)
        {
            body_tag(inner, found)
        } else {
            None
        };

        at += consumed.unwrap_or_else(|| rest.chars().next().map_or(1, char::len_utf8));
    }
}

/// `[[target#heading|alias]]` (an embed's `!` is left before it). Returns the bytes used, opening brackets included.
fn wikilink(inner: &str, found: &mut Found) -> Option<usize> {
    let end = inner.find("]]")?;
    let target = inner[..end].split('|').next().unwrap_or_default().trim();
    add_link(target, found);

    Some(2 + end + 2)
}

/// `[text](destination "title")`, for destinations inside the vault (no URL scheme), percent-decoded.
fn markdown_link(inner: &str, found: &mut Found) -> Option<usize> {
    let text_end = inner.find("](")?;
    if inner[..text_end].contains(['[', ']']) {
        return None;
    }

    let destination = &inner[text_end + 2..];
    let (target, length) = match destination.strip_prefix('<') {
        Some(angled) => {
            let end = angled.find('>')?;
            let close = angled[end + 1..].find(')')?;
            (&angled[..end], 1 + end + 1 + close + 1)
        }
        None => {
            let end = destination.find(')')?;
            let target = destination[..end]
                .split_whitespace()
                .next()
                .unwrap_or_default();
            (target, end + 1)
        }
    };

    let has_scheme = target
        .split_once(':')
        .is_some_and(|(scheme, _)| !scheme.contains(['/', '#']));
    if !has_scheme {
        add_link(&percent_decoded(target), found);
    }

    Some(1 + text_end + 2 + length)
}

/// A link to another note: one with a path, not only `#heading` in the same note.
fn add_link(target: &str, found: &mut Found) {
    let path = target.split('#').next().unwrap_or_default();
    if !path.trim().is_empty() {
        found.links.insert(target.to_owned());
    }
}

/// `#tag` after whitespace or at the start of a line.
fn body_tag(inner: &str, found: &mut Found) -> Option<usize> {
    let length = inner
        .find(|char: char| !(char.is_alphanumeric() || matches!(char, '_' | '-' | '/')))
        .unwrap_or(inner.len());
    let tag = tag(&inner[..length])?;
    found.tags.insert(tag);

    Some(1 + length)
}

/// A tag name lowercased; `None` when it is empty or only digits, which Obsidian doesn't count as a tag.
fn tag(name: &str) -> Option<String> {
    if name.chars().all(|char| char.is_ascii_digit()) {
        return None;
    }

    Some(name.to_lowercase())
}

/// `%XX` escapes decoded; text that doesn't decode to UTF-8 stays as written.
fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let escaped = (bytes[at] == b'%')
            .then(|| text.get(at + 1..at + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escaped {
            Some(byte) => {
                decoded.push(byte);
                at += 3;
            }
            None => {
                decoded.push(bytes[at]);
                at += 1;
            }
        }
    }

    String::from_utf8(decoded).unwrap_or_else(|_| text.to_owned())
}
