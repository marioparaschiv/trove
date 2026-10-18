//! What `trove eval --fit` fits the ranking [`Weights`] on: each note a query matches, with the signals scoring ranks
//! it by. Search itself never reads them this way.

use std::sync::Mutex;

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
    let mut path_of = index
        .connection()
        .prepare_cached("SELECT path FROM files WHERE id = ?1")?;
    let mut notes = matches
        .into_iter()
        .map(|note| {
            Ok(NoteFeatures {
                path: path_of.query_row([note.file_id], |row| row.get(0))?,
                signals: note.signals,
            })
        })
        .collect::<rusqlite::Result<Vec<_>>>()?;
    notes.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(QueryFeatures {
        lookup: words::is_lookup(&words::terms(&query.text)),
        notes,
    })
}
