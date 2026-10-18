//! Helpers shared by the tests that drive the `trove` binary.

#![allow(dead_code, reason = "each test binary uses a different part")]

use std::{
    collections::BTreeSet,
    fs,
    io::{BufRead, BufReader, ErrorKind, Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    time::Duration,
};

use serde_json::{Value, json};
use tempfile::TempDir;
use trove_core::model_setup::{
    MODEL_FILE, MODEL_REPO, MODEL_REVISION, MODELS_FOLDER, ModelFile, hf_hub_cache,
};

/// A small vault of notes on clearly separate topics, plus notes in `.obsidian` and `.trash` that must never be
/// indexed. Extend it with notes on new topics; keep each note's topic distinct from the others.
pub const FIXTURE_VAULT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/vault");

/// Two unrelated topics: sailing, and a larger kitchen topic of bread and cheese notes tied together by two hub notes.
pub const CATEGORIES_VAULT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/categories-vault"
);

/// About 300 notes in 25 topic folders, a fifth of them in Spanish, German, Arabic, Hindi, Chinese or Japanese, linked
/// across folders and holding near-topic distractor pairs: the corpus `EVAL_QUERIES` scores search on.
pub const EVAL_VAULT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/eval-vault");

/// The queries `trove eval` scores the eval vault with.
pub const EVAL_QUERIES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/eval-queries.json"
);

/// Notes on coffee, a trip, and three topics unrelated to either.
pub const RELATED_VAULT: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/related-vault");

/// Shares no word with its note in the fixture vault, so only meaning finds it.
pub const PARAPHRASE: &str = "how to look after my bread culture";
pub const PARAPHRASE_NOTE: &str = "Cooking/Sourdough starter.md";

/// Only the fixture vault's notes in `.obsidian` and `.trash` are about this.
pub const HIDDEN_TOPIC: &str = "how to keep honeybee colonies healthy through winter";

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
    copy_of(Path::new(FIXTURE_VAULT))
}

/// A copy of a fixture vault that a test may change, and that indexing may write its store folder into.
pub fn copy_of(vault: &Path) -> TempDir {
    let copy = tempfile::tempdir().unwrap();
    copy_folder(vault, copy.path());

    copy
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

/// The paths of the notes in a fixture vault, relative to it, leaving out hidden folders as indexing does.
pub fn fixture_notes(vault: &Path) -> BTreeSet<String> {
    let mut notes = BTreeSet::new();
    collect_notes(vault, vault, &mut notes);

    notes
}

fn collect_notes(root: &Path, folder: &Path, notes: &mut BTreeSet<String>) {
    for entry in fs::read_dir(folder).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name().to_str().unwrap().starts_with('.') {
            continue;
        }

        let path = entry.path();
        if path.is_dir() {
            collect_notes(root, &path, notes);
        } else if path.extension().is_some_and(|extension| extension == "md") {
            let relative = path.strip_prefix(root).unwrap();
            notes.insert(path_str(relative).to_owned());
        }
    }
}

/// Deletes the index database of `data_dir`, leaving everything else in it.
pub fn remove_index(data_dir: &Path) {
    for name in ["index.sqlite", "index.sqlite-wal", "index.sqlite-shm"] {
        let path = data_dir.join(name);
        if path.exists() {
            fs::remove_file(path).unwrap();
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
    search_with_switches(data_dir, query, &[])
}

/// The JSON `trove search --json` prints with `switches`.
pub fn search_with_switches(data_dir: &Path, query: &str, switches: &[&str]) -> Value {
    let mut args = vec!["search", "--json", query];
    args.extend(switches);

    serde_json::from_str(&run(data_dir, &args)).unwrap()
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
        Self::start_with_env(data_dir, &[])
    }

    /// Starts `trove serve` on `data_dir` with `env` added to its environment.
    pub fn start_with_env(data_dir: TempDir, env: &[(&str, &str)]) -> Self {
        let mut child = trove(data_dir.path())
            .envs(env.iter().copied())
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

    /// Starts `trove serve` on `data_dir` and returns once its startup index run is done, so a later subscription to
    /// the events stream sees none of that run's events.
    pub fn start_indexed(data_dir: TempDir) -> Self {
        let serve = Self::start_in(data_dir);
        let mut events = serve.events();
        if serve.status()["indexing"]["busy"] == true {
            events.until("index_finished");
        }

        serve
    }

    pub fn status(&self) -> Value {
        let (status, body) = self.call("GET", "/status", None);
        assert_eq!(status, 200, "{body}");

        body
    }

    pub fn search(&self, query: &str) -> Value {
        let (status, response) = self.call("POST", "/search", Some(&json!({ "query": query })));
        assert_eq!(status, 200, "{response}");

        response
    }

    pub fn local_host(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    pub fn get(&self, path: &str, host: &str, token: Option<&str>) -> (u16, String) {
        read_response(self.send("GET", path, host, token, None))
    }

    /// Calls the API as a client would and returns the status and the JSON body.
    pub fn call(&self, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
        let (status, body) = self.request(method, path, body);

        (status, serde_json::from_str(&body).unwrap())
    }

    /// Calls the API as a client would and returns the status and the body as it came.
    pub fn request(&self, method: &str, path: &str, body: Option<&Value>) -> (u16, String) {
        let body = body.map(Value::to_string);

        read_response(self.send(
            method,
            path,
            &self.local_host(),
            Some(&self.token),
            body.as_deref(),
        ))
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

    /// Stops the service and hands back its data folder, for a test that starts another service on it.
    pub fn stop_keeping_data(mut self) -> TempDir {
        self.child.kill().unwrap();
        self.child.wait().unwrap();

        std::mem::replace(&mut self.data_dir, tempfile::tempdir().unwrap())
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
    let text = match fs::read_to_string(runtime_path(data_dir)) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return None,
        Err(error) => panic!("reading the runtime file: {error}"),
    };

    Some(serde_json::from_str(&text).unwrap())
}

/// The paths of the notes in a category of a categories response and in its subcategories.
pub fn members(response: &Value, category: &Value) -> BTreeSet<String> {
    let mut members: BTreeSet<String> = category["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|note| note["path"].as_str().unwrap().to_owned())
        .collect();
    for child in response["categories"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|child| child["parent"] == category["id"])
    {
        members.extend(self::members(response, child));
    }

    members
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
