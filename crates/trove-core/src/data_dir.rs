use std::{
    fs, io,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use thiserror::Error;

pub const DATA_DIR_ENV: &str = "TROVE_DATA_DIR";

/// The per-device folder holding the local index, config, model and runtime file.
#[derive(Debug, Clone)]
pub struct DataDir(PathBuf);

#[derive(Debug, Error)]
pub enum DataDirError {
    #[error("TROVE_DATA_DIR must be an absolute path, got {0:?}")]
    NotAbsolute(PathBuf),
    #[error("no home folder found to place the data folder in")]
    NoHome,
    #[error("failed to create data folder {path}")]
    Create {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl DataDir {
    /// Uses `TROVE_DATA_DIR` when set, otherwise the OS's local data folder.
    pub fn resolve() -> Result<Self, DataDirError> {
        let path = match std::env::var_os(DATA_DIR_ENV) {
            Some(path) => {
                let path = PathBuf::from(path);
                if !path.is_absolute() {
                    return Err(DataDirError::NotAbsolute(path));
                }

                path
            }
            None => ProjectDirs::from("", "", "Trove")
                .ok_or(DataDirError::NoHome)?
                .data_local_dir()
                .to_path_buf(),
        };

        Self::at(path)
    }

    pub fn at(path: PathBuf) -> Result<Self, DataDirError> {
        fs::create_dir_all(&path).map_err(|source| DataDirError::Create {
            path: path.clone(),
            source,
        })?;

        Ok(Self(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}
