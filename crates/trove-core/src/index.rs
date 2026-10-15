use std::path::PathBuf;

use rusqlite::{Connection, Transaction, TransactionBehavior};
use thiserror::Error;

use crate::data_dir::DataDir;

pub const INDEX_FILE: &str = "index.sqlite";
pub const SCHEMA_VERSION: u32 = 1;

const SCHEMA: &str = include_str!("index/schema.sql");

/// The local index: a rebuildable SQLite cache in the data folder.
pub struct Index {
    conn: Connection,
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
                tx.execute(
                    "INSERT INTO meta (key, value) VALUES ('schema_version', ?1), ('device_id', ?2)",
                    (SCHEMA_VERSION, uuid::Uuid::new_v4().to_string()),
                )?;
            }
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

        Ok(Self { conn })
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

    fn meta<T: rusqlite::types::FromSql>(&self, key: &str) -> rusqlite::Result<T> {
        self.conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
                row.get(0)
            })
    }
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
