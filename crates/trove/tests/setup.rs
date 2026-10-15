use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Output, Stdio},
    sync::{Arc, Mutex},
    thread,
};

use serde_json::Value;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const REPO: &str = "test-org/fixture-GGUF";
const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";
const FILE: &str = "fixture-Q8_0.gguf";
const CUT_AT: usize = 100_000;

fn fixture() -> Vec<u8> {
    (0..200_000u32).map(|i| (i * 31 % 251) as u8).collect()
}

#[derive(Debug, Clone, PartialEq)]
struct Request {
    path: String,
    range: Option<String>,
}

#[derive(Clone, Copy)]
enum Reply {
    Full,
    /// Promises the whole body, sends `CUT_AT` bytes of it, then hangs up.
    Cut,
    /// Promises the whole body, sends `CUT_AT` bytes of it, then keeps the connection open without sending more.
    Stall,
    Corrupt,
}

/// The download host: `/resolve/` redirects to `/cdn/`, which serves the fixture with `Range` support. The
/// n-th `/cdn/` request gets `replies[n]` (the last one repeats).
struct Server {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<Request>>>,
}

impl Server {
    fn start(replies: Vec<Reply>) -> Self {
        Self::on(TcpListener::bind("127.0.0.1:0").unwrap(), replies)
    }

    fn on(listener: TcpListener, replies: Vec<Reply>) -> Self {
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&requests);

        thread::spawn(move || {
            let fixture = fixture();
            let mut cdn_requests = 0;
            let mut stalled = Vec::new();

            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let request = read_request(&stream);
                log.lock().unwrap().push(request.clone());

                if request.path.contains("/resolve/") {
                    let location = format!("http://{addr}/cdn/{FILE}");
                    write!(
                        stream,
                        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .unwrap();
                } else {
                    let reply = replies[cdn_requests.min(replies.len() - 1)];
                    cdn_requests += 1;
                    send_fixture(&mut stream, &fixture, request.range.as_deref(), reply);
                    if let Reply::Stall = reply {
                        stalled.push(stream);
                        continue;
                    }
                }

                stream.shutdown(Shutdown::Write).unwrap();
            }
        });

        Self { addr, requests }
    }

    fn endpoint(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }
}

fn read_request(stream: &TcpStream) -> Request {
    let mut lines = BufReader::new(stream).lines();
    let request_line = lines.next().unwrap().unwrap();
    let path = request_line.split(' ').nth(1).unwrap().to_owned();
    let mut range = None;

    for line in lines {
        let line = line.unwrap();
        if line.is_empty() {
            break;
        }

        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("range")
        {
            range = Some(value.trim().to_owned());
        }
    }

    Request { path, range }
}

fn send_fixture(stream: &mut TcpStream, fixture: &[u8], range: Option<&str>, reply: Reply) {
    let start = range.map_or(0, |range| {
        range
            .strip_prefix("bytes=")
            .and_then(|range| range.strip_suffix('-'))
            .unwrap()
            .parse()
            .unwrap()
    });

    let mut body = fixture[start..].to_vec();
    if let Reply::Corrupt = reply {
        body[0] ^= 0xff;
    }

    let status = if range.is_some() {
        format!(
            "206 Partial Content\r\nContent-Range: bytes {start}-{}/{}",
            fixture.len() - 1,
            fixture.len()
        )
    } else {
        "200 OK".to_owned()
    };

    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .unwrap();

    let sent = match reply {
        Reply::Cut | Reply::Stall => &body[..CUT_AT],
        Reply::Full | Reply::Corrupt => &body,
    };
    stream.write_all(sent).unwrap();
}

struct Setup {
    _temp: TempDir,
    data_dir: PathBuf,
    hf_cache: PathBuf,
}

