use std::{collections::BTreeSet, sync::Mutex};

use super::{Candidate, Query, SearchError, Signal, Stage};
use crate::index::{self, Index};

/// Search by words: full-text search with BM25 over each chunk's note title, heading path and text, Porter-stemmed.
/// A chunk matches when it has every word of the query, so a word query never finds more than Obsidian's own search
/// would. Adds its matches, best first, with the negated BM25 score (higher is better) as the score.
pub struct Words<'a> {
    index: &'a Mutex<Index>,
}

impl<'a> Words<'a> {
    pub fn new(index: &'a Mutex<Index>) -> Self {
        Self { index }
    }
}

impl Stage for Words<'_> {
    fn run(
        &self,
        query: &Query,
        mut candidates: Vec<Candidate>,
    ) -> Result<Vec<Candidate>, SearchError> {
        let Some(pattern) = fts_pattern(&query.text) else {
            return Ok(candidates);
        };

        let index = index::lock(self.index);
        // Weights per column: title, heading path, text.
        let mut statement = index.connection().prepare(
            "SELECT rowid, bm25(chunks_fts, 3.0, 2.0, 1.0) AS rank
             FROM chunks_fts WHERE chunks_fts MATCH ?1 ORDER BY rank",
        )?;
        let matches = statement.query_map([pattern], |row| {
            Ok(Candidate {
                chunk_id: row.get(0)?,
                score: -row.get::<_, f32>(1)?,
                why: BTreeSet::from([Signal::Words]),
            })
        })?;
        for candidate in matches {
            candidates.push(candidate?);
        }

        Ok(candidates)
    }
}

/// The query as an FTS5 pattern: each whitespace-separated word quoted as a phrase, so punctuation inside a word
/// (`TRV-8812`, `user@host`) keeps its parts together and never reads as FTS5 syntax. Words without a letter or digit
/// are left out; `None` when none are left.
fn fts_pattern(text: &str) -> Option<String> {
    let phrases: Vec<String> = text
        .split_whitespace()
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .map(|word| format!("\"{}\"", word.replace('"', "\"\"")))
        .collect();

    (!phrases.is_empty()).then(|| phrases.join(" "))
}
