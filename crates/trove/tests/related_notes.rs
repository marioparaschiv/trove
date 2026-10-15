mod common;

use std::path::Path;

use common::{Serve, indexed, path_str, percent_encode, result_paths};
use serde_json::{Value, json};

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

fn serve_related_vault() -> Serve {
    Serve::start_in(indexed(Path::new(RELATED_VAULT)))
}

fn related(serve: &Serve, note: &str) -> (u16, Value) {
    let path = format!(
        "/related?source={}&path={}",
        percent_encode(RELATED_VAULT),
        percent_encode(note)
    );

    serve.call("GET", &path, None)
}

#[test]
fn linked_and_close_notes_rank_above_unrelated_ones() {
    let serve = serve_related_vault();

    let (status, response) = related(&serve, "Coffee/Espresso.md");
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
    assert_eq!(best["source"], path_str(Path::new(RELATED_VAULT)));
    assert_eq!(best["score"], 1.0, "{response}");
    assert_eq!(best["why"], json!(["graph"]), "{response}");

    serve.stop();
}

#[test]
fn a_note_is_never_in_its_own_list() {
    let serve = serve_related_vault();

    for note in NOTES {
        let (status, response) = related(&serve, note);
        assert_eq!(status, 200, "{note}: {response}");
        assert!(
            !result_paths(&response).contains(&note),
            "{note}: {response}"
        );
    }

    serve.stop();
}

#[test]
fn bad_requests_are_refused() {
    let serve = serve_related_vault();

    let (status, response) = related(&serve, "Coffee/Missing.md");
    assert_eq!(status, 404, "{response}");
    let (status, response) = serve.call(
        "GET",
        &format!(
            "/related?source={}&path={}&limit=0",
            percent_encode(RELATED_VAULT),
            percent_encode(LINKED)
        ),
        None,
    );
    assert_eq!(status, 400, "{response}");
    let (status, response) = serve.call("GET", "/related?path=Travel%2FLisbon.md", None);
    assert_eq!(status, 400, "{response}");

    serve.stop();
}
