//! What `trove eval --fit` fits the ranking [`Weights`] on: each note a query matches, with the signals scoring ranks
//! it by. Search itself never reads them this way.

use std::{collections::HashMap, sync::Mutex};

use super::{Meaning, Pipeline, Query, SearchError, Signals, Weights, Words, scoring, words};
use crate::{
    embedder::Embedder,
    index::{self, Index},
};

/// The notes one query matches.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryFeatures {
    /// Whether the query is a lookup, which ranks by [`Weights::lookup_meaning`] and [`Weights::lookup_words`].
    pub lookup: bool,
    /// By path.
    pub notes: Vec<NoteFeatures>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NoteFeatures {
    /// The note's path in its source, `/`-separated, as a search result names it.
    pub path: String,
    /// With titles read, whatever [`Weights::title`] is.
    pub signals: Signals,
}

/// The notes `query` matches, found by meaning (expanded by [`Weights::feedback`]) and words as search finds them.
pub fn query_features(
    index: &Mutex<Index>,
    embedder: &Embedder,
    query: &str,
    weights: Weights,
) -> Result<QueryFeatures, SearchError> {
    let text = query.trim();
    if text.is_empty() {
        return Err(SearchError::EmptyQuery);
    }

    let query = Query {
        text: text.to_owned(),
    };
    let candidates = Pipeline::new(vec![
        Box::new(Meaning::new(index, embedder, weights.feedback)),
        Box::new(Words::new(index, true)),
    ])
    .run(&query)?;

    let index = index::lock(index);
    let matches = scoring::matches(&index, candidates)?;
    let file_ids: Vec<i64> = matches.iter().map(|note| note.file_id).collect();
    let mut paths: HashMap<i64, String> = index
        .connection()
        .prepare("SELECT id, path FROM files WHERE id IN (SELECT value FROM json_each(?1))")?
        .query_map([serde_json::Value::from(file_ids).to_string()], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut notes: Vec<NoteFeatures> = matches
        .into_iter()
        .map(|note| NoteFeatures {
            path: paths
                .remove(&note.file_id)
                .expect("a matching note is in files"),
            signals: note.signals,
        })
        .collect();
    notes.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(QueryFeatures {
        lookup: words::is_lookup(&words::terms(&query.text)),
        notes,
    })
}
