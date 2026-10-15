//! Related notes: personalised PageRank over the graph, seeded by one note.

use std::{collections::BTreeSet, sync::Mutex};

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::{IntoParams, ToSchema};

use crate::{
    graph::VaultGraph,
    index::{self, Index},
    search::{self, Candidate, DEFAULT_LIMIT, SearchResponse, Signal},
};

/// A note, by the source folder it is in and its path there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct RelatedRequest {
    /// The source folder the note is in, as registered.
    pub source: String,
    /// The note's path in its source, `/`-separated.
    pub path: String,
    /// The most notes to return. Defaults to 10.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(minimum = 1)]
    pub limit: Option<u32>,
}

#[derive(Debug, Error)]
pub enum RelatedError {
    #[error("{path} in {source_folder} is not indexed")]
    NotIndexed { source_folder: String, path: String },
    #[error("the limit must be at least 1")]
    ZeroLimit,
    #[error(transparent)]
    Index(#[from] rusqlite::Error),
}

/// The notes closest to the requested one in the graph (links and meaning together), best first, each pointing at its
/// best-ranked section. Scores are relative to the best related note. The note itself is never listed, nor are notes
/// the walk never reaches.
pub fn related(
    index: &Mutex<Index>,
    request: &RelatedRequest,
) -> Result<SearchResponse, RelatedError> {
    let limit = request.limit.unwrap_or(DEFAULT_LIMIT);
    if limit == 0 {
        return Err(RelatedError::ZeroLimit);
    }

    let (file_id, graph) = {
        let index = index::lock(index);
        let file_id: Option<i64> = index
            .connection()
            .query_row(
                "SELECT f.id FROM files f JOIN sources s ON s.id = f.source_id
                 WHERE s.path = ?1 AND f.path = ?2",
                (&request.source, &request.path),
                |row| row.get(0),
            )
            .optional()?;
        let Some(file_id) = file_id else {
            return Err(RelatedError::NotIndexed {
                source_folder: request.source.clone(),
                path: request.path.clone(),
            });
        };

        (file_id, VaultGraph::load(index.connection())?)
    };

    let seeds: Vec<(usize, f32)> = graph
        .note_node(file_id)
        .map(|node| (node, 1.0))
        .into_iter()
        .collect();
    let ranks = graph.graph.personalized_pagerank(&seeds);

    let mut sections: Vec<(i64, f32)> = graph
        .note_ranks(&ranks)
        .into_iter()
        .filter(|&(note, ref rank)| note != file_id && rank.rank > 0.0)
        .filter_map(|(_, rank)| Some((rank.best_section?.0, rank.rank)))
        .collect();
    sections.sort_by(|a, b| b.1.total_cmp(&a.1));
    sections.truncate(limit as usize);

    let best = sections.first().map_or(1.0, |&(_, rank)| rank);
    let candidates: Vec<Candidate> = sections
        .into_iter()
        .map(|(chunk_id, rank)| Candidate {
            chunk_id,
            score: rank / best,
            why: BTreeSet::from([Signal::Graph]),
        })
        .collect();

    Ok(SearchResponse {
        results: search::results(&index::lock(index), &candidates)?,
    })
}
