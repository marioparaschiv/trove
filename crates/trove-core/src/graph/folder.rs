use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A folder inside each source, `/`-separated, whose notes a graph leaves out as if they didn't exist, links to them
/// included. Clients name the folder they write generated notes into (the Obsidian plugin's category notes), so those
/// notes don't feed back into the graph they came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ExcludedFolder(String);

#[derive(Debug, Error)]
#[error("{0:?} is not a folder path inside a source")]
pub struct InvalidFolder(String);

impl ExcludedFolder {
    /// Whether the note at `path` (inside its source) is in the folder or one of its subfolders.
    pub(crate) fn contains(&self, path: &str) -> bool {
        path.strip_prefix(&self.0)
            .is_some_and(|rest| rest.starts_with('/'))
    }
}

impl TryFrom<String> for ExcludedFolder {
    type Error = InvalidFolder;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let folder = text.trim_matches('/');
        let valid = !folder.is_empty()
            && folder
                .split('/')
                .all(|part| !matches!(part, "" | "." | "..") && !part.contains('\\'));

        if valid {
            Ok(Self(folder.to_owned()))
        } else {
            Err(InvalidFolder(text))
        }
    }
}

impl From<ExcludedFolder> for String {
    fn from(folder: ExcludedFolder) -> Self {
        folder.0
    }
}
