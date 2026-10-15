use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::PathBuf,
    process::{Child, Command, Output, Stdio},
};

use serde_json::Value;
use tempfile::TempDir;

struct Serve {
    child: Child,
    data_dir: TempDir,
    port: u16,
    token: String,
}

impl Serve {
    /// Starts `trove serve` and returns once it reports that it listens.
    fn start(idle_timeout: &str) -> Self {
        let data_dir = tempfile::tempdir().unwrap();
        fs::write(
            data_dir.path().join("config.toml"),
            format!("service_idle_timeout = \"{idle_timeout}\"\n"),
        )
        .unwrap();

        let mut child = trove(&data_dir)
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

    fn local_host(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    fn get(&self, path: &str, host: &str, token: Option<&str>) -> (u16, String) {
        let mut stream = self.send(path, host, token);
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        let status = response[9..12].parse().unwrap();
        let (_, body) = response.split_once("\r\n\r\n").unwrap();

        (status, body.to_owned())
    }

    fn send(&self, path: &str, host: &str, token: Option<&str>) -> TcpStream {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        let auth = token
            .map(|token| format!("Authorization: Bearer {token}\r\n"))
            .unwrap_or_default();
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: {host}\r\n{auth}Connection: close\r\n\r\n"
        )
        .unwrap();

        stream
    }

    fn stop(mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }
}

fn trove(data_dir: &TempDir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_trove"));
    command.env("TROVE_DATA_DIR", data_dir.path());

    command
}

fn runtime_path(data_dir: &TempDir) -> PathBuf {
    data_dir.path().join("runtime.json")
}

fn read_runtime(data_dir: &TempDir) -> Option<Value> {
    let text = fs::read_to_string(runtime_path(data_dir)).ok()?;

    Some(serde_json::from_str(&text).unwrap())
}

fn stdout(output: &Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout.clone()).unwrap()
}

#[test]
fn exits_after_the_idle_timeout_and_removes_the_runtime_file() {
    let mut serve = Serve::start("500ms");

    let exit = serve.child.wait().unwrap();

    assert!(exit.success());
    assert!(!runtime_path(&serve.data_dir).exists());
}

#[test]
fn event_stream_ends_with_stopping_when_the_service_goes_idle() {
    let mut serve = Serve::start("2s");
    let (status, body) = serve.get("/status", &serve.local_host(), Some(&serve.token));
    assert_eq!(status, 200, "{body}");

    let events = serve.send("/events", &serve.local_host(), Some(&serve.token));
    let lines: Vec<String> = BufReader::new(events).lines().map(Result::unwrap).collect();

    let data: Vec<Value> = lines
        .iter()
        .filter_map(|line| line.strip_prefix("data: "))
        .map(|data| serde_json::from_str(data).unwrap())
        .collect();
    assert_eq!(data, [serde_json::json!({ "type": "stopping" })]);
    assert!(serve.child.wait().unwrap().success());
    assert!(!runtime_path(&serve.data_dir).exists());
}

#[test]
fn status_needs_the_token_and_the_local_host() {
    let serve = Serve::start("1m");
    let host = serve.local_host();

    let (status, body) = serve.get("/status", &host, Some(&serve.token));
    assert_eq!(status, 200, "{body}");
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["pid"], serve.child.id());
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(
        body["index"],
        serde_json::json!({ "files": 0, "chunks": 0 })
    );

    let localhost = format!("localhost:{}", serve.port);
    assert_eq!(serve.get("/status", &localhost, Some(&serve.token)).0, 200);
    assert_eq!(serve.get("/events", &host, None).0, 401);
    assert_eq!(serve.get("/status", &host, None).0, 401);
    assert_eq!(serve.get("/status", &host, Some("wrong")).0, 401);
    assert_eq!(
        serve.get("/status", "evil.example", Some(&serve.token)).0,
        403
    );
    assert_eq!(
        serve
            .get(
                "/status",
                &format!("evil.example:{}", serve.port),
                Some(&serve.token)
            )
            .0,
        403
    );

    serve.stop();
}

#[test]
fn a_second_serve_leaves_the_running_one_alone() {
    let serve = Serve::start("1m");
    let runtime = read_runtime(&serve.data_dir);

    let output = trove(&serve.data_dir).arg("serve").output().unwrap();

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("already running"));
    assert_eq!(read_runtime(&serve.data_dir), runtime);
    serve.stop();
}

#[test]
fn status_command_uses_the_running_service() {
    let serve = Serve::start("1m");

    let output = trove(&serve.data_dir).arg("status").output().unwrap();

    let expected = format!(
        "service: running (pid {}, port {})\nindex: 0 files, 0 chunks\n",
        serve.child.id(),
        serve.port
    );
    assert_eq!(stdout(&output), expected);
    serve.stop();
}

#[test]
fn status_command_reads_the_index_without_a_service() {
    let data_dir = tempfile::tempdir().unwrap();

    let output = trove(&data_dir).arg("status").output().unwrap();

    assert_eq!(
        stdout(&output),
        "service: not running\nindex: 0 files, 0 chunks\n"
    );
}

#[test]
fn status_command_ignores_a_runtime_file_left_by_a_crash() {
    let serve = Serve::start("1m");
    let runtime = read_runtime(&serve.data_dir).unwrap();
    let data_dir = tempfile::tempdir().unwrap();
    serve.stop();
    fs::write(runtime_path(&data_dir), runtime.to_string()).unwrap();

    let output = trove(&data_dir).arg("status").output().unwrap();

    assert_eq!(
        stdout(&output),
        "service: not running\nindex: 0 files, 0 chunks\n"
    );
}
