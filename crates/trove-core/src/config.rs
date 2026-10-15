use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Deserialize;
use thiserror::Error;

use crate::data_dir::DataDir;

pub const CONFIG_FILE: &str = "config.toml";

/// Settings read from `config.toml` in the data folder. A missing file or field takes its default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub model_idle_timeout: Duration,
    pub service_idle_timeout: Duration,
    pub index_debounce: Duration,
    pub store_folder: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model_idle_timeout: Duration::from_secs(180),
            service_idle_timeout: Duration::from_secs(15 * 60),
            index_debounce: Duration::from_secs(30),
            store_folder: ".trove".to_owned(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid config file {path}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("invalid `{field}` in config file {path}: {reason}")]
    Invalid {
        path: PathBuf,
        field: &'static str,
        reason: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    model_idle_timeout: Option<String>,
    service_idle_timeout: Option<String>,
    index_debounce: Option<String>,
    store_folder: Option<String>,
}

impl Config {
    pub fn load(data_dir: &DataDir) -> Result<Self, ConfigError> {
        let path = data_dir.path().join(CONFIG_FILE);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => return Err(ConfigError::Read { path, source }),
        };

        let raw: RawConfig = toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.clone(),
            source,
        })?;

        let defaults = Self::default();

        Ok(Self {
            model_idle_timeout: timeout(
                &path,
                "model_idle_timeout",
                raw.model_idle_timeout,
                defaults.model_idle_timeout,
            )?,
            service_idle_timeout: timeout(
                &path,
                "service_idle_timeout",
                raw.service_idle_timeout,
                defaults.service_idle_timeout,
            )?,
            index_debounce: timeout(
                &path,
                "index_debounce",
                raw.index_debounce,
                defaults.index_debounce,
            )?,
            store_folder: match raw.store_folder {
                Some(name) => store_folder(&path, name)?,
                None => defaults.store_folder,
            },
        })
    }
}

fn invalid(path: &Path, field: &'static str, reason: String) -> ConfigError {
    ConfigError::Invalid {
        path: path.to_path_buf(),
        field,
        reason,
    }
}

fn timeout(
    path: &Path,
    field: &'static str,
    value: Option<String>,
    default: Duration,
) -> Result<Duration, ConfigError> {
    let Some(value) = value else {
        return Ok(default);
    };

    let duration = humantime::parse_duration(&value).map_err(|error| {
        invalid(
            path,
            field,
            format!("{value:?} is not a duration ({error})"),
        )
    })?;

    if duration.is_zero() {
        return Err(invalid(path, field, "must be greater than zero".to_owned()));
    }

    Ok(duration)
}

/// The store folder syncs between devices, so its name must be valid on every OS.
fn store_folder(path: &Path, name: String) -> Result<String, ConfigError> {
    const FORBIDDEN: &[char] = &['/', '\\', '<', '>', ':', '"', '|', '?', '*'];

    let reason = if name.is_empty() {
        "must not be empty".to_owned()
    } else if name == "." || name == ".." {
        format!("{name:?} is not a folder name")
    } else if let Some(char) = name
        .chars()
        .find(|char| FORBIDDEN.contains(char) || char.is_control())
    {
        format!("{name:?} contains {char:?}; it must be a single folder name valid on every OS")
    } else if name.ends_with(['.', ' ']) {
        format!("{name:?} ends with a dot or space, which Windows does not allow")
    } else {
        return Ok(name);
    };

    Err(invalid(path, "store_folder", reason))
}
