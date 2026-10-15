//! Helpers shared by the tests that drive the `trove` binary.

#![allow(dead_code, reason = "each test binary uses a different part")]

use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    time::Duration,
};

use serde_json::Value;
use tempfile::TempDir;
use trove_core::model_setup::{
    MODEL_FILE, MODEL_REPO, MODEL_REVISION, MODELS_FOLDER, ModelFile, hf_hub_cache,
};

/// A small vault of notes on clearly separate topics, plus notes in `.obsidian` and `.trash` that must never be
/// indexed. Extend it with notes on new topics; keep each note's topic distinct from the others.
pub const FIXTURE_VAULT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/vault");

pub fn trove(data_dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_trove"));
    command.env("TROVE_DATA_DIR", data_dir);

    command
}

/// Runs `trove` with `args` and returns its stdout, failing the test with its stderr if it fails.
pub fn run(data_dir: &Path, args: &[&str]) -> String {
    stdout(&trove(data_dir).args(args).output().unwrap())
}

pub fn stdout(output: &Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout.clone()).unwrap()
}

/// A temporary data folder holding the pinned model, linked from the Hugging Face cache (copied where it can't be).
pub fn data_dir_with_model() -> TempDir {
    let fetch = format!("hf download {MODEL_REPO} {MODEL_FILE} --revision {MODEL_REVISION}");
    let hub = hf_hub_cache()
        .unwrap_or_else(|| panic!("no Hugging Face cache folder (no home folder): run `{fetch}`"));
    let cached = ModelFile::pinned().path_in_hf_cache(&hub);
    // The cache's snapshot entry may be a symbolic link to the blob, and a hard link would point at the link.
    let cached = fs::canonicalize(&cached)
        .unwrap_or_else(|_| panic!("model not found at {}: run `{fetch}`", cached.display()));

    let data_dir = tempfile::tempdir().unwrap();
    let models = data_dir.path().join(MODELS_FOLDER);
    fs::create_dir(&models).unwrap();
    let target = models.join(MODEL_FILE);
    if fs::hard_link(&cached, &target).is_err() {
        fs::copy(&cached, &target).unwrap();
    }

    data_dir
}

/// A copy of the fixture vault that a test may change.
pub fn vault_copy() -> TempDir {
    let vault = tempfile::tempdir().unwrap();
    copy_folder(Path::new(FIXTURE_VAULT), vault.path());

    vault
}

