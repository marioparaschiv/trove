mod common;

use std::path::Path;

use common::{Serve, copy_of, indexed, path_str, percent_encode, result_paths};
use serde_json::{Value, json};
use tempfile::TempDir;

/// Notes on coffee, a trip, and three topics unrelated to either.
const RELATED_VAULT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/related-vault");

const NOTES: [&str; 6] = [
    "Coffee/Espresso.md",
    "Coffee/Pour over.md",
    "Travel/Lisbon.md",
    "Sailing/Knots.md",
    "Astronomy/Telescope.md",
    "Taxes/Deductions.md",
];
/// Linked from `Coffee/Espresso.md`, on another topic.
const LINKED: &str = "Travel/Lisbon.md";
/// Close in meaning to `Coffee/Espresso.md`, with no link either way.
const CLOSE: &str = "Coffee/Pour over.md";
const UNRELATED: [&str; 3] = [
    "Sailing/Knots.md",
    "Astronomy/Telescope.md",
    "Taxes/Deductions.md",
];

/// A service over an indexed copy of the related-notes vault.
struct RelatedVault {
    serve: Serve,
    vault: TempDir,
}

impl RelatedVault {
    fn start() -> Self {
        let vault = copy_of(Path::new(RELATED_VAULT));
        let serve = Serve::start_in(indexed(vault.path()));

        Self { serve, vault }
    }

    fn source(&self) -> &str {
        path_str(self.vault.path())
    }

    fn related(&self, query: &str) -> (u16, Value) {
        let path = format!("/related?source={}&{query}", percent_encode(self.source()));

        self.serve.call("GET", &path, None)
    }

    fn related_to(&self, note: &str) -> (u16, Value) {
        self.related(&format!("path={}", percent_encode(note)))
    }
}

#[test]
fn linked_and_close_notes_rank_above_unrelated_ones() {
    let vault = RelatedVault::start();

    let (status, response) = vault.related_to("Coffee/Espresso.md");
    assert_eq!(status, 200, "{response}");
    let paths = result_paths(&response);
    let mut top: Vec<&str> = paths.iter().take(2).copied().collect();
    top.sort_unstable();
    assert_eq!(top, [CLOSE, LINKED], "{response}");
    for note in UNRELATED {
        let place = paths.iter().position(|&path| path == note);
        assert!(place.is_none_or(|place| place >= 2), "{note}: {response}");
    }

    let best = &response["results"][0];
    assert_eq!(best["source"], vault.source());
    assert_eq!(best["score"], 1.0, "{response}");
    assert_eq!(best["why"], json!(["graph"]), "{response}");

    vault.serve.stop();
}

#[test]
fn a_note_is_never_in_its_own_list() {
    let vault = RelatedVault::start();

    for note in NOTES {
        let (status, response) = vault.related_to(note);
        assert_eq!(status, 200, "{note}: {response}");
        assert!(
            !result_paths(&response).contains(&note),
            "{note}: {response}"
        );
    }

    vault.serve.stop();
}

#[test]
fn bad_requests_are_refused() {
    let vault = RelatedVault::start();

    let (status, response) = vault.related_to("Coffee/Missing.md");
    assert_eq!(status, 404, "{response}");
    let (status, response) = vault.related(&format!("path={}&limit=0", percent_encode(LINKED)));
    assert_eq!(status, 400, "{response}");
    let (status, response) = vault
        .serve
        .call("GET", "/related?path=Travel%2FLisbon.md", None);
    assert_eq!(status, 400, "{response}");

    vault.serve.stop();
}
