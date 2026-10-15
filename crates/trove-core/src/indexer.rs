mod chunker;
mod embed;
mod frontmatter;
mod links;
mod neighbours;
mod scan;

use std::sync::Arc;

use rusqlite::Transaction;
use thiserror::Error;

use crate::{
    embedder::{Embedder, EmbedderError},
    index::IndexError,
};

pub use chunker::Chunker;
pub use links::LinkParser;
pub(crate) use chunker::document_title;
pub use scan::{IndexProblem, IndexReport, index_sources};

#[derive(Debug, Error)]
pub enum IndexerError {
    #[error(transparent)]
    Index(#[from] IndexError),
    #[error(transparent)]
    Embedder(#[from] EmbedderError),
}

impl From<rusqlite::Error> for IndexerError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Index(error.into())
    }
}

/// A note whose `files` row is already written, handed to each processor.
pub struct NoteFile<'a> {
    pub id: i64,
    pub title: &'a str,
    pub text: &'a str,
}

/// One independent step of indexing a note. Each processor replaces the rows it owns for the note.
pub trait FileProcessor: Send + Sync {
    fn process(&self, tx: &Transaction<'_>, note: &NoteFile<'_>) -> Result<(), IndexerError>;
}

/// Runs its processors in order over a note, inside the caller's transaction.
pub struct Indexer {
    processors: Vec<Box<dyn FileProcessor>>,
}

impl Indexer {
    pub fn new(processors: Vec<Box<dyn FileProcessor>>) -> Self {
        Self { processors }
    }

    /// Every processor a note goes through.
    pub fn standard(embedder: Arc<Embedder>) -> Self {
        Self::new(vec![Box::new(Chunker::new(embedder)), Box::new(LinkParser)])
    }

    pub fn process(&self, tx: &Transaction<'_>, note: &NoteFile<'_>) -> Result<(), IndexerError> {
        for processor in &self.processors {
            processor.process(tx, note)?;
        }

        Ok(())
    }
}
