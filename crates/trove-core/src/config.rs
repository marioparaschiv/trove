use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Deserialize;
use thiserror::Error;

use crate::{data_dir::DataDir, model_setup::ModelFile};

pub const CONFIG_FILE: &str = "config.toml";

/// Settings read from `config.toml` in the data folder. A missing file or field takes its default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub model_idle_timeout: Duration,
    pub service_idle_timeout: Duration,
    pub index_debounce: Duration,
    pub store_folder: String,
    /// The first wait between download retries; later ones double up to a minute.
    pub download_retry_delay: Duration,
    /// The model `trove setup` installs. Defaults to the pinned EmbeddingGemma 2 file.
    pub model: ModelFile,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model_idle_timeout: Duration::from_secs(180),
            service_idle_timeout: Duration::from_secs(15 * 60),
            index_debounce: Duration::from_secs(30),
            store_folder: ".trove".to_owned(),
            download_retry_delay: Duration::from_secs(2),
            model: ModelFile::pinned(),
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
    download_retry_delay: Option<String>,
    model: Option<RawModel>,
}

/// Every field is required: a model is identified by all of them together.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawModel {
    repo: String,
    revision: String,
    file: String,
    size: u64,
    sha256: String,
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
            download_retry_delay: timeout(
                &path,
                "download_retry_delay",
                raw.download_retry_delay,
                defaults.download_retry_delay,
            )?,
            model: match raw.model {
                Some(model) => model_file(&path, model)?,
                None => defaults.model,
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

fn model_file(path: &Path, raw: RawModel) -> Result<ModelFile, ConfigError> {
    let is_name_char = |char: char| char.is_ascii_alphanumeric() || "._-".contains(char);
    let is_name = |name: &str| {
        !name.is_empty() && name != "." && name != ".." && name.chars().all(is_name_char)
    };
    let is_lower_hex = |text: &str, len: usize| {
        text.len() == len
            && text
                .chars()
                .all(|char| matches!(char, '0'..='9' | 'a'..='f'))
    };

    if !matches!(raw.repo.split_once('/'), Some((owner, name)) if is_name(owner) && is_name(name)) {
        return Err(invalid(
            path,
            "model.repo",
            format!("{:?} is not an `owner/name` repository", raw.repo),
        ));
    }

    if !is_lower_hex(&raw.revision, 40) {
        return Err(invalid(
            path,
            "model.revision",
            format!(
                "{:?} is not a 40-character lowercase hex commit",
                raw.revision
            ),
        ));
    }

    if !is_name(&raw.file) {
        return Err(invalid(
            path,
            "model.file",
            format!("{:?} is not a plain file name", raw.file),
        ));
    }

    if raw.size == 0 {
        return Err(invalid(
            path,
            "model.size",
            "must be greater than zero".to_owned(),
        ));
    }

    if !is_lower_hex(&raw.sha256, 64) {
        return Err(invalid(
            path,
            "model.sha256",
            format!(
                "{:?} is not a 64-character lowercase hex SHA-256",
                raw.sha256
            ),
        ));
    }

    Ok(ModelFile {
        repo: raw.repo,
        revision: raw.revision,
        file: raw.file,
        size: raw.size,
        sha256: raw.sha256,
    })
}
