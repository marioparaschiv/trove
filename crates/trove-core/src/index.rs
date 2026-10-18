pub(crate) mod keyword_text;
mod sources;
mod stats;
mod vector_scan;
mod vectors;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use rusqlite::{Connection, Transaction, TransactionBehavior};
use thiserror::Error;

use crate::{data_dir::DataDir, graph::VaultGraph};
use vector_scan::VectorScan;

pub use sources::{Source, SourceError};
pub use stats::IndexStats;
pub use vector_scan::{VECTOR_SCAN_FILE, VectorScanError};
pub(crate) use vectors::SectionVector;

pub const INDEX_FILE: &str = "index.sqlite";
pub const SCHEMA_VERSION: u32 = 2;

const SCHEMA: &str = include_str!("index/schema.sql");
const CHUNKS_FTS: &str = include_str!("index/chunks_fts.sql");

/// The local index: a rebuildable SQLite cache in the data folder.
pub struct Index {
    conn: Connection,
    /// The graph search reranks with, and the index version it was loaded at.
    graph: Option<(Version, Arc<VaultGraph>)>,
    vector_scan_path: PathBuf,
    /// The mapped vector scan file, and the index version it was last checked at.
    vector_scan: Option<(Version, VectorScan)>,
}

/// Changes whenever anything writes to the index, through this connection or another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Version {
    /// Rows this connection has changed since it opened.
    total_changes: u64,
    /// SQLite's `data_version`, which changes when another connection commits.
    data_version: i64,
}

#[derive(Debug, Error)]
pub enum IndexError {
    #[error("failed to open index {path}")]
    Open {
        path: PathBuf,
        #[source]
        source: rusqlite::Error,
    },
    #[error(
        "index {path} has schema version {found}, but this Trove only reads version {expected}"
    )]
    UnsupportedSchema {
        path: PathBuf,
        found: u32,
        expected: u32,
    },
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    VectorScan(#[from] VectorScanError),
}

impl Index {
    /// Opens the index in `data_dir`, creating the schema if the index is new.
    pub fn open(data_dir: &DataDir) -> Result<Self, IndexError> {
        let path = data_dir.path().join(INDEX_FILE);
        let open_error = |source| IndexError::Open {
            path: path.clone(),
            source,
        };

        let mut conn = Connection::open(&path).map_err(open_error)?;
        conn.pragma_update(None, "foreign_keys", true)
            .map_err(open_error)?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(open_error)?;

        // Immediate, so a CLI and a service opening a new index at once don't both create it.
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        match read_schema_version(&tx)? {
            None => {
                tx.execute_batch(SCHEMA)?;
                tx.execute_batch(CHUNKS_FTS)?;
                tx.execute(
                    "INSERT INTO meta (key, value) VALUES ('schema_version', ?1), ('device_id', ?2)",
                    (SCHEMA_VERSION, uuid::Uuid::new_v4().to_string()),
                )?;
            }
            Some(1) => migrate_to_keyword_text(&tx)?,
            Some(SCHEMA_VERSION) => {}
            Some(found) => {
                return Err(IndexError::UnsupportedSchema {
                    path,
                    found,
                    expected: SCHEMA_VERSION,
                });
            }
        }
        tx.commit()?;

        Ok(Self {
            conn,
            graph: None,
            vector_scan_path: data_dir.path().join(VECTOR_SCAN_FILE),
            vector_scan: None,
        })
    }

    pub fn schema_version(&self) -> Result<u32, IndexError> {
        Ok(self.meta("schema_version")?)
    }

    pub fn device_id(&self) -> Result<String, IndexError> {
        Ok(self.meta("device_id")?)
    }

    pub(crate) fn transaction(&mut self) -> rusqlite::Result<Transaction<'_>> {
        self.conn.transaction()
    }

    pub(crate) fn connection(&self) -> &Connection {
        &self.conn
    }

    /// The whole vault graph, loaded again only when the index has changed since it was last loaded.
    pub(crate) fn graph(&mut self) -> rusqlite::Result<Arc<VaultGraph>> {
        let version = self.version()?;
        if let Some((loaded_at, graph)) = &self.graph
            && *loaded_at == version
        {
            return Ok(Arc::clone(graph));
        }

        let graph = Arc::new(VaultGraph::load(&self.conn)?);
        self.graph = Some((version, Arc::clone(&graph)));

        Ok(graph)
    }

    fn version(&self) -> rusqlite::Result<Version> {
        Ok(Version {
            total_changes: self.conn.total_changes(),
            data_version: self
                .conn
                .pragma_query_value(None, "data_version", |row| row.get(0))?,
        })
    }

    fn meta<T: rusqlite::types::FromSql>(&self, key: &str) -> rusqlite::Result<T> {
        self.conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
                row.get(0)
            })
    }
}

/// Locks an index shared between threads.
pub fn lock(index: &Mutex<Index>) -> MutexGuard<'_, Index> {
    // A panic mid-query leaves the connection usable: SQLite rolls back the open transaction.
    index.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Schema 1 to 2: keyword search with DigUp's tokenizer over normalised text (`keyword_text`) and a trigram table
/// beside it, and chunks of mostly digits or symbols marked and without a vector. The keyword tables are made again
/// from the old table's rows, so no note is read and no other vector changes.
fn migrate_to_keyword_text(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    let rows: Vec<(i64, String, String, String)> = tx
        .prepare("SELECT rowid, title, heading_path, text FROM chunks_fts")?
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<rusqlite::Result<_>>()?;

    tx.execute_batch(
        "DROP TRIGGER chunks_fts_delete;
         DROP TABLE chunks_fts;
         ALTER TABLE chunks ADD COLUMN has_words INTEGER NOT NULL DEFAULT 1;",
    )?;
    tx.execute_batch(CHUNKS_FTS)?;
    let mut mark_wordless = tx.prepare("UPDATE chunks SET has_words = 0 WHERE id = ?1")?;
    for (chunk_id, title, heading_path, text) in rows {
        keyword_text::insert(tx, chunk_id, &title, &heading_path, &text)?;
        if !keyword_text::has_words(&text) {
            mark_wordless.execute([chunk_id])?;
        }
    }
    // Chunks with the same text hash have the same text.
    tx.execute(
        "DELETE FROM vectors WHERE text_hash IN (SELECT text_hash FROM chunks WHERE NOT has_words)",
        [],
    )?;

    tx.execute(
        "UPDATE meta SET value = ?1 WHERE key = 'schema_version'",
        [SCHEMA_VERSION],
    )?;

    Ok(())
}

fn read_schema_version(conn: &Connection) -> rusqlite::Result<Option<u32>> {
    let has_meta: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'meta')",
        [],
        |row| row.get(0),
    )?;
    if !has_meta {
        return Ok(None);
    }

    conn.query_row(
        "SELECT value FROM meta WHERE key = 'schema_version'",
        [],
        |row| row.get(0),
    )
    .map(Some)
}
