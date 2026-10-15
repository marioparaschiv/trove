mod common;

use std::{fs, path::Path};

use common::{Serve, data_dir_with_model, indexed, path_str, result_paths, run, trove, vault_copy};
use serde_json::{Value, json};

const QUERIES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/queries.json");

/// Appears once in the vault, inside a long service log that is about something else, so meaning alone misses it.
const IDENTIFIER: &str = "7Q4K-29XP";
const IDENTIFIER_NOTE: &str = "Home/Boiler service.md";

/// Shares no word with its note, so only meaning finds it.
const PARAPHRASE: &str = "how to keep my bread culture alive";
const PARAPHRASE_NOTE: &str = "Cooking/Sourdough starter.md";

fn search(data_dir: &Path, query: &str, switches: &[&str]) -> Value {
    let mut args = vec!["search", "--json", query];
    args.extend(switches);

    serde_json::from_str(&run(data_dir, &args)).unwrap()
}

#[test]
fn words_find_an_exact_identifier_that_meaning_misses() {
    let vault = vault_copy();
    let data_dir = indexed(vault.path());

    let response = search(data_dir.path(), IDENTIFIER, &[]);
    assert_eq!(result_paths(&response), [IDENTIFIER_NOTE], "{response}");
    assert_eq!(response["results"][0]["why"], json!(["words"]));

    assert_eq!(
        search(data_dir.path(), IDENTIFIER, &["--no-words"]),
        json!({ "results": [] })
    );
}

#[test]
fn meaning_finds_a_paraphrase_that_words_miss() {
    let vault = vault_copy();
    let data_dir = indexed(vault.path());

    let response = search(data_dir.path(), PARAPHRASE, &[]);
    assert_eq!(result_paths(&response)[0], PARAPHRASE_NOTE, "{response}");
    assert_eq!(response["results"][0]["why"], json!(["meaning"]));

    assert_eq!(
        search(data_dir.path(), PARAPHRASE, &["--no-meaning"]),
        json!({ "results": [] })
    );
    assert_eq!(
        run(
            data_dir.path(),
            &["search", PARAPHRASE, "--no-meaning", "--no-words"]
        ),
        "no matches\n"
    );
}

#[test]
fn each_result_lists_the_stages_that_found_it() {
    let vault = vault_copy();
    let data_dir = indexed(vault.path());

    let response = search(data_dir.path(), "boiler", &[]);
    assert_eq!(result_paths(&response), [IDENTIFIER_NOTE], "{response}");
    assert_eq!(response["results"][0]["why"], json!(["meaning", "words"]));

    let plain = run(data_dir.path(), &["search", "boiler"]);
    assert!(
        plain.ends_with("Home/Boiler service.md > Boiler service  (meaning, words)\n"),
        "{plain}"
    );
}

#[test]
fn stage_switches_and_why_work_over_http() {
    let vault = vault_copy();
    let data_dir = data_dir_with_model();
    run(data_dir.path(), &["sources", "add", path_str(vault.path())]);
    let serve = Serve::start_in(data_dir);
    run(serve.data_dir.path(), &["index"]);

    let (status, response) = serve.call("POST", "/search", Some(&json!({ "query": IDENTIFIER })));
    assert_eq!(status, 200, "{response}");
    assert_eq!(result_paths(&response), [IDENTIFIER_NOTE]);
    assert_eq!(response["results"][0]["why"], json!(["words"]));

    let (status, response) = serve.call(
        "POST",
        "/search",
        Some(&json!({ "query": IDENTIFIER, "skip": ["words"] })),
    );
    assert_eq!((status, response), (200, json!({ "results": [] })));

    // The CLI's switches go through the running service.
    assert_eq!(
        search(serve.data_dir.path(), PARAPHRASE, &["--no-meaning"]),
        json!({ "results": [] })
    );

    let (status, response) = serve.call(
        "POST",
        "/search",
        Some(&json!({ "query": IDENTIFIER, "skip": ["colour"] })),
    );
    assert_eq!(status, 400, "{response}");

    serve.stop();
}

/// The value printed on the line starting with `name`.
fn metric(output: &str, name: &str) -> f64 {
    output
        .lines()
        .find_map(|line| line.strip_prefix(name))
        .unwrap_or_else(|| panic!("no {name} in {output}"))
        .trim()
        .parse()
        .unwrap()
}

#[test]
fn eval_scores_the_fixture_queries() {
    let vault = vault_copy();
    let data_dir = indexed(vault.path());

    let output = run(data_dir.path(), &["eval", QUERIES]);
    for name in ["recall@5", "recall@10", "MRR"] {
        assert_eq!(metric(&output, name), 1.0, "{output}");
    }

    // Words alone miss every paraphrase.
    let output = run(data_dir.path(), &["eval", QUERIES, "--no-meaning"]);
    assert!(metric(&output, "recall@10") < 0.5, "{output}");
}

#[test]
fn eval_fails_on_a_malformed_queries_file() {
    let data_dir = tempfile::tempdir().unwrap();
    let malformed = [
        ("not json", "[{\"query\": \"bread\""),
        ("not a list", "{\"query\": \"bread\", \"note\": \"a.md\"}"),
        ("empty list", "[]"),
        ("missing note", "[{\"query\": \"bread\"}]"),
        (
            "unknown field",
            "[{\"query\": \"bread\", \"note\": \"a.md\", \"rank\": 1}]",
        ),
        ("empty query", "[{\"query\": \" \", \"note\": \"a.md\"}]"),
        ("empty note", "[{\"query\": \"bread\", \"note\": \"\"}]"),
    ];

    for (case, text) in malformed {
        let file = data_dir.path().join("queries.json");
        fs::write(&file, text).unwrap();
        let output = trove(data_dir.path())
            .args(["eval", file.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{case}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("failed to read queries from"),
            "{case}: {stderr}"
        );
    }

    let missing = data_dir.path().join("missing.json");
    let output = trove(data_dir.path())
        .args(["eval", missing.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!output.status.success());
}
