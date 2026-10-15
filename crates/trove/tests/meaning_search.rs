mod common;

use std::{collections::HashSet, fs, path::Path};

use common::{
    FIXTURE_VAULT, Serve, data_dir_with_model, indexed, path_str, percent_encode, result_paths,
    run, search, trove, vault_copy,
};
use serde_json::{Value, json};

struct Paraphrase {
    query: &'static str,
    note: &'static str,
    heading_path: &'static [&'static str],
}

const PARAPHRASES: [Paraphrase; 4] = [
    Paraphrase {
        query: "how often should I nourish my bread culture",
        note: "Cooking/Sourdough starter.md",
        heading_path: &["Sourdough starter", "Feeding schedule"],
    },
    Paraphrase {
        query: "Japanese shrine hike with orange torii",
        note: "Travel/Kyoto.md",
        heading_path: &["Kyoto", "Fushimi Inari"],
    },
    Paraphrase {
        query: "fretting six strings simultaneously using one fingertip",
        note: "Music/Guitar practice.md",
        heading_path: &["Guitar practice", "Barre chords"],
    },
    // The answer is the end of one section longer than the model's context, so only splitting the section finds it.
    Paraphrase {
        query: "repairing air mattress leaks while camping",
        note: "Nature/Birding journal.md",
        heading_path: &["Birding journal"],
    },
];

const MATCHES_NOTHING: &str = "differential equations for orbital mechanics";

/// Only notes in `.obsidian` and `.trash` are about this.
const HIDDEN_TOPIC: &str = "how to keep honeybee colonies healthy through winter";