impl Setup {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let data_dir = temp.path().join("data");
        let hf_cache = temp.path().join("hf-cache");
        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&hf_cache).unwrap();

        let fixture = fixture();
        let config = format!(
            "download_retry_delay = \"1ms\"\ndownload_idle_timeout = \"500ms\"\n\n[model]\nrepo = \"{REPO}\"\nrevision = \"{REVISION}\"\nfile = \"{FILE}\"\nsize = {}\nsha256 = \"{}\"\n",
            fixture.len(),
            hex::encode(Sha256::digest(&fixture)),
        );
        fs::write(data_dir.join("config.toml"), config).unwrap();

        Self {
            _temp: temp,
            data_dir,
            hf_cache,
        }
    }

    fn command(&self, endpoint: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_trove"));
        command
            .args(["setup", "--json"])
            .env("TROVE_DATA_DIR", &self.data_dir)
            .env("HF_HUB_CACHE", &self.hf_cache)
            .env("HF_ENDPOINT", endpoint)
            .env_remove("HF_HOME");
        command
    }

    fn run(&self, endpoint: &str) -> (Output, Vec<Value>) {
        let output = self.command(endpoint).output().unwrap();
        let events = String::from_utf8(output.stdout.clone())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        (output, events)
    }

    fn model(&self) -> PathBuf {
        self.data_dir.join("models").join(FILE)
    }

    fn partial(&self) -> PathBuf {
        self.data_dir.join("models").join(format!("{FILE}.partial"))
    }
}

/// Event kinds in order, leaving out `progress`, whose count depends on timing.
fn kinds(events: &[Value]) -> Vec<&str> {
    events
        .iter()
        .map(|event| event["event"].as_str().unwrap())
        .filter(|kind| *kind != "progress")
        .collect()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "setup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn resolve_path() -> String {
    format!("/{REPO}/resolve/{REVISION}/{FILE}")
}

fn cdn_path() -> String {
    format!("/cdn/{FILE}")
}

fn request(path: String, range: Option<&str>) -> Request {
    Request {
        path,
        range: range.map(str::to_owned),
    }
}

#[test]
fn downloads_the_model_and_reports_each_step() {
    let server = Server::start(vec![Reply::Full]);
    let setup = Setup::new();

    let (output, events) = setup.run(&server.endpoint());

    assert_success(&output);
    assert_eq!(kinds(&events), ["file", "verifying", "done"]);
    assert_eq!(events[0]["from"], "download");
    assert_eq!(events[0]["name"], FILE);
    assert_eq!(events[1]["event"], "progress");
    assert_eq!(events[1]["total"], fixture().len());
    assert_eq!(
        events.last().unwrap()["path"],
        setup.model().to_str().unwrap()
    );
    assert_eq!(fs::read(setup.model()).unwrap(), fixture());
    assert!(!setup.partial().exists());
    assert_eq!(
        server.requests(),
        [request(resolve_path(), None), request(cdn_path(), None)]
    );
}

/// Runs setup against a server whose first transfer goes wrong after `CUT_AT` bytes and checks that the retry
/// resumes there, sending `Range` both to the hub and, after its redirect, to the CDN. Returns the events.
fn assert_resumes_after(first: Reply) -> Vec<Value> {
    let server = Server::start(vec![first, Reply::Full]);
    let setup = Setup::new();

    let (output, events) = setup.run(&server.endpoint());

    assert_success(&output);
    assert_eq!(kinds(&events), ["file", "retry", "verifying", "done"]);
    assert_eq!(fs::read(setup.model()).unwrap(), fixture());

    let range = format!("bytes={CUT_AT}-");
    assert_eq!(
        server.requests(),
        [
            request(resolve_path(), None),
            request(cdn_path(), None),
            request(resolve_path(), Some(&range)),
            request(cdn_path(), Some(&range)),
        ]
    );

    events
}

#[test]
fn an_interrupted_download_resumes_from_the_partial_size() {
    assert_resumes_after(Reply::Cut);
}

#[test]
fn a_stalled_download_times_out_and_resumes_from_the_partial_size() {
    let events = assert_resumes_after(Reply::Stall);

    let retry = events
        .iter()
        .find(|event| event["event"] == "retry")
        .unwrap();
    assert!(
        retry["message"]
            .as_str()
            .unwrap()
            .contains("no data for 500ms")
    );
}

