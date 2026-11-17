mod common;

use std::fs;

use common::{indexed, run, search_with_switches};
use serde_json::{Value, json};

const NOTE: &str = "Boiler.md";
const TEXT: &str = "# Boiler\n\nYearly check in October.\n\n## Pressure\n\nThe gauge should read 1.5 bar when cold.\n\
                    Top up the pressure from the filling loop.\n\n## Radiators\n\nBleed the radiators when the \
                    pressure drops.\n";

/// The words a snippet marks.
fn highlighted(found: &Value) -> Vec<String> {
    let snippet = &found["snippet"];
    let text = snippet["text"].as_str().unwrap();
    snippet["highlights"]
        .as_array()
        .unwrap()
        .iter()
        .map(|range| {
            let (start, end) = (
                range["start"].as_u64().unwrap(),
                range["end"].as_u64().unwrap(),
            );
            text[start as usize..end as usize].to_owned()
        })
        .collect()
}

#[test]
fn each_match_has_its_byte_span_lines_headings_and_marked_words() {
    let vault = tempfile::tempdir().unwrap();
    fs::write(vault.path().join(NOTE), TEXT).unwrap();
    let data_dir = indexed(vault.path());

    let response = search_with_switches(data_dir.path(), "pressure", &["--no-meaning"]);
    let result = &response["results"][0];
    assert_eq!(result["path"], NOTE, "{response}");
    assert_eq!(result["kind"], "markdown");

    let matches = result["matches"].as_array().unwrap();
    let headings: Vec<&Value> = matches.iter().map(|found| &found["heading_path"]).collect();
    assert_eq!(
        headings,
        [
            &json!(["Boiler", "Pressure"]),
            &json!(["Boiler", "Radiators"])
        ],
        "{response}"
    );
    for found in matches {
        let location = &found["location"];
        assert_eq!(location["type"], "text");
        let (start, end) = (
            location["byte_start"].as_u64().unwrap() as usize,
            location["byte_end"].as_u64().unwrap() as usize,
        );
        let line_of = |at: usize| 1 + TEXT[..at].matches('\n').count() as u64;
        assert_eq!(location["line_start"], line_of(start), "{found}");
        assert_eq!(location["line_end"], line_of(end - 1), "{found}");
        assert!(
            TEXT[start..end].contains(found["snippet"]["text"].as_str().unwrap()),
            "{found}"
        );
        assert!(
            highlighted(found)
                .iter()
                .all(|word| word.eq_ignore_ascii_case("pressure")),
            "{found}"
        );
        assert!(!highlighted(found).is_empty(), "{found}");
    }

    // A misspelt word is marked as the word it stands for.
    let response = search_with_switches(data_dir.path(), "presure gauge", &["--no-meaning"]);
    let best = &response["results"][0]["matches"][0];
    assert_eq!(
        best["heading_path"],
        json!(["Boiler", "Pressure"]),
        "{response}"
    );
    assert_eq!(highlighted(best), ["gauge", "pressure"], "{response}");

    let plain = run(
        data_dir.path(),
        &["search", "pressure", "--no-meaning", "--no-graph"],
    );
    let lines: Vec<&str> = plain.lines().collect();
    assert!(
        lines[0].ends_with("Boiler.md:7-8 > Boiler > Pressure  (words)"),
        "{plain}"
    );
    assert_eq!(
        lines[1], "        Boiler.md:12 > Boiler > Radiators",
        "{plain}"
    );
}

#[test]
fn a_file_changed_since_it_was_indexed_has_no_lines() {
    let vault = tempfile::tempdir().unwrap();
    fs::write(vault.path().join(NOTE), TEXT).unwrap();
    let data_dir = indexed(vault.path());
    fs::write(vault.path().join(NOTE), format!("Added first.\n\n{TEXT}")).unwrap();

    let response = search_with_switches(data_dir.path(), "radiators", &["--no-meaning"]);
    let location = &response["results"][0]["matches"][0]["location"];
    assert_eq!(location["type"], "text", "{response}");
    assert!(location.get("byte_start").is_some(), "{response}");
    assert!(location.get("line_start").is_none(), "{response}");
}
