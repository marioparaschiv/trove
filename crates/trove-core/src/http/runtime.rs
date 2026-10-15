use std::{
    fs::{self, File, TryLockError},
    io::{self, Write},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use thiserror::Error;
use utoipa::ToSchema;

use crate::data_dir::DataDir;

pub const RUNTIME_FILE: &str = "runtime.json";
const LOCK_FILE: &str = "serve.lock";

/// Written to the data folder while `trove serve` runs, so clients can find and call it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeFile {
    /// Port on 127.0.0.1.
    pub port: u16,
    /// Sent as `Authorization: Bearer <token>` on every request.
    pub token: String,
    pub pid: u32,
    pub version: String,
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("failed to lock {path}")]
    Lock {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to read runtime file {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid runtime file {path}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to write runtime file {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to remove runtime file {path}")]
    Remove {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Held by `trove serve` for its whole life, so only one service runs per data folder.
/// The OS releases it when the process dies, which makes a leftover runtime file harmless.
pub(crate) struct ServiceLock {
    _file: File,
}

impl ServiceLock {
    /// Returns `None` when another service holds the lock.
    pub(crate) fn try_acquire(data_dir: &DataDir) -> Result<Option<Self>, RuntimeError> {
        let path = data_dir.path().join(LOCK_FILE);
        let lock_error = |source| RuntimeError::Lock {
            path: path.clone(),
            source,
        };

        let file = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(lock_error)?;

        match file.try_lock() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(source)) => Err(lock_error(source)),
        }
    }
}

impl RuntimeFile {
    /// The file may be left over from a service that crashed; callers find out when they connect.
    pub fn read(data_dir: &DataDir) -> Result<Option<Self>, RuntimeError> {
        let path = data_dir.path().join(RUNTIME_FILE);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(RuntimeError::Read { path, source }),
        };

        serde_json::from_str(&text)
            .map(Some)
            .map_err(|source| RuntimeError::Parse { path, source })
    }

    /// Writes through a temporary file and a rename, so readers never see a partial file.
    /// The temporary file is created readable by the owner only, which keeps the token private.
    pub(crate) fn write(&self, data_dir: &DataDir) -> Result<(), RuntimeError> {
        let path = data_dir.path().join(RUNTIME_FILE);
        let write_error = |source| RuntimeError::Write {
            path: path.clone(),
            source,
        };

        let mut file = NamedTempFile::new_in(data_dir.path()).map_err(write_error)?;
        serde_json::to_writer(&mut file, self).map_err(|error| write_error(error.into()))?;
        file.flush().map_err(write_error)?;
        file.persist(&path)
            .map_err(|error| write_error(error.error))?;

        Ok(())
    }

    pub(crate) fn remove(data_dir: &DataDir) -> Result<(), RuntimeError> {
        let path = data_dir.path().join(RUNTIME_FILE);

        fs::remove_file(&path).map_err(|source| RuntimeError::Remove { path, source })
    }
}
