use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::sync_store::CategoryPin;

/// The category pins in force once later pins have overridden earlier ones and every name is followed through the
/// renames and merges that replaced it.
#[derive(Default)]
pub(super) struct Pins {
    /// Each pinned category's notes, by file id, which find it among a run's communities.
    pub(super) anchors: BTreeMap<String, BTreeSet<i64>>,
    /// Pinned categories placed by hand: under another pinned category, or at the top level for `None`.
    pub(super) parents: HashMap<String, Option<String>>,
    /// Notes placed by hand in a pinned category, by file id.
    pub(super) placed: HashMap<i64, String>,
}

enum Name {
    Pinned(BTreeSet<i64>),
    ReplacedBy(String),
}

impl Pins {
    /// `pins` oldest first, each with its source. `file_id` gives the file id of a note by its source and path, or
    /// `None` for a note left out of the categories.
    pub(super) fn resolve(
        pins: impl IntoIterator<Item = (String, CategoryPin)>,
        file_id: impl Fn(&str, &str) -> Option<i64>,
    ) -> Self {
        let mut names: HashMap<String, Name> = HashMap::new();
        let mut parents: Vec<(String, Option<String>)> = Vec::new();
        let mut placed: Vec<(i64, String)> = Vec::new();
        for (source, pin) in pins {
            let file_id = |path: &str| file_id(&source, path);
            match pin {
                CategoryPin::Category {
                    name,
                    notes,
                    replaces,
                } => {
                    for replaced in replaces.into_iter().filter(|replaced| *replaced != name) {
                        names.insert(replaced, Name::ReplacedBy(name.clone()));
                    }
                    let anchor = notes.iter().filter_map(|path| file_id(path)).collect();
                    names.insert(name, Name::Pinned(anchor));
                }
                CategoryPin::Parent { category, parent } => parents.push((category, parent)),
                CategoryPin::Note { path, category } => {
                    if let Some(file_id) = file_id(&path) {
                        placed.push((file_id, category));
                    }
                }
            }
        }

        // Replacements can't loop: of the names in a loop, the one pinned last would be pinned still.
        let current = |name: &str| -> Option<String> {
            let mut name = name;
            loop {
                match names.get(name)? {
                    Name::Pinned(_) => return Some(name.to_owned()),
                    Name::ReplacedBy(next) => name = next,
                }
            }
        };

        let mut resolved = Self::default();
        for (category, parent) in parents {
            let Some(category) = current(&category) else {
                continue;
            };
            match parent.map(|parent| current(&parent)) {
                None => {
                    resolved.parents.insert(category, None);
                }
                Some(Some(parent)) if parent != category => {
                    resolved.parents.insert(category, Some(parent));
                }
                Some(_) => {
                    resolved.parents.remove(&category);
                }
            }
        }
        for (file_id, category) in placed {
            match current(&category) {
                Some(category) => resolved.placed.insert(file_id, category),
                None => resolved.placed.remove(&file_id),
            };
        }
        resolved.anchors = names
            .into_iter()
            .filter_map(|(name, state)| match state {
                Name::Pinned(anchor) => Some((name, anchor)),
                Name::ReplacedBy(_) => None,
            })
            .collect();

        resolved
    }
}
