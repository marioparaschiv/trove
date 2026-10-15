mod common;

use std::{
    fs::{self, File},
    path::Path,
    time::{Duration, SystemTime},
};

use common::{Serve, indexed, result_paths, run, vault_copy};
use serde_json::{Value, json};
use tempfile::TempDir;

const EDITED_NOTE: &str = "Travel/Kyoto.md";
const EDITED_SECTION: &str = "Take the train west to the bamboo grove, then cross the Togetsukyo bridge and climb to the \
                              monkey park for the view over the city.";
const BEEKEEPING: &str = "Check each hive every week from April to September. Look for eggs and young larvae to confirm \
                          the queen is laying, add a super when the brood box is full, and treat for varroa mites after \
                          the honey harvest.";
/// Only the edited section is about this.
const BEEKEEPING_QUERY: &str = "how to keep honeybee colonies healthy through winter";

/// Replaces the text of one section of a three-section note.
fn edit_one_section(vault: &Path) {
    let path = vault.join(EDITED_NOTE);
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains(EDITED_SECTION));
    fs::write(&path, text.replace(EDITED_SECTION, BEEKEEPING)).unwrap();
}

fn assert_finds_the_edited_section(response: &Value) {
    assert_eq!(response["results"][0]["path"], EDITED_NOTE, "{response}");
    assert_eq!(
        response["results"][0]["heading_path"],
        json!(["Kyoto", "Arashiyama"]),
        "{response}"
    );
}

fn search(serve: &Serve, query: &str) -> Value {
    let (status, response) = serve.call("POST", "/search", Some(&json!({ "query": query })));
    assert_eq!(status, 200, "{response}");

    response
}

fn status(serve: &Serve) -> Value {
    let (status, body) = serve.call("GET", "/status", None);
    assert_eq!(status, 200, "{body}");

    body
}

/// The notes and chunks in the index, as `trove status` prints them.
fn index_size(data_dir: &Path) -> (u64, u64) {
    let status = run(data_dir, &["status"]);
    let line = status
        .lines()
        .find_map(|line| line.strip_prefix("index: "))
        .unwrap();
    let (files, chunks) = line
        .strip_suffix(" chunks")
        .and_then(|line| line.split_once(" files, "))
        .unwrap();

    (files.parse().unwrap(), chunks.parse().unwrap())
}

/// Starts the service with a short debounce and returns once its startup index run is done, so a later subscription
/// to the events stream sees none of that run's events.
fn serve_indexed(data_dir: TempDir) -> Serve {
    fs::write(
        data_dir.path().join("config.toml"),
        "index_debounce = \"200ms\"\n",
    )
    .unwrap();
    let serve = Serve::start_in(data_dir);
    let mut events = serve.events();

    if status(&serve)["indexing"]["busy"] == true {
        events.until("index_finished");
    }

    serve
}

#[test]
fn an_edited_section_is_embedded_alone_and_found_after_the_debounce() {
    let vault = vault_copy();
    let serve = serve_indexed(indexed(vault.path()));
    let mut events = serve.events();

    edit_one_section(vault.path());

    assert_eq!(
        events.until("index_finished"),
        [
            json!({ "type": "index_started", "mode": "changed" }),
            json!({ "type": "index_progress", "embedded": 1, "total": 1 }),
            json!({
                "type": "index_finished",
                "report": { "files": 1, "removed": 0, "embedded": 1, "problems": [] },
            }),
        ]
    );
    assert_eq!(status(&serve)["indexing"]["embedded"], 1);
    assert_finds_the_edited_section(&search(&serve, BEEKEEPING_QUERY));

    let notes = &status(&serve)["index"]["files"];
    assert_eq!(
        run(serve.data_dir.path(), &["index", "--full"]),
        format!("indexed {notes} notes (0 removed), embedded 0 chunks\n")
    );
    for body in [json!({ "mode": "everything" }), json!({ "full": true })] {
        let (status, error) = serve.call("POST", "/index", Some(&body));
        assert_eq!(status, 400, "{error}");
    }

    serve.stop();
}

#[test]
fn a_renamed_note_is_found_at_its_new_path_without_embedding() {
    let vault = vault_copy();
    let serve = serve_indexed(indexed(vault.path()));
    let mut events = serve.events();

    fs::rename(
        vault.path().join("Music/Guitar practice.md"),
        vault.path().join("Music/Guitar lessons.md"),
    )
    .unwrap();

    assert_eq!(
        events.until("index_finished"),
        [
            json!({ "type": "index_started", "mode": "changed" }),
            json!({
                "type": "index_finished",
                "report": { "files": 1, "removed": 1, "embedded": 0, "problems": [] },
            }),
        ]
    );
    assert_eq!(status(&serve)["indexing"]["embedded"], 0);
    let response = search(
        &serve,
        "fretting six strings simultaneously using one fingertip",
    );
    assert_eq!(result_paths(&response)[0], "Music/Guitar lessons.md");

    serve.stop();
}

#[test]
fn a_note_edited_while_the_service_was_stopped_is_indexed_on_start() {
    let vault = vault_copy();
    let data_dir = indexed(vault.path());

    edit_one_section(vault.path());
    let serve = serve_indexed(data_dir);

    assert_eq!(status(&serve)["indexing"]["embedded"], 1);
    assert_finds_the_edited_section(&search(&serve, BEEKEEPING_QUERY));

    serve.stop();
}

#[test]
fn index_modes_choose_what_is_read_and_embedded() {
    let vault = vault_copy();
    let data_dir = indexed(vault.path());
    let index = |args: &[&str]| run(data_dir.path(), &[&["index"], args].concat());
    let (notes, chunks) = index_size(data_dir.path());

    assert_eq!(
        index(&[]),
        "indexed 0 notes (0 removed), embedded 0 chunks\n"
    );

    // A newer modification time alone makes the note be read, but its unchanged content isn't chunked again.
    File::options()
        .write(true)
        .open(vault.path().join(EDITED_NOTE))
        .unwrap()
        .set_modified(SystemTime::now() + Duration::from_secs(60))
        .unwrap();
    assert_eq!(
        index(&[]),
        "indexed 0 notes (0 removed), embedded 0 chunks\n"
    );

    assert_eq!(
        index(&["--full"]),
        format!("indexed {notes} notes (0 removed), embedded 0 chunks\n")
    );
    edit_one_section(vault.path());
    assert_eq!(
        index(&["--full"]),
        format!("indexed {notes} notes (0 removed), embedded 1 chunks\n")
    );
    assert_eq!(
        index(&["--reembed"]),
        format!("indexed {notes} notes (0 removed), embedded {chunks} chunks\n")
    );

    fs::remove_file(vault.path().join(EDITED_NOTE)).unwrap();
    assert_eq!(
        index(&[]),
        "indexed 0 notes (1 removed), embedded 0 chunks\n"
    );
    assert_eq!(
        common::search(data_dir.path(), BEEKEEPING_QUERY),
        json!({ "results": [] })
    );
}
