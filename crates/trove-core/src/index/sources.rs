use std::path::{Path, PathBuf};

use rusqlite::{ErrorCode, OptionalExtension};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::ToSchema;

use super::{Index, IndexError};

/// A registered folder or single file that Trove indexes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Source {
    pub id: i64,
    /// Absolute path of the folder or file on this device. Note paths are relative to the folder, or for a file to the
    /// folder holding it.
    pub path: String,
}

#[derive(Debug, Error)]
pub enum SourceError {
    #[error("source path must be absolute, got {0:?}")]
    NotAbsolute(PathBuf),
    #[error("source path must be valid Unicode, got {0:?}")]
    NotUnicode(PathBuf),
    #[error("{0:?} is not a folder or a file")]
    NotAFolderOrFile(PathBuf),
    #[error("{0:?} is already a source")]
    AlreadyAdded(String),
    #[error("{0:?} is not a source")]
    NotFound(String),
    #[error(transparent)]
    Index(#[from] IndexError),
}

impl From<rusqlite::Error> for SourceError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Index(error.into())
    }
}

impl Index {
    pub fn sources(&self) -> Result<Vec<Source>, IndexError> {
        let mut statement = self
            .conn
            .prepare("SELECT id, path FROM sources ORDER BY path")?;
        let sources = statement
            .query_map([], |row| {
                Ok(Source {
                    id: row.get(0)?,
                    path: row.get(1)?,
                })
            })?
            .collect::<Result<_, _>>()?;

        Ok(sources)
    }

    /// The id of the source at `path`, as registered.
    pub(crate) fn source_id(&self, path: &str) -> rusqlite::Result<Option<i64>> {
        self.conn
            .query_row("SELECT id FROM sources WHERE path = ?1", [path], |row| {
                row.get(0)
            })
            .optional()
    }

    /// Registers the folder or file at `path`, which must be absolute.
    pub fn add_source(&mut self, path: &Path) -> Result<Source, SourceError> {
        let path = normalised(path)?;
        if !(Path::new(&path).is_dir() || Path::new(&path).is_file()) {
            return Err(SourceError::NotAFolderOrFile(path.into()));
        }

        let inserted = self.conn.query_row(
            "INSERT INTO sources (path) VALUES (?1) RETURNING id",
            [&path],
            |row| row.get(0),
        );

        match inserted {
            Ok(id) => Ok(Source { id, path }),
            Err(rusqlite::Error::SqliteFailure(error, _))
                if error.code == ErrorCode::ConstraintViolation =>
            {
                Err(SourceError::AlreadyAdded(path))
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Unregisters the folder or file at `path`, which must be absolute, and drops everything indexed from it.
    pub fn remove_source(&mut self, path: &Path) -> Result<Source, SourceError> {
        let path = normalised(path)?;
        let id = self
            .conn
            .query_row(
                "DELETE FROM sources WHERE path = ?1 RETURNING id",
                [&path],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| SourceError::NotFound(path.clone()))?;

        Ok(Source { id, path })
    }
}

/// The folder a source's note paths are relative to, which also holds its store folder: the source itself, or the
/// folder holding a file source.
pub fn source_root(source: &Path) -> &Path {
    match source.parent() {
        Some(parent) if source.is_file() => parent,
        _ => source,
    }
}

/// The one spelling of `path` that sources are stored and looked up by: no trailing separator, no `.` parts.
fn normalised(path: &Path) -> Result<String, SourceError> {
    if !path.is_absolute() {
        return Err(SourceError::NotAbsolute(path.to_path_buf()));
    }

    let path: PathBuf = path.components().collect();
    path.into_os_string()
        .into_string()
        .map_err(|path| SourceError::NotUnicode(path.into()))
}
