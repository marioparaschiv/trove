use std::{collections::BTreeSet, sync::Mutex};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{
    CollapseToNotes, GraphRerank, Meaning, Pipeline, Query, Scoring, SearchError, SearchResult,
    Signal, Stage, WEIGHTS, Weights, Words, results,
};
use crate::{embedder::Embedder, index::Index};

pub const DEFAULT_LIMIT: u32 = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchRequest {
    pub query: String,
    /// The most files to return. Defaults to 10.
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

/// Finds the notes that best match `request`.
pub fn search(
    index: &Mutex<Index>,
    embedder: &Embedder,
    request: &SearchRequest,
) -> Result<SearchResponse, SearchError> {
    search_with(index, embedder, request, WEIGHTS)
}

/// [`search`] ranking by `weights` instead of [`WEIGHTS`], for the eval to fit them.
pub fn search_with(
    index: &Mutex<Index>,
    embedder: &Embedder,
    request: &SearchRequest,
    weights: Weights,
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
        stages.push(Box::new(Meaning::new(index, embedder, weights.feedback)));
    }
    if !request.skip.contains(&Signal::Words) {
        stages.push(Box::new(Words::new(index, weights.title != 0.0)));
    }
    stages.push(Box::new(Scoring::new(index, weights)));
    if !request.skip.contains(&Signal::Graph) {
        stages.push(Box::new(GraphRerank::new(index, weights.graph_share)));
    }
    stages.push(Box::new(CollapseToNotes::new(index)));

    let pipeline = Pipeline::new(stages);
    let mut candidates = pipeline.run(&Query {
        text: text.to_owned(),
    })?;
    candidates.truncate(limit as usize);

    Ok(SearchResponse {
        // The query's words are marked only where search by words found them.
        results: results(
            index,
            &candidates,
            (!request.skip.contains(&Signal::Words)).then_some(text),
        )?,
    })
}
