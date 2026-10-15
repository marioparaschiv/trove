use thiserror::Error;

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
    #[error(transparent)]
    Index(#[from] rusqlite::Error),
}

/// One step of search. Each stage gets the previous stage's candidates and returns its own.
pub trait Stage: Send + Sync {
    fn run(&self, query: &Query, candidates: Vec<Candidate>)
    -> Result<Vec<Candidate>, SearchError>;
}

/// Runs its stages in order, starting from no candidates.
pub struct Pipeline {
    stages: Vec<Box<dyn Stage>>,
}

impl Pipeline {
    pub fn new(stages: Vec<Box<dyn Stage>>) -> Self {
        Self { stages }
    }

    pub fn run(&self, query: &Query) -> Result<Vec<Candidate>, SearchError> {
        self.stages
            .iter()
            .try_fold(Vec::new(), |candidates, stage| stage.run(query, candidates))
    }
}
