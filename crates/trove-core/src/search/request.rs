use std::{
    collections::{BTreeSet, HashMap},
    sync::Mutex,
};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{
    Candidate, CollapseToNotes, GraphRerank, Meaning, Pipeline, Query, SearchError, Signal, Stage,
};
use crate::{
    embedder::Embedder,
    index::{self, Index},
};

pub const DEFAULT_LIMIT: u32 = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchRequest {
    pub query: String,
    /// The most notes to return. Defaults to 10.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(minimum = 1)]
    pub limit: Option<u32>,
    /// Stages to turn off, to measure what each adds.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub skip: BTreeSet<Signal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SearchResponse {
    /// Best first. Empty when nothing matches.
    pub results: Vec<SearchResult>,
}

/// A note, pointing at its best matching chunk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SearchResult {
    /// The source folder the note is in, as registered.
    pub source: String,
    /// The note's path in its source, `/`-separated.
    pub path: String,
    pub title: String,
    /// The headings above the best chunk, outermost first. Empty for text before the first heading.
    pub heading_path: Vec<String>,
    pub score: f32,
    /// The stages that found the note.
    pub why: BTreeSet<Signal>,
}

/// Finds the notes that best match `request`.
pub fn search(
    index: &Mutex<Index>,
    embedder: &Embedder,
    request: &SearchRequest,
) -> Result<SearchResponse, SearchError> {
    let text = request.query.trim();
    if text.is_empty() {
        return Err(SearchError::EmptyQuery);
    }
    let limit = request.limit.unwrap_or(DEFAULT_LIMIT);
    if limit == 0 {
        return Err(SearchError::ZeroLimit);
    }

    let mut stages: Vec<Box<dyn Stage>> = Vec::new();
    if !request.skip.contains(&Signal::Meaning) {
        stages.push(Box::new(Meaning::new(index, embedder)));
    }
    if !request.skip.contains(&Signal::Graph) {
        stages.push(Box::new(GraphRerank::new(index)));
    }
    stages.push(Box::new(CollapseToNotes::new(index)));

    let pipeline = Pipeline::new(stages);
    let mut candidates = pipeline.run(&Query {
        text: text.to_owned(),
    })?;
    candidates.truncate(limit as usize);

    Ok(SearchResponse {
        results: results(&index::lock(index), &candidates)?,
    })
}

fn results(index: &Index, candidates: &[Candidate]) -> rusqlite::Result<Vec<SearchResult>> {
    let chunk_ids: Vec<i64> = candidates
        .iter()
        .map(|candidate| candidate.chunk_id)
        .collect();
    let mut statement = index.connection().prepare(
        "SELECT c.id, s.path, f.path, f.title, c.heading_path
         FROM chunks c
         JOIN files f ON f.id = c.file_id
         JOIN sources s ON s.id = f.source_id
         WHERE c.id IN (SELECT value FROM json_each(?1))",
    )?;
    let mut notes: HashMap<i64, (String, String, String, String)> = statement
        .query_map([serde_json::Value::from(chunk_ids).to_string()], |row| {
            Ok((
                row.get(0)?,
                (row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?),
            ))
        })?
        .collect::<Result<_, _>>()?;

    Ok(candidates
        .iter()
        .filter_map(|candidate| {
            let (source, path, title, heading_path) = notes.remove(&candidate.chunk_id)?;
            Some(SearchResult {
                source,
                path,
                title,
                heading_path: if heading_path.is_empty() {
                    Vec::new()
                } else {
                    heading_path.split('\n').map(str::to_owned).collect()
                },
                score: candidate.score,
                why: candidate.why.clone(),
            })
        })
        .collect())
}
