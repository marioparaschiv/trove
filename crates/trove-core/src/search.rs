mod collapse;
mod meaning;
mod request;

use thiserror::Error;

use crate::embedder::EmbedderError;

pub use collapse::CollapseToNotes;
pub use meaning::Meaning;
pub use request::{DEFAULT_LIMIT, SearchRequest, SearchResponse, SearchResult, search};

pub struct Query {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub chunk_id: i64,
    pub score: f32,
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
