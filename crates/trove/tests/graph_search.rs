mod common;

use std::{fs, path::Path};

use common::{Serve, indexed, result_paths, run, vault_copy};
use serde_json::{Value, json};

/// Finds only the hub note by meaning; the hub links to notes on unrelated topics, each through one kind of link.
const HUB_QUERY: &str = "pulling a coffee shot at home with a lever machine";
const HUB: &str = "Hobbies/Home espresso.md";

struct Target {
    note: &'static str,
    /// Where the result points: the linked heading or block, or the note's best section.
    heading_path: &'static [&'static str],
}

/// What the hub links to, one kind of link each.
const TARGETS: [Target; 6] = [
    // Wikilink to a heading.
    Target {
        note: "Travel/Kyoto.md",
        heading_path: &["Kyoto", "Arashiyama"],
    },
    // Wikilink to a block.
    Target {
        note: "Engineering/Retry backoff.md",
        heading_path: &["Retry backoff", "Jitter"],
    },
    // Embed.
    Target {
        note: "Finance/Index funds.md",
        heading_path: &["Index funds"],
    },
    // Markdown link.
    Target {
        note: "Home/Bicycle chain.md",
        heading_path: &["Bicycle chain"],
    },
    // Link in a property.
    Target {
        note: "Garden/Tomato blight.md",
        heading_path: &["Tomato blight"],
    },
    // A tag in the hub's body that the target has as a property.
    Target {
        note: "Cooking/Sourdough starter.md",
        heading_path: &["Sourdough starter"],
    },
];

fn search(data_dir: &Path, switches: &[&str]) -> Value {
    let mut args = vec!["search", "--json", HUB_QUERY];
    args.extend(switches);

    serde_json::from_str(&run(data_dir, &args)).unwrap()
}

fn result<'a>(response: &'a Value, note: &str) -> Option<&'a Value> {
    response["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|result| result["path"] == note)
}

fn assert_found_by_graph(response: &Value, target: &Target) {
    let found = result(response, target.note)
        .unwrap_or_else(|| panic!("{} not found: {response}", target.note));
    assert_eq!(found["why"], json!(["graph"]), "{response}");
    assert_eq!(
        found["heading_path"],
        json!(target.heading_path),
        "{response}"
    );
}

#[test]
fn notes_linked_from_the_match_are_found_by_the_graph_and_only_with_it() {
    let vault = vault_copy();
    let data_dir = indexed(vault.path());

    let response = search(data_dir.path(), &[]);
    assert_eq!(response["results"][0]["path"], HUB, "{response}");
    assert_eq!(response["results"][0]["why"], json!(["meaning"]));
    for target in &TARGETS {
        assert_found_by_graph(&response, target);
    }

    assert_eq!(
        result_paths(&search(data_dir.path(), &["--no-graph"])),
        [HUB]
    );

    let serve = Serve::start_in(data_dir);
    let (status, response) = serve.call(
        "POST",
        "/search",
        Some(&json!({ "query": HUB_QUERY, "skip": ["graph"] })),
    );
    serve.stop();
    assert_eq!(status, 200, "{response}");
    assert_eq!(result_paths(&response), [HUB]);
}

#[test]
fn editing_a_note_changes_only_its_own_edges() {
    let vault = vault_copy();
    let data_dir = indexed(vault.path());

    // The hub drops its embed.
    let hub = vault.path().join(HUB);
    let text = fs::read_to_string(&hub).unwrap();
    fs::write(&hub, text.replace("![[Index funds]], ", "")).unwrap();
    // A target changes a section the hub doesn't link to.
    let kyoto = vault.path().join(TARGETS[0].note);
    let text = fs::read_to_string(&kyoto).unwrap();
    fs::write(&kyoto, text.replace("Four days", "Five days")).unwrap();
    run(data_dir.path(), &["index"]);

    let response = search(data_dir.path(), &[]);
    assert!(result(&response, TARGETS[2].note).is_none(), "{response}");
    for target in TARGETS
        .iter()
        .filter(|target| target.note != TARGETS[2].note)
    {
        assert_found_by_graph(&response, target);
    }
}

#[test]
fn a_note_linked_while_the_service_runs_is_found_by_the_next_search() {
    let vault = vault_copy();
    let serve = Serve::start_indexed(indexed(vault.path()));
    let search = || {
        let (status, response) = serve.call(
            "POST",
            "/search",
            Some(&json!({ "query": HUB_QUERY, "limit": 20 })),
        );
        assert_eq!(status, 200, "{response}");
        response
    };
    let kites = Target {
        note: "Hobbies/Kites.md",
        heading_path: &["Kites"],
    };
    assert!(result(&search(), kites.note).is_none());

    fs::write(
        vault.path().join(kites.note),
        "# Kites\n\nA delta kite climbs best in a steady breeze of about 15 km/h.\n",
    )
    .unwrap();
    let hub = vault.path().join(HUB);
    let text = fs::read_to_string(&hub).unwrap();
    fs::write(
        &hub,
        text.replace("[[Kyoto#Arashiyama]]", "[[Kyoto#Arashiyama]], [[Kites]]"),
    )
    .unwrap();
    let (status, report) = serve.call("POST", "/index", Some(&json!({})));
    assert_eq!(status, 200, "{report}");

    assert_found_by_graph(&search(), &kites);
    serve.stop();
}
