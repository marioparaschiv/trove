mod common;

use std::{collections::BTreeSet, fs, path::Path};

use common::{Serve, copy_of, indexed, path_str, run};
use serde_json::{Value, json};
use tempfile::TempDir;

/// Two unrelated topics: sailing, and a larger kitchen topic of bread and cheese notes tied together by two hub notes.
const VAULT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/categories-vault"
);

/// In the sail handling category, which the navigation category is merged into.
const POINTS_OF_SAIL: &str = "Sailing/Points of sail.md";
const CHART_NAVIGATION: &str = "Sailing/Chart navigation.md";
/// In the bread category, and moved by hand to the cheese category.
const MOZZARELLA: &str = "Cheese/Mozzarella stretching.md";
const RENNET: &str = "Cheese/Rennet and curds.md";
/// A sailing note added after the pins.
const HEAVING_TO: &str = "Sailing/Heaving to.md";

/// Starts a service on `data_dir` and waits out its startup index run.
fn start(data_dir: TempDir) -> Serve {
    let serve = Serve::start_in(data_dir);
    let mut events = serve.events();
    let (_, status) = serve.call("GET", "/status", None);
    if status["indexing"]["busy"] == true {
        events.until("index_finished");
    }

    serve
}

fn categories(serve: &Serve) -> Value {
    let (status, response) = serve.call("GET", "/categories", None);
    assert_eq!(status, 200, "{response}");

    response
}

fn pin(serve: &Serve, vault: &Path, pins: &Value) {
    let body = json!({ "source": path_str(vault), "pins": pins });
    let (status, response) = serve.request("PUT", "/categories/pins", Some(&body));
    assert_eq!(status, 204, "{response}");
}

fn all(response: &Value) -> &Vec<Value> {
    response["categories"].as_array().unwrap()
}

/// The category listing the note.
fn category_of<'a>(response: &'a Value, path: &str) -> &'a Value {
    all(response)
        .iter()
        .find(|category| {
            category["notes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|note| note["path"] == path)
        })
        .unwrap_or_else(|| panic!("{path} is in no category: {response:#}"))
}

fn named<'a>(response: &'a Value, name: &str) -> &'a Value {
    all(response)
        .iter()
        .find(|category| category["name"] == name)
        .unwrap_or_else(|| panic!("no category is named {name}: {response:#}"))
}

