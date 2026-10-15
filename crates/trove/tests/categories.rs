mod common;

use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::Path,
};

use common::{Serve, copy_of, indexed, run};
use serde_json::Value;

/// Two unrelated topics: sailing, and a larger kitchen topic of bread and cheese notes tied together by two hub notes.
const VAULT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/categories-vault"
);

fn is_kitchen(path: &str) -> bool {
    !path.starts_with("Sailing/")
}

struct Categories {
    by_id: HashMap<u64, Value>,
}

impl Categories {
    fn new(response: &Value) -> Self {
        let by_id = response["categories"]
            .as_array()
            .unwrap()
            .iter()
            .map(|category| (category["id"].as_u64().unwrap(), category.clone()))
            .collect();

        Self { by_id }
    }

    fn top_level(&self) -> impl Iterator<Item = &Value> {
        self.by_id
            .values()
            .filter(|category| category["parent"].is_null())
    }

    fn children(&self, id: u64) -> Vec<&Value> {
        self.by_id
            .values()
            .filter(|category| category["parent"].as_u64() == Some(id))
            .collect()
    }

    /// The paths of the notes in a category and its subcategories.
    fn members(&self, category: &Value) -> BTreeSet<String> {
        let mut members: BTreeSet<String> = category["notes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|note| note["path"].as_str().unwrap().to_owned())
            .collect();
        for child in self.children(category["id"].as_u64().unwrap()) {
            members.extend(self.members(child));
        }

        members
    }
}

fn name_terms(category: &Value) -> Vec<&str> {
    category["name"].as_str().unwrap().split(", ").collect()
}

fn words(note: &str) -> BTreeSet<String> {
    fs::read_to_string(Path::new(VAULT).join(note))
        .unwrap()
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .map(|word| word.trim_matches('\'').to_owned())
        .collect()
}

#[test]
fn categories_follow_the_topics_and_are_the_same_each_time() {
    let vault = copy_of(Path::new(VAULT));
    let data_dir = indexed(vault.path());
    let response: Value =
        serde_json::from_str(&run(data_dir.path(), &["categories", "--json"])).unwrap();
    let categories = Categories::new(&response);

    let all_notes: BTreeSet<String> = ["Bread", "Cheese", "Sailing"]
        .iter()
        .flat_map(|folder| fs::read_dir(Path::new(VAULT).join(folder)).unwrap())
        .map(|entry| {
            let entry = entry.unwrap();
            let folder = entry
                .path()
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_owned();
            format!(
                "{}/{}",
                folder.to_str().unwrap(),
                entry.file_name().to_str().unwrap()
            )
        })
        .chain([
            "Fermentation.md".to_owned(),
            "Kitchen projects.md".to_owned(),
        ])
        .collect();
    let categorised: BTreeSet<String> = categories
        .top_level()
        .flat_map(|category| categories.members(category))
        .collect();
    assert_eq!(categorised, all_notes);

    // Each top-level category holds one topic, and the kitchen topic is a single one with subcategories.
    let mut kitchen = Vec::new();
    for category in categories.top_level() {
        let members = categories.members(category);
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
    let subcategories = categories.children(kitchen["id"].as_u64().unwrap());
    assert!(subcategories.len() >= 2, "{kitchen:#}");
    assert!(kitchen["notes"].as_array().unwrap().is_empty());

    for category in categories.by_id.values() {
        let members = categories.members(category);
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
    for category in categories.top_level() {
        let members = categories.members(category);
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
    let serve = Serve::start_in(indexed(vault.path()));
    let (status, over_http) = serve.call("GET", "/categories", None);
    assert_eq!(status, 200);
    assert_eq!(over_http, response);
}
