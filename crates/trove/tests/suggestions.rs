mod common;

use std::{fs, path::Path};

use common::{RELATED_VAULT, Serve, copy_of, indexed, path_str, percent_encode, remove_index, run};
use serde_json::json;
use trove_core::sync_store::{SuggestionState, SyncStore, note_pair};

const ESPRESSO: &str = "Coffee/Espresso.md";
/// Close in meaning to `ESPRESSO`, with no link either way.
const POUR_OVER: &str = "Coffee/Pour over.md";
/// Linked from `ESPRESSO`.
const LISBON: &str = "Travel/Lisbon.md";

fn suggested_pairs(serve: &Serve, vault: &Path) -> Vec<[String; 2]> {
    let path = format!("/suggestions?source={}", percent_encode(path_str(vault)));
    let (status, response) = serve.call("GET", &path, None);
    assert_eq!(status, 200, "{response}");

    response["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|suggestion| {
            [
                suggestion["note"]["path"].as_str().unwrap().to_owned(),
                suggestion["target"]["path"].as_str().unwrap().to_owned(),
            ]
        })
        .collect()
}

fn is_suggested(serve: &Serve, vault: &Path, a: &str, b: &str) -> bool {
    suggested_pairs(serve, vault).contains(&[a.to_owned(), b.to_owned()])
}

fn decide(serve: &Serve, vault: &Path, note: &str, target: &str, state: &str) {
    let body = json!({ "source": path_str(vault), "note": note, "target": target, "state": state });
    let (status, response) = serve.request("POST", "/suggestions/decisions", Some(&body));
    assert_eq!(status, 204, "{response}");
}