#[test]
fn a_matching_file_in_the_hugging_face_cache_is_copied_without_a_request() {
    let server = Server::start(vec![Reply::Full]);
    let setup = Setup::new();
    let snapshot = setup
        .hf_cache
        .join("models--test-org--fixture-GGUF/snapshots")
        .join(REVISION);
    fs::create_dir_all(&snapshot).unwrap();
    fs::write(snapshot.join(FILE), fixture()).unwrap();

    let (output, events) = setup.run(&server.endpoint());

    assert_success(&output);
    assert_eq!(kinds(&events), ["file", "done"]);
    assert_eq!(events[0]["from"], "cache");
    assert_eq!(fs::read(setup.model()).unwrap(), fixture());
    assert_eq!(server.requests(), []);
}

#[test]
fn a_corrupted_download_is_deleted_and_retried_until_three_bad_checks() {
    let server = Server::start(vec![Reply::Corrupt]);
    let setup = Setup::new();

    let (output, events) = setup.run(&server.endpoint());

    assert!(!output.status.success());
    assert_eq!(
        kinds(&events),
        [
            "file",
            "verifying",
            "retry",
            "verifying",
            "retry",
            "verifying",
            "error"
        ]
    );
    assert!(
        events.last().unwrap()["message"]
            .as_str()
            .unwrap()
            .contains("damaged")
    );
    assert!(!setup.model().exists());
    assert!(!setup.partial().exists());
    // Each retry starts over: the damaged file was deleted, so nothing is resumed.
    assert_eq!(
        server.requests(),
        [
            request(resolve_path(), None),
            request(cdn_path(), None),
            request(resolve_path(), None),
            request(cdn_path(), None),
            request(resolve_path(), None),
            request(cdn_path(), None),
        ]
    );
}

#[test]
fn running_setup_again_with_a_good_file_present_does_nothing() {
    let server = Server::start(vec![Reply::Full]);
    let setup = Setup::new();
    let (output, _) = setup.run(&server.endpoint());
    assert_success(&output);
    let requests = server.requests();

    let (output, events) = setup.run(&server.endpoint());

    assert_success(&output);
    assert_eq!(kinds(&events), ["done"]);
    assert_eq!(server.requests(), requests);
}

fn spawn_events(mut command: Command) -> (Child, impl Iterator<Item = Value>) {
    let mut child = command.stdout(Stdio::piped()).spawn().unwrap();
    let lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let events = lines.map(|line| serde_json::from_str(&line.unwrap()).unwrap());
    (child, events)
}

#[test]
fn a_refused_connection_counts_as_a_failure_and_is_retried() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let setup = Setup::new();

    let (mut child, mut events) = spawn_events(setup.command(&format!("http://{addr}")));
    assert_eq!(events.next().unwrap()["event"], "file");
    assert_eq!(events.next().unwrap()["event"], "retry");

    Server::on(TcpListener::bind(addr).unwrap(), vec![Reply::Full]);
    let events: Vec<Value> = events.collect();

    assert!(child.wait().unwrap().success());
    // Attempts already under way when the server came up may still have been refused.
    let kinds = kinds(&events);
    let retries = kinds.iter().take_while(|kind| **kind == "retry").count();
    assert_eq!(kinds[retries..], ["verifying", "done"]);
    assert_eq!(fs::read(setup.model()).unwrap(), fixture());
}

#[test]
fn with_no_network_setup_waits_for_one() {
    let setup = Setup::new();

    // `.invalid` never resolves (RFC 2606), which is what every lookup looks like without a network.
    let (mut child, events) = spawn_events(setup.command("http://trove-test.invalid"));
    let events: Vec<Value> = events.take(2).collect();

    assert_eq!(kinds(&events), ["file", "waiting"]);
    assert!(child.try_wait().unwrap().is_none(), "setup stopped waiting");
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn invalid_model_config_fails_naming_the_field() {
    let setup = Setup::new();
    let config_path = setup.data_dir.join("config.toml");
    let config = fs::read_to_string(&config_path).unwrap();
    fs::write(&config_path, config.replace(REVISION, "main")).unwrap();

    let (output, _) = setup.run("http://127.0.0.1:9");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("`model.revision`"));
}
