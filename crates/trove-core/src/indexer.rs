mod chunker;

use rusqlite::Transaction;

use crate::index::{Index, IndexError};

pub use chunker::Chunker;

/// A note whose `files` row is already written, handed to each processor.
pub struct NoteFile<'a> {
    pub id: i64,
    pub title: &'a str,
    pub text: &'a str,
}

/// One independent step of indexing a note. Each processor replaces the rows it owns for the note.
pub trait FileProcessor: Send + Sync {
    fn process(&self, tx: &Transaction<'_>, note: &NoteFile<'_>) -> Result<(), IndexError>;
}

/// Runs its processors in order over a note, in one transaction.
pub struct Indexer {
    processors: Vec<Box<dyn FileProcessor>>,
}

impl Indexer {
    pub fn new(processors: Vec<Box<dyn FileProcessor>>) -> Self {
        Self { processors }
    }

    pub fn process(&self, index: &mut Index, note: &NoteFile<'_>) -> Result<(), IndexError> {
        let tx = index.transaction()?;
        for processor in &self.processors {
            processor.process(&tx, note)?;
        }

        Ok(tx.commit()?)
    }
}

impl Default for Indexer {
    fn default() -> Self {
        Self::new(vec![Box::new(Chunker)])
    }
}
