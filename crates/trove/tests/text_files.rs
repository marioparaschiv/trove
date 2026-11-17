mod common;

use std::{fs, path::Path};

use common::{
    Serve, data_dir_with_model, path_str, result_paths, run, search_with_switches, stdout, trove,
};
use serde_json::{Value, json};
use tempfile::TempDir;

/// A made-up word, so a keyword search finds exactly the files that hold it.
const WORD: &str = "quillwort";

/// A folder holding `files`, created with their parent folders.
fn folder(files: &[(&str, &[u8])]) -> TempDir {
    let folder = tempfile::tempdir().unwrap();
    for (path, bytes) in files {
        let path = folder.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    folder
}

/// A data folder with the model and `config`, `source` registered and indexed. Returns the index run's stderr too.
fn indexed_with(source: &Path, config: &str) -> (TempDir, String) {
    let data_dir = data_dir_with_model();
    fs::write(data_dir.path().join("config.toml"), config).unwrap();
    run(data_dir.path(), &["sources", "add", path_str(source)]);
    let output = trove(data_dir.path()).arg("index").output().unwrap();
    stdout(&output);

    (data_dir, String::from_utf8(output.stderr).unwrap())
}

/// What keyword search alone finds for `query`, without notes the graph adds.
fn word_search(data_dir: &Path, query: &str) -> Value {
    search_with_switches(data_dir, query, &["--no-meaning", "--no-graph"])
}

#[test]
fn a_folder_indexes_its_text_files_and_skips_ignored_hidden_binary_media_and_large_files() {
    let paragraph = format!("A {WORD} grows under water at the edge of the lake.\n");
    let large = paragraph.repeat(100);
    let source = folder(&[
        (
            "Field notes/lake.txt",
            format!(
                "---\ntags: [plants]\n---\n\n# Not a heading\n\n{paragraph}\nSee [[note]] and #botany.\n"
            )
            .as_bytes(),
        ),
        ("note.md", format!("# Note\n\n{paragraph}").as_bytes()),
        (".gitignore", b"build/\n*.tmp\n"),
        ("build/output.txt", paragraph.as_bytes()),
        ("scratch.tmp", paragraph.as_bytes()),
        (".hidden.txt", paragraph.as_bytes()),
        (".obsidian/plugin.txt", paragraph.as_bytes()),
        ("photo.png", paragraph.as_bytes()),
        ("recording.mp3", paragraph.as_bytes()),
        ("data.bin", format!("\0{paragraph}").as_bytes()),
        ("latin1.txt", b"A quillwort \xe9t\xe9 grows here.\n"),
        ("large.log", large.as_bytes()),
    ]);

    let (data_dir, problems) = indexed_with(source.path(), "max_file_size = \"1KB\"\n");

    let large = source.path().join("large.log");
    assert_eq!(
        problems,
        format!(
            "warning: {}: the file is larger than max_file_size ({} > 1000 bytes)\n",
            large.display(),
            fs::metadata(&large).unwrap().len()
        )
    );
    let response = word_search(data_dir.path(), WORD);
    let mut found = result_paths(&response);
    found.sort_unstable();
    assert_eq!(found, ["Field notes/lake.txt", "note.md"], "{response}");

    let text = response["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|result| result["path"] == "Field notes/lake.txt")
        .unwrap();
    assert_eq!(text["title"], "lake.txt");
    assert_eq!(text["heading_path"], json!([]));
    // Frontmatter and tags are plain text in a text file, so their words are found like any other.
    for query in ["plants", "botany"] {
        assert_eq!(
            result_paths(&word_search(data_dir.path(), query)),
            ["Field notes/lake.txt"]
        );
    }
}

#[test]
fn a_text_file_turned_binary_is_dropped_from_the_index() {
    let source = folder(&[("log.txt", format!("The {WORD} log.\n").as_bytes())]);
    let (data_dir, _) = indexed_with(source.path(), "");
    assert_eq!(
        result_paths(&word_search(data_dir.path(), WORD)),
        ["log.txt"]
    );

    fs::write(
        source.path().join("log.txt"),
        format!("\0The {WORD} log.\n"),
    )
    .unwrap();

    assert_eq!(
        run(data_dir.path(), &["index"]),
        "indexed 0 notes (1 removed), embedded 0 chunks\n"
    );
    assert_eq!(result_paths(&word_search(data_dir.path(), WORD)), [""; 0]);
}

#[test]
fn a_single_file_is_a_source_of_its_own() {
    let source = folder(&[
        (
            "chosen.txt",
            format!("The {WORD} in the pond.\n").as_bytes(),
        ),
        (
            "sibling.txt",
            format!("Another {WORD} nearby.\n").as_bytes(),
        ),
    ]);
    let file = source.path().join("chosen.txt");
    let file_path = path_str(&file);

    let (data_dir, problems) = indexed_with(&file, "");

    assert_eq!(problems, "");
    assert_eq!(
        run(data_dir.path(), &["sources", "list"]),
        format!("{file_path}\n")
    );
    let response = word_search(data_dir.path(), WORD);
    assert_eq!(result_paths(&response), ["chosen.txt"], "{response}");
    assert_eq!(response["results"][0]["source"], file_path);
    // The file's vectors sync with the folder holding it.
    assert!(source.path().join(".trove/vectors").is_dir());

    run(data_dir.path(), &["sources", "remove", file_path]);
    assert_eq!(run(data_dir.path(), &["sources", "list"]), "");
    assert_eq!(result_paths(&word_search(data_dir.path(), WORD)), [""; 0]);
}

#[test]
fn the_service_reindexes_a_file_source_when_the_file_changes() {
    let source = folder(&[("chosen.txt", b"Nothing to see yet.\n")]);
    let file = source.path().join("chosen.txt");
    let data_dir = data_dir_with_model();
    fs::write(
        data_dir.path().join("config.toml"),
        "index_debounce = \"200ms\"\n",
    )
    .unwrap();
    let serve = Serve::start_indexed(data_dir);
    let (status, added) = serve.call(
        "POST",
        "/sources",
        Some(&json!({ "path": path_str(&file) })),
    );
    assert_eq!(status, 201, "{added}");
    let mut events = serve.events();

    fs::write(&file, format!("A {WORD} appeared.\n")).unwrap();

    let finished = events.until("index_finished");
    assert_eq!(
        finished.last().unwrap()["report"]["files"],
        1,
        "{finished:?}"
    );
    assert_eq!(result_paths(&serve.search(WORD)), ["chosen.txt"]);

    serve.stop();
}
