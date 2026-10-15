mod common;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use common::{
    Serve, data_dir_with_model, indexed, path_str, result_paths, run, search, vault_copy,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const STORE_FOLDER: &str = ".trove";
const PARAPHRASE: &str = "how to keep my bread culture alive";
const PARAPHRASE_NOTE: &str = "Cooking/Sourdough starter.md";
const HIDDEN_TOPIC: &str = "how to keep honeybee colonies healthy through winter";
/// Where a vector file holds its `vector_version`: after the 8-byte magic and the 4-byte format version.
const VECTOR_VERSION_BYTES: std::ops::Range<usize> = 12..76;

/// Every file in the store folder by its path inside it.
fn store_files(store: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    collect_files(store, store, &mut files);

    files
}

fn collect_files(root: &Path, folder: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
    for entry in fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_files(root, &path, files);
        } else {
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            files.insert(relative, fs::read(&path).unwrap());
        }
    }
}

/// The vector file of the note at `relative`, as the spec names it.
fn vector_file(vault: &Path, relative: &str) -> PathBuf {
    let name = hex::encode(Sha256::digest(relative));
    vault
        .join(STORE_FOLDER)
        .join("vectors")
        .join(&name[..2])
        .join(format!("{name}.vec"))
}

/// The notes and chunk texts `trove index` reports.
fn index_counts(output: &str) -> (u64, u64) {
    let (notes, rest) = output
        .strip_prefix("indexed ")
        .and_then(|output| output.split_once(" notes ("))
        .unwrap_or_else(|| panic!("unexpected output {output:?}"));
    let embedded = rest
        .split_once("embedded ")
        .and_then(|(_, rest)| rest.strip_suffix(" chunks\n"))
        .unwrap_or_else(|| panic!("unexpected output {output:?}"));

    (notes.parse().unwrap(), embedded.parse().unwrap())
}

fn status(serve: &Serve) -> Value {
    let (status, body) = serve.call("GET", "/status", None);
    assert_eq!(status, 200, "{body}");

    body
}

#[test]
fn indexing_the_same_vault_twice_writes_identical_vector_files() {
    let vault = vault_copy();
    let data_dir = indexed(vault.path());
    let store = vault.path().join(STORE_FOLDER);
    let first = store_files(&store);

    let (notes, embedded) = index_counts(&run(data_dir.path(), &["index", "--reembed"]));
    assert!(embedded > 0);
    assert_eq!(first.len() as u64, notes, "one file per note: {first:?}");
    assert_eq!(store_files(&store), first);
    assert!(vector_file(vault.path(), PARAPHRASE_NOTE).is_file());
}

#[test]
fn a_rebuilt_index_makes_no_model_calls() {
    let vault = vault_copy();
    let data_dir = indexed(vault.path());
    let before = run(data_dir.path(), &["status"]);
    let files = store_files(&vault.path().join(STORE_FOLDER));

    for name in ["index.sqlite", "index.sqlite-wal", "index.sqlite-shm"] {
        let path = data_dir.path().join(name);
        if path.exists() {
            fs::remove_file(path).unwrap();
        }
    }
    run(data_dir.path(), &["sources", "add", path_str(vault.path())]);
    let serve = Serve::start_in(data_dir);
    let mut events = serve.events();
    if status(&serve)["indexing"]["busy"] == true {
        events.until("index_finished");
    }

    let status = status(&serve);
    assert_eq!(status["indexing"]["embedded"], 0, "{status}");
    assert_eq!(status["model"]["loaded"], false, "{status}");
    assert_eq!(
        run(serve.data_dir.path(), &["status"]).lines().nth(1),
        before.lines().nth(1),
        "same index size"
    );
    assert_eq!(store_files(&vault.path().join(STORE_FOLDER)), files);

    let (_, response) = serve.call("POST", "/search", Some(&json!({ "query": PARAPHRASE })));
    assert_eq!(result_paths(&response)[0], PARAPHRASE_NOTE, "{response}");
    assert_eq!(response["results"][0]["why"], json!(["meaning"]));

    serve.stop();
}

