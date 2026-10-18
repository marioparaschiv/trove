mod common;

use std::{fs, path::Path};

use common::{indexed, result_paths, run};
use serde_json::Value;
use tempfile::TempDir;

/// A vault of a few short notes, each on its own topic.
fn vault(notes: &[(&str, &str)]) -> TempDir {
    let vault = tempfile::tempdir().unwrap();
    for (path, text) in notes {
        fs::write(vault.path().join(path), text).unwrap();
    }

    vault
}

/// The notes keyword search alone finds for `query`, best first.
fn word_matches(data_dir: &Path, query: &str) -> Vec<String> {
    let response: Value =
        serde_json::from_str(&run(data_dir, &["search", "--json", "--no-meaning", query])).unwrap();

    result_paths(&response)
        .into_iter()
        .map(str::to_owned)
        .collect()
}

#[test]
fn arabic_with_harakat_and_alef_variants_matches_a_query_typed_without_them() {
    let vault = vault(&[
        (
            "School.md",
            "# المدرسة\n\nذَهَبَ أَحْمَدُ إِلَى المَدْرَسَةِ صَبَاحًا مَعَ أُخْتِهِ.\n",
        ),
        (
            "Garden.md",
            "# Garden\n\nTomatoes need sun, water and a stake to climb.\n",
        ),
    ]);
    let data_dir = indexed(vault.path());

    assert_eq!(word_matches(data_dir.path(), "احمد"), ["School.md"]);
    assert_eq!(word_matches(data_dir.path(), "اخته صباحا"), ["School.md"]);
}