/// The paths of the notes in a category and its subcategories.
fn members(response: &Value, category: &Value) -> BTreeSet<String> {
    let mut members: BTreeSet<String> = category["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|note| note["path"].as_str().unwrap().to_owned())
        .collect();
    for child in all(response)
        .iter()
        .filter(|child| child["parent"] == category["id"])
    {
        members.extend(self::members(response, child));
    }

    members
}

/// What every run after the pins must show: the renamed, merged sailing category holding every sailing note, and the
/// moved note in the cheese category, which is moved to the top level.
fn assert_pins_hold(response: &Value) {
    let sailing = named(response, "Sailing");
    assert!(sailing["parent"].is_null(), "{response:#}");
    let sailing_notes = members(response, sailing);
    for note in [POINTS_OF_SAIL, CHART_NAVIGATION] {
        assert!(sailing_notes.contains(note), "{response:#}");
    }
    let other_sailing: Vec<&Value> = all(response)
        .iter()
        .filter(|category| category["name"] != "Sailing" && category["parent"].is_null())
        .filter(|category| {
            members(response, category)
                .iter()
                .any(|note| note.starts_with("Sailing/"))
        })
        .collect();
    assert!(other_sailing.is_empty(), "{response:#}");

    let cheese = category_of(response, MOZZARELLA);
    assert_eq!(cheese["name"], "Cheese", "{response:#}");
    assert_eq!(category_of(response, RENNET)["name"], "Cheese");
    assert!(cheese["parent"].is_null(), "{response:#}");
}

#[test]
fn pinned_renames_moves_and_merges_hold_after_later_runs_and_on_another_device() {
    let vault = copy_of(Path::new(VAULT));
    let serve = start(indexed(vault.path()));
    let before = categories(&serve);

    let sail_handling = category_of(&before, POINTS_OF_SAIL);
    let navigation = category_of(&before, CHART_NAVIGATION);
    assert_ne!(sail_handling["id"], navigation["id"], "{before:#}");
    let cheese = category_of(&before, RENNET);
    assert_ne!(category_of(&before, MOZZARELLA)["id"], cheese["id"]);

    // Renaming the sail handling category, as the plugin sends it when its category note is renamed.
    let sail_handling_notes = members(&before, sail_handling);
    pin(
        &serve,
        vault.path(),
        &json!([{
            "type": "category",
            "name": "Sailing",
            "notes": sail_handling_notes,
            "replaces": [sail_handling["name"]],
        }]),
    );
    let renamed = categories(&serve);
    let sailing = named(&renamed, "Sailing");
    assert_eq!(members(&renamed, sailing), sail_handling_notes);
    assert_eq!(
        category_of(&renamed, CHART_NAVIGATION)["name"],
        navigation["name"]
    );

    // Merging navigation into it, moving a bread note to the cheese category by hand, and that category to the top.
    let mut merged = sail_handling_notes.clone();
    merged.extend(members(&before, navigation));
    let mut cheese_notes = members(&before, cheese);
    cheese_notes.insert(MOZZARELLA.to_owned());
    pin(
        &serve,
        vault.path(),
        &json!([
            {
                "type": "category",
                "name": "Sailing",
                "notes": merged,
                "replaces": [navigation["name"]],
            },
            { "type": "category", "name": "Cheese", "notes": cheese_notes },
            { "type": "note", "path": MOZZARELLA, "category": "Cheese" },
            { "type": "parent", "category": "Cheese", "parent": null },
        ]),
    );
    assert_pins_hold(&categories(&serve));

    // The next run, on a vault with one more sailing note.
    fs::write(
        vault.path().join(HEAVING_TO),
        "# Heaving to\n\nHeaving to stops the boat in strong wind: tack without releasing the jib sheet, so the \
         backed jib and the mainsail balance and the boat drifts slowly. See [[Tacking]] and [[Reefing the \
         mainsail]].\n",
    )
    .unwrap();
    let (status, report) = serve.call("POST", "/index", Some(&json!({})));
    assert_eq!(status, 200, "{report}");
    let after = categories(&serve);
    assert_pins_hold(&after);
    assert!(
        members(&after, named(&after, "Sailing")).contains(HEAVING_TO),
        "{after:#}"
    );

    // A second device (data folder) on the same vault reads the pins from the vault's store.
    let second_device = indexed(vault.path());
    let response: Value =
        serde_json::from_str(&run(second_device.path(), &["categories", "--json"])).unwrap();
    assert_pins_hold(&response);
    assert_eq!(response, after);
}

#[test]
fn pins_need_a_source_and_names() {
    let vault = copy_of(Path::new(VAULT));
    let serve = start(indexed(vault.path()));
    let note_pin =
        |category: &str| json!([{ "type": "note", "path": RENNET, "category": category }]);

    let body = json!({ "source": "/not/a/source", "pins": note_pin("Cheese") });
    let (status, response) = serve.request("PUT", "/categories/pins", Some(&body));
    assert_eq!(status, 404, "{response}");

    let body = json!({ "source": path_str(vault.path()), "pins": note_pin(" ") });
    let (status, response) = serve.request("PUT", "/categories/pins", Some(&body));
    assert_eq!(status, 400, "{response}");

    let body = json!({ "source": path_str(vault.path()), "pins": [{ "type": "rename" }] });
    let (status, response) = serve.request("PUT", "/categories/pins", Some(&body));
    assert_eq!(status, 400, "{response}");

    assert!(!vault.path().join(".trove/decisions").exists());
}
