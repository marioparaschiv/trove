mod common;

use std::{fs, path::Path};

use common::{indexed, result_paths, run, search, vault_copy};
use serde_json::{Value, json};
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

#[test]
fn hindi_words_with_vowel_signs_match_a_query_typed_without_their_ending() {
    let vault = vault(&[
        ("Library.md", "# पुस्तकालय\n\nमैंने कल तीन पुस्तकें लौटाईं।\n"),
        (
            "Garden.md",
            "# Garden\n\nTomatoes need sun, water and a stake to climb.\n",
        ),
    ]);
    let data_dir = indexed(vault.path());

    assert_eq!(word_matches(data_dir.path(), "पुस्तक"), ["Library.md"]);
    assert_eq!(word_matches(data_dir.path(), "लौटाईं"), ["Library.md"]);
}

#[test]
fn a_question_finds_the_note_with_its_content_words_without_its_filler_words() {
    let vault = vault_copy();
    let data_dir = indexed(vault.path());

    // The note has "read", "boiler", "pressure" and "gauge", but neither "how" nor "do".
    assert_eq!(
        word_matches(data_dir.path(), "how do I read the boiler pressure gauge")[0],
        "Home/Boiler service.md"
    );
}

#[test]
fn a_lookup_ranks_its_exact_match_above_a_stronger_meaning_match() {
    let vault = vault_copy();
    fs::write(
        vault.path().join("Kitchen scale.md"),
        "# Kitchen scale\n\nBought a digital kitchen scale for weighing flour and water when baking bread. It reads in \
         grams up to five kilos and has a tare button. Order 4815162342, delivered on Tuesday.\n",
    )
    .unwrap();
    let data_dir = indexed(vault.path());

    let meaning: Value = serde_json::from_str(&run(
        data_dir.path(),
        &["search", "--json", "--no-words", "7Q4K-29XP"],
    ))
    .unwrap();
    assert_eq!(result_paths(&meaning), ["Kitchen scale.md"], "{meaning}");

    let response = search(data_dir.path(), "7Q4K-29XP");
    assert_eq!(
        result_paths(&response),
        ["Home/Boiler service.md", "Kitchen scale.md"],
        "{response}"
    );
    assert_eq!(response["results"][1]["why"], json!(["meaning"]));
}