fn orphans(serve: &Serve, query: &str) -> Vec<String> {
    let (status, response) = serve.call("GET", &format!("/orphans?{query}"), None);
    assert_eq!(status, 200, "{response}");

    response["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|note| note["path"].as_str().unwrap().to_owned())
        .collect()
}

fn index(serve: &Serve) {
    let (status, report) = serve.call("POST", "/index", Some(&json!({})));
    assert_eq!(status, 200, "{report}");
}

#[test]
fn similar_unlinked_notes_are_suggested_until_linked_or_decided() {
    let vault = copy_of(Path::new(RELATED_VAULT));
    let serve = Serve::start_indexed(indexed(vault.path()));

    let path = format!(
        "/suggestions?source={}",
        percent_encode(path_str(vault.path()))
    );
    let (status, response) = serve.call("GET", &path, None);
    assert_eq!(status, 200, "{response}");
    let suggestion = &response["suggestions"][0];
    assert_eq!(suggestion["note"]["path"], ESPRESSO, "{response}");
    assert_eq!(suggestion["note"]["title"], "Espresso");
    assert_eq!(suggestion["note"]["source"], path_str(vault.path()));
    assert_eq!(suggestion["target"]["path"], POUR_OVER);
    let similarities: Vec<f64> = response["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|suggestion| suggestion["similarity"].as_f64().unwrap())
        .collect();
    assert!(
        similarities.is_sorted_by(|a, b| a >= b),
        "most similar first: {response}"
    );
    assert!(!is_suggested(&serve, vault.path(), ESPRESSO, LISBON));

    // What accepting in Obsidian does: a wikilink at the end of the note, then the decision.
    let espresso = vault.path().join(ESPRESSO);
    let text = fs::read_to_string(&espresso).unwrap();
    fs::write(&espresso, format!("{text}\n[[Pour over]]\n")).unwrap();
    index(&serve);
    assert!(!is_suggested(&serve, vault.path(), ESPRESSO, POUR_OVER));
    decide(&serve, vault.path(), POUR_OVER, ESPRESSO, "accepted");
    assert_eq!(merged_state(vault.path()), Some(SuggestionState::Accepted));

    serve.stop();
}

#[test]
fn an_accepted_suggestion_is_not_suggested_again() {
    let vault = copy_of(Path::new(RELATED_VAULT));
    let serve = Serve::start_indexed(indexed(vault.path()));

    decide(&serve, vault.path(), POUR_OVER, ESPRESSO, "accepted");
    assert!(!is_suggested(&serve, vault.path(), ESPRESSO, POUR_OVER));

    serve.stop();
}

#[test]
fn a_dismissed_suggestion_stays_dismissed_after_the_index_is_rebuilt() {
    let vault = copy_of(Path::new(RELATED_VAULT));
    let serve = Serve::start_indexed(indexed(vault.path()));
    decide(&serve, vault.path(), ESPRESSO, POUR_OVER, "dismissed");
    assert!(!is_suggested(&serve, vault.path(), ESPRESSO, POUR_OVER));
    let data_dir = serve.stop_keeping_data();

    remove_index(data_dir.path());
    run(data_dir.path(), &["sources", "add", path_str(vault.path())]);
    let rebuilt = Serve::start_indexed(data_dir);
    assert!(!is_suggested(&rebuilt, vault.path(), ESPRESSO, POUR_OVER));

    rebuilt.stop();
}

/// The merged decision about the pair, as every device reads it from the vault's decisions files.
fn merged_state(vault: &Path) -> Option<SuggestionState> {
    SyncStore::new(vault, ".trove")
        .suggestion_states()
        .unwrap()
        .get(&note_pair(ESPRESSO, POUR_OVER))
        .copied()
}

#[test]
fn decisions_from_two_data_folders_merge_and_the_latest_wins() {
    let vault = copy_of(Path::new(RELATED_VAULT));
    let first = Serve::start_indexed(indexed(vault.path()));
    let second = Serve::start_indexed(indexed(vault.path()));

    decide(&first, vault.path(), ESPRESSO, POUR_OVER, "dismissed");
    assert!(!is_suggested(&second, vault.path(), ESPRESSO, POUR_OVER));
    assert_eq!(merged_state(vault.path()), Some(SuggestionState::Dismissed));

    decide(&second, vault.path(), POUR_OVER, ESPRESSO, "accepted");
    assert_eq!(merged_state(vault.path()), Some(SuggestionState::Accepted));

    decide(&first, vault.path(), ESPRESSO, POUR_OVER, "dismissed");
    assert_eq!(merged_state(vault.path()), Some(SuggestionState::Dismissed));
    assert!(!is_suggested(&first, vault.path(), ESPRESSO, POUR_OVER));
    assert!(!is_suggested(&second, vault.path(), ESPRESSO, POUR_OVER));

    let files = fs::read_dir(vault.path().join(".trove/decisions"))
        .unwrap()
        .count();
    assert_eq!(files, 2, "one decisions file per data folder");

    first.stop();
    second.stop();
}

#[test]
fn orphans_are_the_notes_with_no_link_in_or_out() {
    let vault = copy_of(Path::new(RELATED_VAULT));
    let serve = Serve::start_indexed(indexed(vault.path()));
    let source = format!("source={}", percent_encode(path_str(vault.path())));

    let expected = [
        "Astronomy/Telescope.md",
        "Coffee/Pour over.md",
        "Sailing/Knots.md",
        "Taxes/Deductions.md",
    ];
    assert_eq!(orphans(&serve, &source), expected);

    // A link into the excluded folder counts only without the exclusion.
    fs::create_dir(vault.path().join("Categories")).unwrap();
    fs::write(vault.path().join("Categories/Coffee.md"), "# Coffee\n").unwrap();
    let pour_over = vault.path().join(POUR_OVER);
    let text = fs::read_to_string(&pour_over).unwrap();
    fs::write(
        &pour_over,
        format!("---\ncategory: \"[[Categories/Coffee]]\"\n---\n{text}"),
    )
    .unwrap();
    index(&serve);
    let all: Vec<String> = orphans(&serve, &source);
    assert!(!all.contains(&POUR_OVER.to_owned()), "{all:?}");
    assert_eq!(
        orphans(&serve, &format!("{source}&exclude_folder=Categories/")),
        expected
    );

    serve.stop();
}

#[test]
fn bad_requests_are_refused() {
    let vault = copy_of(Path::new(RELATED_VAULT));
    let serve = Serve::start_indexed(indexed(vault.path()));
    let source = percent_encode(path_str(vault.path()));

    for (path, expected) in [
        (format!("/suggestions?source={source}&limit=0"), 400),
        (
            format!("/suggestions?source={source}&exclude_folder=%2F"),
            400,
        ),
        ("/suggestions".to_owned(), 400),
        ("/suggestions?source=%2Fnowhere".to_owned(), 404),
        ("/orphans?source=%2Fnowhere".to_owned(), 404),
    ] {
        let (status, response) = serve.call("GET", &path, None);
        assert_eq!(status, expected, "{path}: {response}");
    }

    let decision = |note: &str, target: &str, state: &str| json!({ "source": path_str(vault.path()), "note": note, "target": target, "state": state });
    for (body, expected) in [
        (decision(ESPRESSO, ESPRESSO, "dismissed"), 400),
        (decision(ESPRESSO, "Coffee/Missing.md", "dismissed"), 404),
        (decision(ESPRESSO, POUR_OVER, "maybe"), 400),
    ] {
        let (status, response) = serve.call("POST", "/suggestions/decisions", Some(&body));
        assert_eq!(status, expected, "{body}: {response}");
    }
    assert!(!vault.path().join(".trove/decisions").exists());

    serve.stop();
}
