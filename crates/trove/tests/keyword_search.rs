mod common;

use std::{fs, path::Path};

use common::{data_dir_with_model, indexed, path_str, result_paths, run, search, vault_copy};
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
fn a_word_with_one_letter_wrong_finds_the_note_with_the_word() {
    let vault = vault(&[
        (
            "Cluster.md",
            "# Cluster\n\nUpgraded the kubernetes cluster to the new release on Friday.\n",
        ),
        (
            "Garden.md",
            "# Garden\n\nTomatoes need sun, water and a stake to climb.\n",
        ),
    ]);
    let data_dir = indexed(vault.path());

    assert_eq!(word_matches(data_dir.path(), "kubernetis"), ["Cluster.md"]);
    assert_eq!(word_matches(data_dir.path(), "tomatoas"), ["Garden.md"]);
    assert_eq!(
        word_matches(data_dir.path(), "kubernetis upgrade"),
        ["Cluster.md"]
    );
}

#[test]
fn a_word_typed_as_in_a_note_matches_only_that_word_not_words_like_it() {
    let vault = vault(&[
        ("Cat.md", "# Cat\n\nThe cat sleeps on the windowsill.\n"),
        (
            "Cart.md",
            "# Cart\n\nThe shopping cart has a squeaky wheel.\n",
        ),
    ]);
    let data_dir = indexed(vault.path());

    assert_eq!(word_matches(data_dir.path(), "cat"), ["Cat.md"]);
}

#[test]
fn a_chinese_or_japanese_word_inside_a_longer_run_of_text_finds_its_note() {
    let vault = vault(&[
        ("Shopping.md", "# 购物\n\n我昨天在超市买了咖啡豆和牛奶。\n"),
        (
            "Trip.md",
            "# 旅行\n\n東京タワーに行きました。夜景がきれいでした。\n",
        ),
        (
            "Garden.md",
            "# Garden\n\nTomatoes need sun, water and a stake to climb.\n",
        ),
    ]);
    let data_dir = indexed(vault.path());

    assert_eq!(word_matches(data_dir.path(), "咖啡豆"), ["Shopping.md"]);
    assert_eq!(word_matches(data_dir.path(), "牛奶"), ["Shopping.md"]);
    assert_eq!(word_matches(data_dir.path(), "タワー"), ["Trip.md"]);
    assert_eq!(word_matches(data_dir.path(), "夜景"), ["Trip.md"]);
}

#[test]
fn a_chunk_of_digits_gets_no_vector_and_is_found_by_its_words() {
    let vault = vault(&[(
        "Meter readings.md",
        "# Meter readings\n\nGas meter readings, taken on the first of each month.\n\n## 2024\n\n\
         | 04815 | 04902 | 05011 |\n|-------|-------|-------|\n| 05187 | 05342 | 05420 |\n",
    )]);
    let data_dir = data_dir_with_model();
    run(data_dir.path(), &["sources", "add", path_str(vault.path())]);

    let report = run(data_dir.path(), &["index"]);
    assert!(report.contains("embedded 1 chunks"), "{report}");

    let response = search(data_dir.path(), "05342");
    assert_eq!(result_paths(&response), ["Meter readings.md"], "{response}");
    assert_eq!(
        response["results"][0]["heading_path"],
        json!(["Meter readings", "2024"])
    );
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

#[test]
fn a_note_with_a_rare_query_word_ranks_above_one_with_more_common_ones_and_no_note_is_added_or_lost()
 {
    let vault = vault(&[
        (
            "Sumac.md",
            "# Sumac\n\nA recipe with sumac, lemon and onion.\n",
        ),
        (
            "Weeknight.md",
            "# Weeknight\n\nA quick dinner recipe with rice.\n",
        ),
        ("Pasta.md", "# Pasta\n\nA quick dinner recipe with pasta.\n"),
        ("Soup.md", "# Soup\n\nA quick dinner of soup.\n"),
        ("Bike.md", "# Bike\n\nA quick ride along the river.\n"),
        (
            "Garden.md",
            "# Garden\n\nTomatoes need sun, water and a stake to climb.\n",
        ),
    ]);
    let data_dir = indexed(vault.path());

    // Sumac.md has two of the four words, Weeknight.md and Pasta.md three, but "sumac" is in no other note.
    let response: Value = serde_json::from_str(&run(
        data_dir.path(),
        &[
            "search",
            "--json",
            "--no-meaning",
            "--no-graph",
            "quick dinner recipe with sumac",
        ],
    ))
    .unwrap();
    let matches = result_paths(&response);
    assert_eq!(matches[0], "Sumac.md");
    // The notes with at least half the words, rare or not.
    let mut found = matches.clone();
    found.sort();
    assert_eq!(found, ["Pasta.md", "Soup.md", "Sumac.md", "Weeknight.md"]);
}