#[test]
fn a_second_device_imports_the_vectors_without_embedding() {
    let vault = vault_copy();
    let first = indexed(vault.path());
    let (notes, _) = index_counts(&run(first.path(), &["index", "--full"]));

    let second = data_dir_with_model();
    run(second.path(), &["sources", "add", path_str(vault.path())]);
    assert_eq!(
        run(second.path(), &["index"]),
        format!("indexed {notes} notes (0 removed), embedded 0 chunks\n")
    );
    assert_eq!(
        result_paths(&search(second.path(), PARAPHRASE))[0],
        PARAPHRASE_NOTE
    );
}

#[test]
fn a_vector_file_from_another_vector_version_is_ignored_and_replaced() {
    let vault = vault_copy();
    let first = indexed(vault.path());
    let (_, all_chunks) = index_counts(&run(first.path(), &["index", "--reembed"]));
    let path = vector_file(vault.path(), PARAPHRASE_NOTE);
    let original = fs::read(&path).unwrap();
    let mut other_version = original.clone();
    other_version[VECTOR_VERSION_BYTES].fill(b'0');
    fs::write(&path, &other_version).unwrap();

    let second = data_dir_with_model();
    run(second.path(), &["sources", "add", path_str(vault.path())]);
    let (_, embedded) = index_counts(&run(second.path(), &["index"]));

    assert!(
        embedded > 0 && embedded < all_chunks,
        "only that note is embedded: {embedded} of {all_chunks}"
    );
    assert_eq!(
        fs::read(&path).unwrap()[VECTOR_VERSION_BYTES],
        original[VECTOR_VERSION_BYTES]
    );
    let third = data_dir_with_model();
    run(third.path(), &["sources", "add", path_str(vault.path())]);
    assert_eq!(index_counts(&run(third.path(), &["index"])).1, 0);
}

#[test]
fn deleted_notes_and_conflict_copies_leave_the_store() {
    let vault = vault_copy();
    let data_dir = indexed(vault.path());
    let kept = vector_file(vault.path(), PARAPHRASE_NOTE);
    let stem = kept.file_stem().unwrap().to_str().unwrap();
    let conflicts = [
        kept.with_file_name(format!("{stem}.sync-conflict-20261015-101500-ABCDEF7.vec")),
        kept.with_file_name(format!("{stem} (Laptop's conflicted copy 2026-10-15).vec")),
    ];
    for conflict in &conflicts {
        fs::write(conflict, b"conflict").unwrap();
    }
    let deleted = "Travel/Kyoto.md";
    assert!(vector_file(vault.path(), deleted).is_file());
    fs::remove_file(vault.path().join(deleted)).unwrap();

    run(data_dir.path(), &["index"]);

    for conflict in &conflicts {
        assert!(!conflict.exists(), "{}", conflict.display());
    }
    assert!(!vector_file(vault.path(), deleted).exists());
    assert!(kept.is_file());
}

#[test]
fn the_store_folder_name_is_configurable_and_never_indexed() {
    let vault = vault_copy();
    let store = vault.path().join("trove-store");
    fs::create_dir(&store).unwrap();
    fs::copy(
        vault.path().join(".trash/Beekeeping.md"),
        store.join("Beekeeping.md"),
    )
    .unwrap();
    let data_dir = data_dir_with_model();
    fs::write(
        data_dir.path().join("config.toml"),
        "store_folder = \"trove-store\"\n",
    )
    .unwrap();
    run(data_dir.path(), &["sources", "add", path_str(vault.path())]);

    let (notes, _) = index_counts(&run(data_dir.path(), &["index"]));

    assert_eq!(
        search(data_dir.path(), HIDDEN_TOPIC),
        json!({ "results": [] })
    );
    assert!(!vault.path().join(STORE_FOLDER).exists());
    let vector_files = store_files(&store.join("vectors"));
    assert_eq!(vector_files.len() as u64, notes, "{vector_files:?}");
}
