mod collapse;
pub mod features;
mod graph;
mod meaning;
mod request;
mod scoring;
mod weights;
mod words;

use std::{collections::BTreeSet, fmt};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::ToSchema;

use crate::{embedder::EmbedderError, index::VectorScanError};

pub(crate) use collapse::CollapseToNotes;
pub(crate) use graph::GraphRerank;
pub(crate) use meaning::Meaning;
pub(crate) use request::results;
pub use request::{
    DEFAULT_LIMIT, SearchRequest, SearchResponse, SearchResult, search, search_with,
};
pub(crate) use scoring::Scoring;
pub use scoring::Signals;
pub use weights::{WEIGHTS, Weights};
pub(crate) use words::Words;

pub struct Query {
    pub text: String,
}

/// A stage that finds chunks: what a result lists as the reasons it was found, and what a request can turn off.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Signal {
    Meaning,
    Words,
    Graph,
}

impl fmt::Display for Signal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Meaning => "meaning",
            Self::Words => "words",
            Self::Graph => "graph",
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub chunk_id: i64,
    pub score: f32,
    /// The stages that found this chunk.
    pub why: BTreeSet<Signal>,
    /// How much of the query the chunk's note has, for a chunk found by words.
    pub coverage: Option<Coverage>,
}

/// How much of the query's words a note has.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Coverage {
    /// The share of the query's words in the note, which decides whether it matches by words.
    pub share: f32,
    /// The share with each word weighted by how few notes have it, which ranks the notes that match.
    pub weighted: f32,
    /// The weighted share of the query's words in the note's title; 0 when titles aren't read ([`Weights::title`]).
    pub in_title: f32,
}

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("the query is empty")]
    EmptyQuery,
    #[error("the limit must be at least 1")]
    ZeroLimit,
    #[error(transparent)]
    Index(#[from] rusqlite::Error),
    #[error(transparent)]
    Embedder(#[from] EmbedderError),
    #[error(transparent)]
    VectorScan(#[from] VectorScanError),
}

/// One step of search. Each stage gets the previous stage's candidates and returns its own.
pub trait Stage: Send + Sync {
    fn run(&self, query: &Query, candidates: Vec<Candidate>)
    -> Result<Vec<Candidate>, SearchError>;
}

/// Runs its stages in order, starting from no candidates.
pub struct Pipeline<'a> {
    stages: Vec<Box<dyn Stage + 'a>>,
}

impl<'a> Pipeline<'a> {
    pub fn new(stages: Vec<Box<dyn Stage + 'a>>) -> Self {
        Self { stages }
    }

    pub fn run(&self, query: &Query) -> Result<Vec<Candidate>, SearchError> {
        self.stages
            .iter()
            .try_fold(Vec::new(), |candidates, stage| stage.run(query, candidates))
    }
}
