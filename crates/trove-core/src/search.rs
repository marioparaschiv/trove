mod collapse;
mod fusion;
mod graph;
mod meaning;
mod request;
mod words;

use std::{collections::BTreeSet, fmt};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::ToSchema;

use crate::embedder::EmbedderError;

pub use collapse::CollapseToNotes;
pub use fusion::Fusion;
pub use graph::GraphRerank;
pub use meaning::Meaning;
pub(crate) use request::results;
pub use request::{DEFAULT_LIMIT, SearchRequest, SearchResponse, SearchResult, search};
pub use words::Words;

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
