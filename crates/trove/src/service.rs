use std::io;

use anyhow::{Context as _, bail};
use serde::de::DeserializeOwned;
use trove_core::{
    data_dir::DataDir,
    http::{ErrorBody, RuntimeFile},
};

/// A client for the `trove serve` running on this data folder.
pub struct Service {
    runtime: RuntimeFile,
    agent: ureq::Agent,
}

impl Service {
    /// The running service, or `None` when none answers, so the caller can open the index directly.
    pub fn connect(data_dir: &DataDir) -> anyhow::Result<Option<Self>> {
        let Some(runtime) = RuntimeFile::read(data_dir)? else {
            return Ok(None);
        };

        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .proxy(None)
            .build()
            .into();
        let service = Self { runtime, agent };

        // A runtime file left by a service that crashed points at a port nobody listens on.
        match service.request("/status") {
            Ok(_) => Ok(Some(service)),
            Err(ureq::Error::Io(error)) if error.kind() == io::ErrorKind::ConnectionRefused => {
                Ok(None)
            }
            Err(error) => Err(error).context("failed to reach `trove serve`"),
        }
    }

    pub fn runtime(&self) -> &RuntimeFile {
        &self.runtime
    }

    pub fn get<T: DeserializeOwned>(&self, path: &str) -> anyhow::Result<T> {
        let mut response = self
            .request(path)
            .with_context(|| format!("GET {path} failed"))?;
        let body = response
            .body_mut()
            .read_to_string()
            .with_context(|| format!("failed to read the response to GET {path}"))?;

        if !response.status().is_success() {
            let error: ErrorBody = serde_json::from_str(&body)
                .with_context(|| format!("GET {path} failed with {}", response.status()))?;
            bail!(
                "GET {path} failed with {}: {}",
                response.status(),
                error.error
            );
        }

        serde_json::from_str(&body).with_context(|| format!("invalid response to GET {path}"))
    }

    fn request(&self, path: &str) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
        self.agent
            .get(format!("http://127.0.0.1:{}{path}", self.runtime.port))
            .header("Authorization", format!("Bearer {}", self.runtime.token))
            .call()
    }
}
