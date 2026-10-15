use std::io;

use anyhow::{Context as _, bail};
use serde::{Serialize, de::DeserializeOwned};
use trove_core::{
    data_dir::DataDir,
    http::{ErrorBody, RuntimeFile},
};
use ureq::{Body, RequestBuilder, http::Response};

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

        // A runtime file left by a service that crashed points at a port nobody listens on, or that another program
        // has taken since.
        match service.request("/status") {
            // Trove's `/status` refuses no request with its own token, so a refusal is another program's.
            Ok(response) if response.status().is_client_error() => Ok(None),
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
        read("GET", path, self.request(path))
    }

    pub fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl Serialize,
    ) -> anyhow::Result<T> {
        let body = serde_json::to_vec(body).context("failed to encode the request")?;
        read(
            "POST",
            path,
            self.authorised(self.agent.post(self.url(path)))
                .header("Content-Type", "application/json")
                .send(&body[..]),
        )
    }

    pub fn post_empty<T: DeserializeOwned>(&self, path: &str) -> anyhow::Result<T> {
        read(
            "POST",
            path,
            self.authorised(self.agent.post(self.url(path)))
                .send_empty(),
        )
    }

    pub fn delete<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> anyhow::Result<T> {
        read(
            "DELETE",
            path,
            self.authorised(self.agent.delete(self.url(path)))
                .query_pairs(query.iter().copied())
                .call(),
        )
    }

    fn request(&self, path: &str) -> Result<Response<Body>, ureq::Error> {
        self.authorised(self.agent.get(self.url(path))).call()
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.runtime.port)
    }

    fn authorised<B>(&self, request: RequestBuilder<B>) -> RequestBuilder<B> {
        request.header("Authorization", format!("Bearer {}", self.runtime.token))
    }
}

fn read<T: DeserializeOwned>(
    method: &str,
    path: &str,
    response: Result<Response<Body>, ureq::Error>,
) -> anyhow::Result<T> {
    let mut response = response.with_context(|| format!("{method} {path} failed"))?;
    let body = response
        .body_mut()
        .read_to_string()
        .with_context(|| format!("failed to read the response to {method} {path}"))?;

    if !response.status().is_success() {
        let error: ErrorBody = serde_json::from_str(&body)
            .with_context(|| format!("{method} {path} failed with {}", response.status()))?;
        bail!(
            "{method} {path} failed with {}: {}",
            response.status(),
            error.error
        );
    }

    serde_json::from_str(&body).with_context(|| format!("invalid response to {method} {path}"))
}