fn copy_folder(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            fs::create_dir(&target).unwrap();
            copy_folder(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A data folder with the model and `vault` registered and indexed, without a service.
pub fn indexed(vault: &Path) -> TempDir {
    let data_dir = data_dir_with_model();
    run(data_dir.path(), &["sources", "add", path_str(vault)]);
    run(data_dir.path(), &["index"]);

    data_dir
}

pub fn path_str(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// The JSON `trove search --json` prints.
pub fn search(data_dir: &Path, query: &str) -> Value {
    serde_json::from_str(&run(data_dir, &["search", "--json", query])).unwrap()
}

/// The paths of the notes in a search response, best first.
pub fn result_paths(response: &Value) -> Vec<&str> {
    response["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|result| result["path"].as_str().unwrap())
        .collect()
}

pub struct Serve {
    pub child: Child,
    pub data_dir: TempDir,
    pub port: u16,
    pub token: String,
}

impl Serve {
    /// Starts `trove serve` on a new data folder that goes idle after `idle_timeout`.
    pub fn start(idle_timeout: &str) -> Self {
        let data_dir = tempfile::tempdir().unwrap();
        fs::write(
            data_dir.path().join("config.toml"),
            format!("service_idle_timeout = \"{idle_timeout}\"\n"),
        )
        .unwrap();

        Self::start_in(data_dir)
    }

    /// Starts `trove serve` on `data_dir` and returns once it reports that it listens.
    pub fn start_in(data_dir: TempDir) -> Self {
        let mut child = trove(data_dir.path())
            .arg("serve")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();

        let mut line = String::new();
        BufReader::new(child.stdout.as_mut().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert!(line.starts_with("listening on "), "{line:?}");

        let runtime = read_runtime(&data_dir).expect("runtime file written on start");
        assert_eq!(runtime["pid"], child.id());
        assert_eq!(runtime["version"], env!("CARGO_PKG_VERSION"));

        Self {
            port: runtime["port"].as_u64().unwrap().try_into().unwrap(),
            token: runtime["token"].as_str().unwrap().to_owned(),
            child,
            data_dir,
        }
    }

    pub fn local_host(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    pub fn get(&self, path: &str, host: &str, token: Option<&str>) -> (u16, String) {
        read_response(self.send("GET", path, host, token, None))
    }

    /// Calls the API as a client would and returns the status and the JSON body.
    pub fn call(&self, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
        let body = body.map(Value::to_string);
        let (status, body) = read_response(self.send(
            method,
            path,
            &self.local_host(),
            Some(&self.token),
            body.as_deref(),
        ));

        (status, serde_json::from_str(&body).unwrap())
    }

    pub fn send(
        &self,
        method: &str,
        path: &str,
        host: &str,
        token: Option<&str>,
        body: Option<&str>,
    ) -> TcpStream {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        let auth = token
            .map(|token| format!("Authorization: Bearer {token}\r\n"))
            .unwrap_or_default();
        let body = body
            .map(|body| {
                format!(
                    "Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                )
            })
            .unwrap_or_else(|| "Content-Length: 0\r\n\r\n".to_owned());
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: {host}\r\n{auth}Connection: close\r\n{body}"
        )
        .unwrap();

        stream
    }

    /// Subscribes to `GET /events`. The subscription is open once this returns, so no later event is missed.
    pub fn events(&self) -> Events {
        let stream = self.send(
            "GET",
            "/events",
            &self.local_host(),
            Some(&self.token),
            None,
        );
        // Turns an event that never comes into a failure instead of a hung test.
        stream
            .set_read_timeout(Some(Duration::from_secs(300)))
            .unwrap();

        let mut reader = BufReader::new(stream);
        let mut status_line = String::new();
        reader.read_line(&mut status_line).unwrap();
        assert!(status_line.starts_with("HTTP/1.1 200"), "{status_line}");
        let mut line = String::new();
        while line != "\r\n" {
            line.clear();
            reader.read_line(&mut line).unwrap();
        }

        Events(reader)
    }

    pub fn stop(mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }
}

// A test that fails before `stop` would otherwise leave the service running until its idle timeout.
impl Drop for Serve {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            self.child.kill().unwrap();
            self.child.wait().unwrap();
        }
    }
}

/// An open `GET /events` stream.
pub struct Events(BufReader<TcpStream>);

impl Events {
    /// The data of the next event.
    pub fn next(&mut self) -> Value {
        let mut line = String::new();
        loop {
            line.clear();
            assert_ne!(self.0.read_line(&mut line).unwrap(), 0, "the stream ended");
            if let Some(data) = line.strip_prefix("data: ") {
                return serde_json::from_str(data).unwrap();
            }
        }
    }

    /// Every event up to and including the next one of type `kind`.
    pub fn until(&mut self, kind: &str) -> Vec<Value> {
        let mut events = Vec::new();
        loop {
            let event = self.next();
            let done = event["type"] == kind;
            events.push(event);
            if done {
                return events;
            }
        }
    }
}

fn read_response(mut stream: TcpStream) -> (u16, String) {
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();

    let status = response[9..12].parse().unwrap();
    let (_, body) = response.split_once("\r\n\r\n").unwrap();

    (status, body.to_owned())
}

pub fn runtime_path(data_dir: &TempDir) -> PathBuf {
    data_dir.path().join("runtime.json")
}

pub fn read_runtime(data_dir: &TempDir) -> Option<Value> {
    let text = fs::read_to_string(runtime_path(data_dir)).ok()?;

    Some(serde_json::from_str(&text).unwrap())
}

/// Encodes `text` for a URL query string.
pub fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                char::from(byte).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}