fn words(text: &str) -> HashSet<String> {
    text.split(|char: char| !char.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn assert_shares_no_words(paraphrase: &Paraphrase) {
    let note = fs::read_to_string(Path::new(FIXTURE_VAULT).join(paraphrase.note)).unwrap();
    let shared: Vec<String> = words(paraphrase.query)
        .intersection(&words(&note))
        .cloned()
        .collect();
    assert!(
        shared.is_empty(),
        "{:?} shares {shared:?} with {}",
        paraphrase.query,
        paraphrase.note
    );
}

fn assert_found_first(response: &Value, paraphrase: &Paraphrase) {
    let best = &response["results"][0];
    assert_eq!(
        best["path"], paraphrase.note,
        "{}: {response}",
        paraphrase.query
    );
    assert_eq!(
        best["heading_path"],
        json!(paraphrase.heading_path),
        "{}: {response}",
        paraphrase.query
    );
}

#[test]
fn a_paraphrase_sharing_no_words_finds_its_note_first_at_the_heading() {
    let vault = vault_copy();
    let data_dir = indexed(vault.path());

    for paraphrase in &PARAPHRASES {
        assert_shares_no_words(paraphrase);
        assert_found_first(&search(data_dir.path(), paraphrase.query), paraphrase);
    }
}

#[test]
fn a_query_matching_nothing_finds_nothing() {
    let vault = vault_copy();
    let data_dir = indexed(vault.path());

    assert_eq!(
        search(data_dir.path(), MATCHES_NOTHING),
        json!({ "results": [] })
    );
    assert_eq!(
        run(data_dir.path(), &["search", MATCHES_NOTHING]),
        "no matches\n"
    );
}

#[test]
fn notes_in_obsidian_and_trash_folders_are_never_indexed() {
    let vault = vault_copy();
    let data_dir = indexed(vault.path());

    assert_eq!(
        search(data_dir.path(), HIDDEN_TOPIC),
        json!({ "results": [] })
    );

    // The same note outside those folders is found.
    fs::copy(
        vault.path().join(".trash/Beekeeping.md"),
        vault.path().join("Beekeeping.md"),
    )
    .unwrap();
    run(data_dir.path(), &["index"]);
    assert_eq!(
        result_paths(&search(data_dir.path(), HIDDEN_TOPIC)),
        ["Beekeeping.md"]
    );
}

#[test]
fn index_and_search_go_through_a_running_service() {
    let vault = vault_copy();
    let data_dir = data_dir_with_model();
    run(data_dir.path(), &["sources", "add", path_str(vault.path())]);
    let serve = Serve::start_in(data_dir);

    run(serve.data_dir.path(), &["index"]);
    // The service embedded the notes, so its model is the one loaded.
    let (_, status) = serve.call("GET", "/status", None);
    assert_eq!(status["model"]["loaded"], true, "{status}");

    let paraphrase = &PARAPHRASES[0];
    let (status, response) = serve.call(
        "POST",
        "/search",
        Some(&json!({ "query": paraphrase.query, "limit": 3 })),
    );
    assert_eq!(status, 200, "{response}");
    assert_found_first(&response, paraphrase);
    let (_, sources) = serve.call("GET", "/sources", None);
    assert_eq!(response["results"][0]["source"], sources[0]["path"]);

    assert_found_first(&search(serve.data_dir.path(), paraphrase.query), paraphrase);

    let (status, response) = serve.call(
        "POST",
        "/search",
        Some(&json!({ "query": MATCHES_NOTHING })),
    );
    assert_eq!((status, response), (200, json!({ "results": [] })));

    let (status, response) = serve.call("POST", "/search", Some(&json!({ "query": "  " })));
    assert_eq!(status, 400, "{response}");
    let (status, response) = serve.call("POST", "/search", Some(&json!({ "text": "sleep" })));
    assert_eq!(status, 400, "{response}");

    serve.stop();
}

#[test]
fn prewarm_loads_the_model_and_unload_frees_it() {
    let serve = Serve::start_in(data_dir_with_model());
    let model_loaded = || serve.call("GET", "/status", None).1["model"]["loaded"].clone();
    assert_eq!(model_loaded(), false);

    assert_eq!(
        serve.call("POST", "/prewarm", None),
        (200, json!({ "loaded": true }))
    );
    assert_eq!(model_loaded(), true);
    assert!(run(serve.data_dir.path(), &["status"]).ends_with("model: loaded\n"));

    assert_eq!(run(serve.data_dir.path(), &["unload"]), "model unloaded\n");
    assert_eq!(model_loaded(), false);

    serve.stop();
}

#[test]
fn sources_are_added_listed_and_removed_through_the_cli_and_the_api() {
    let data_dir = tempfile::tempdir().unwrap();
    let vault = tempfile::tempdir().unwrap();
    let vault_path = path_str(vault.path());

    run(data_dir.path(), &["sources", "add", vault_path]);
    assert_eq!(
        run(data_dir.path(), &["sources", "list"]),
        format!("{vault_path}\n")
    );

    let again = trove(data_dir.path())
        .args(["sources", "add", vault_path])
        .output()
        .unwrap();
    assert!(!again.status.success());
    assert!(String::from_utf8_lossy(&again.stderr).contains("already a source"));

    let missing = vault.path().join("missing");
    let missing = trove(data_dir.path())
        .args(["sources", "add", path_str(&missing)])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("is not a folder"));

    let serve = Serve::start_in(data_dir);
    let (status, sources) = serve.call("GET", "/sources", None);
    assert_eq!(status, 200);
    assert_eq!(sources[0]["path"], vault_path);

    let (status, error) = serve.call(
        "POST",
        "/sources",
        Some(&json!({ "path": "relative/vault" })),
    );
    assert_eq!(status, 400, "{error}");
    let (status, error) = serve.call("POST", "/sources", Some(&json!({ "path": vault_path })));
    assert_eq!(status, 409, "{error}");

    run(serve.data_dir.path(), &["sources", "remove", vault_path]);
    assert_eq!(serve.call("GET", "/sources", None), (200, json!([])));

    let (status, source) = serve.call("POST", "/sources", Some(&json!({ "path": vault_path })));
    assert_eq!(status, 201, "{source}");
    let query = format!("/sources?path={}", percent_encode(vault_path));
    assert_eq!(serve.call("DELETE", &query, None), (200, source));
    let (status, error) = serve.call("DELETE", &query, None);
    assert_eq!(status, 404, "{error}");

    serve.stop();
}
