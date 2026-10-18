mod common;

use std::fs;

use common::{Serve, read_runtime, runtime_path, stdout, trove};
use serde_json::Value;

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

    // The startup index run may come before it.
    serve.events().until("stopping");
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
    assert_eq!(
        body["model"],
        serde_json::json!({ "loaded": false, "device": null })
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

    let output = trove(serve.data_dir.path()).arg("serve").output().unwrap();

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("already running"));
    assert_eq!(read_runtime(&serve.data_dir), runtime);
    serve.stop();
}

#[test]
fn status_command_uses_the_running_service() {
    let serve = Serve::start("1m");

    let output = trove(serve.data_dir.path()).arg("status").output().unwrap();

    let expected = format!(
        "service: running (pid {}, port {})\nindex: 0 files, 0 chunks\nmodel: not loaded\n",
        serve.child.id(),
        serve.port
    );
    assert_eq!(stdout(&output), expected);
    serve.stop();
}

#[test]
fn status_command_reads_the_index_without_a_service() {
    let data_dir = tempfile::tempdir().unwrap();

    let output = trove(data_dir.path()).arg("status").output().unwrap();

    assert_eq!(
        stdout(&output),
        "service: not running\nindex: 0 files, 0 chunks\nmodel: not loaded\n"
    );
}

#[test]
fn status_command_ignores_a_runtime_file_left_by_a_crash() {
    let serve = Serve::start("1m");
    let runtime = read_runtime(&serve.data_dir).unwrap();
    let data_dir = tempfile::tempdir().unwrap();
    serve.stop();
    fs::write(runtime_path(&data_dir), runtime.to_string()).unwrap();

    let output = trove(data_dir.path()).arg("status").output().unwrap();

    assert_eq!(
        stdout(&output),
        "service: not running\nindex: 0 files, 0 chunks\nmodel: not loaded\n"
    );
}
