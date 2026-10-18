mod common;

use std::{fs, path::Path};

use common::{Serve, data_dir_with_model, indexed, path_str, result_paths, run, trove, vault_copy};
use serde_json::{Value, json};

const QUERIES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/queries.json");

/// Appears once in the vault, inside a long service log that is about something else, so meaning alone misses it.
const IDENTIFIER: &str = "7Q4K-29XP";
const IDENTIFIER_NOTE: &str = "Home/Boiler service.md";

/// Shares no word with its note, so only meaning finds it.
const PARAPHRASE: &str = "how to look after my bread culture";
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

/// Runs `trove eval` and returns the summary it writes with `--out`.
fn eval(data_dir: &Path, queries: &Path, switches: &[&str]) -> Value {
    let out = data_dir.join("summary.json");
    let mut args = vec!["eval", path_str(queries), "--out", path_str(&out)];
    args.extend(switches);
    run(data_dir, &args);

    serde_json::from_slice(&fs::read(out).unwrap()).unwrap()
}

#[test]
fn eval_scores_the_fixture_queries() {
    let vault = vault_copy();
    let data_dir = indexed(vault.path());

    let summary = eval(data_dir.path(), Path::new(QUERIES), &[]);
    assert_eq!(summary["overall"]["queries"], 20, "{summary}");
    assert_eq!(summary["overall"]["mrr"], 1.0, "{summary}");
    assert_eq!(summary["langs"]["en"]["queries"], 20, "{summary}");
    let kinds: Vec<&str> = summary["kinds"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(kinds, ["lookup", "paraphrase", "question", "vague"]);
    assert_eq!(summary["no_answer"], Value::Null);

    // Words alone miss every paraphrase.
    let summary = eval(data_dir.path(), Path::new(QUERIES), &["--no-meaning"]);
    assert_eq!(summary["kinds"]["paraphrase"]["hits_at_10"], 0, "{summary}");
}

#[test]
fn eval_fit_gives_the_same_weights_and_scores_each_run() {
    let vault = vault_copy();
    let data_dir = indexed(vault.path());

    let report = eval(data_dir.path(), Path::new(QUERIES), &["--fit"]);
    assert!(report["examples"].as_u64().unwrap() > 0, "{report}");
    let held_out = report["held_out"]["fitted"]["overall"]["queries"]
        .as_u64()
        .unwrap();
    assert!((1..20).contains(&held_out), "{report}");
    assert_eq!(
        report["all"]["fitted"]["overall"]["queries"], 20,
        "{report}"
    );

    assert_eq!(
        eval(data_dir.path(), Path::new(QUERIES), &["--fit"]),
        report
    );
}

#[test]
fn eval_counts_a_no_answer_query_as_a_hit_when_nothing_is_found() {
    let vault = vault_copy();
    let data_dir = indexed(vault.path());
    let queries = data_dir.path().join("queries.json");
    fs::write(
        &queries,
        json!([
            { "query": "zeppelin airship", "kind": "no_answer", "lang": "en", "note": null },
            { "query": "tomato", "kind": "no_answer", "lang": "en", "note": null },
            { "query": IDENTIFIER, "kind": "lookup", "lang": "en", "note": IDENTIFIER_NOTE },
        ])
        .to_string(),
    )
    .unwrap();

    let summary = eval(data_dir.path(), &queries, &["--no-meaning"]);
    assert_eq!(
        summary["no_answer"],
        json!({ "queries": 2, "empty": 1, "empty_share": 0.5 })
    );
    assert_eq!(summary["overall"]["queries"], 1, "{summary}");
    assert_eq!(summary["queries"][0]["rank"], Value::Null);
    assert_eq!(summary["queries"][0]["results"], 0);
    assert_eq!(summary["queries"][2]["rank"], 1);
}

#[test]
fn eval_fails_on_a_malformed_queries_file() {
    let data_dir = tempfile::tempdir().unwrap();
    let malformed = [
        ("not json", r#"[{"query": "bread""#),
        (
            "not a list",
            r#"{"query": "bread", "kind": "vague", "lang": "en", "note": "a.md"}"#,
        ),
        ("empty list", "[]"),
        (
            "missing kind",
            r#"[{"query": "bread", "lang": "en", "note": "a.md"}]"#,
        ),
        (
            "unknown kind",
            r#"[{"query": "bread", "kind": "fuzzy", "lang": "en", "note": "a.md"}]"#,
        ),
        (
            "missing lang",
            r#"[{"query": "bread", "kind": "vague", "note": "a.md"}]"#,
        ),
        (
            "missing note",
            r#"[{"query": "bread", "kind": "vague", "lang": "en"}]"#,
        ),
        (
            "null note on a query with an answer",
            r#"[{"query": "bread", "kind": "vague", "lang": "en", "note": null}]"#,
        ),
        (
            "note on a no_answer query",
            r#"[{"query": "bread", "kind": "no_answer", "lang": "en", "note": "a.md"}]"#,
        ),
        (
            "unknown field",
            r#"[{"query": "bread", "kind": "vague", "lang": "en", "note": "a.md", "rank": 1}]"#,
        ),
        (
            "empty query",
            r#"[{"query": " ", "kind": "vague", "lang": "en", "note": "a.md"}]"#,
        ),
        (
            "empty note",
            r#"[{"query": "bread", "kind": "vague", "lang": "en", "note": ""}]"#,
        ),
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
