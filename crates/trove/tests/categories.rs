mod common;

use std::{collections::BTreeSet, fs, path::Path};

use common::{CATEGORIES_VAULT, Serve, copy_of, fixture_notes, indexed, members, run};
use serde_json::Value;

fn is_kitchen(path: &str) -> bool {
    !path.starts_with("Sailing/")
}

fn all(response: &Value) -> &Vec<Value> {
    response["categories"].as_array().unwrap()
}

fn top_level(response: &Value) -> impl Iterator<Item = &Value> {
    all(response)
        .iter()
        .filter(|category| category["parent"].is_null())
}

fn children<'a>(response: &'a Value, category: &Value) -> Vec<&'a Value> {
    all(response)
        .iter()
        .filter(|child| child["parent"] == category["id"])
        .collect()
}

fn name_terms(category: &Value) -> Vec<&str> {
    category["name"].as_str().unwrap().split(", ").collect()
}

fn words(note: &str) -> BTreeSet<String> {
    fs::read_to_string(Path::new(CATEGORIES_VAULT).join(note))
        .unwrap()
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .map(|word| word.trim_matches('\'').to_owned())
        .collect()
}

#[test]
fn categories_follow_the_topics_and_are_the_same_each_time() {
    let vault = copy_of(Path::new(CATEGORIES_VAULT));
    let data_dir = indexed(vault.path());
    let response: Value =
        serde_json::from_str(&run(data_dir.path(), &["categories", "--json"])).unwrap();

    let all_notes = fixture_notes(Path::new(CATEGORIES_VAULT));
    let categorised: BTreeSet<String> = top_level(&response)
        .flat_map(|category| members(&response, category))
        .collect();
    assert_eq!(categorised, all_notes);

    // Each top-level category holds one topic, and the kitchen topic is a single one with subcategories.
    let mut kitchen = Vec::new();
    for category in top_level(&response) {
        let members = members(&response, category);
        let topics: BTreeSet<bool> = members.iter().map(|path| is_kitchen(path)).collect();
        assert_eq!(topics.len(), 1, "{members:?} mixes the topics");
        if topics.contains(&true) {
            kitchen.push(category);
        }
    }
    let [kitchen] = kitchen[..] else {
        panic!(
            "the kitchen notes are in {} top-level categories",
            kitchen.len()
        );
    };
    let subcategories = children(&response, kitchen);
    assert!(subcategories.len() >= 2, "{kitchen:#}");
    assert!(kitchen["notes"].as_array().unwrap().is_empty());

    for category in all(&response) {
        let members = members(&response, category);
        let start_here = category["start_here"]["path"].as_str().unwrap();
        assert!(
            members.contains(start_here),
            "{start_here} is not in {members:?}"
        );

        // Every name term comes from the category's notes.
        let member_words: BTreeSet<String> = members.iter().flat_map(|note| words(note)).collect();
        for term in name_terms(category) {
            assert!(
                member_words.contains(term),
                "{term} is in none of {members:?}"
            );
        }
    }

    // Top-level names use no word of the other topic.
    for category in top_level(&response) {
        let members = members(&response, category);
        let other_topic: BTreeSet<String> = all_notes
            .iter()
            .filter(|note| is_kitchen(note) != members.iter().any(|path| is_kitchen(path)))
            .flat_map(|note| words(note))
            .collect();
        for term in name_terms(category) {
            assert!(!other_topic.contains(term), "{term} is in the other topic");
        }
    }

    let plain = run(data_dir.path(), &["categories"]);
    // The kitchen category is at the top level, so its line has no indent.
    let kitchen_line = format!(
        "{}  (start here: {})",
        kitchen["name"].as_str().unwrap(),
        kitchen["start_here"]["path"].as_str().unwrap()
    );
    assert!(plain.lines().any(|line| line == kitchen_line), "{plain}");
    for note in &all_notes {
        assert!(plain.contains(&format!("- {note}\n")), "{plain}");
    }

    // A second data folder on the same vault, read over HTTP, gives the same hierarchy.
    let serve = Serve::start_indexed(indexed(vault.path()));
    let (status, over_http) = serve.call("GET", "/categories", None);
    assert_eq!(status, 200);
    assert_eq!(over_http, response);
}

/// The body of a category note as the Obsidian plugin writes it: Bases views of its subcategories and notes.
const CATEGORY_NOTE: &str = r#"## Subcategories

```base
filters:
  and:
    - list(parent).contains(this)
views:
  - type: table
    name: Subcategories
```

## Notes

```base
filters:
  and:
    - list(category).contains(this)
views:
  - type: table
    name: Notes
```
"#;

/// Adds `name: "value"` to the note's frontmatter, as Obsidian's frontmatter API would.
fn set_property(note: &Path, name: &str, value: &str) {
    let text = fs::read_to_string(note).unwrap();
    let property = format!("{name}: \"{value}\"\n");
    let text = match text.strip_prefix("---\n") {
        Some(rest) => format!("---\n{property}{rest}"),
        None => format!("---\n{property}---\n{text}"),
    };
    fs::write(note, text).unwrap();
}

#[test]
fn category_notes_in_the_excluded_folder_leave_the_categories_unchanged() {
    let vault = copy_of(Path::new(CATEGORIES_VAULT));
    let serve = Serve::start_indexed(indexed(vault.path()));
    let excluding = "/categories?exclude_folder=Categories";
    let (status, before) = serve.call("GET", excluding, None);
    assert_eq!(status, 200, "{before}");
    assert_eq!(serve.call("GET", "/categories", None).1, before);

    // What the plugin writes: a category note per category, linking to its parent, and `category` on each note.
    let categories = before["categories"].as_array().unwrap();
    let link = |id: &Value| {
        let category = categories
            .iter()
            .find(|category| &category["id"] == id)
            .unwrap();
        format!("[[Categories/{}]]", category["name"].as_str().unwrap())
    };
    let folder = vault.path().join("Categories");
    fs::create_dir(&folder).unwrap();
    for category in categories {
        let note = folder.join(format!("{}.md", category["name"].as_str().unwrap()));
        fs::write(&note, CATEGORY_NOTE).unwrap();
        if !category["parent"].is_null() {
            set_property(&note, "parent", &link(&category["parent"]));
        }
        for member in category["notes"].as_array().unwrap() {
            let path = vault.path().join(member["path"].as_str().unwrap());
            set_property(&path, "category", &link(&category["id"]));
        }
    }
    let (status, report) = serve.call("POST", "/index", Some(&serde_json::json!({})));
    assert_eq!(status, 200, "{report}");

    let (status, after) = serve.call("GET", excluding, None);
    assert_eq!(status, 200, "{after}");
    assert_eq!(after, before);

    let (status, response) = serve.call("GET", "/categories?exclude_folder=..", None);
    assert_eq!(status, 400, "{response}");
}
