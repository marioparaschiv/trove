//! Decisions the user made, kept in the store so they hold on every device and survive a rebuilt index. Each device
//! appends to its own `<store>/decisions/<device id>.jsonl`, one JSON object per line with the time it was made
//! (`at`, RFC 3339), so sync tools never see two devices write one file. Reading merges every device's file, and the
//! latest decision about a thing wins.

use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::ToSchema;

use super::{StoreError, SyncStore, write_atomically};

const DECISIONS_FOLDER: &str = "decisions";
const EXTENSION: &str = "jsonl";

/// What the user decided about a suggested link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionState {
    /// Linked by the user.
    Accepted,
    /// Never to be suggested again.
    Dismissed,
}

/// A manual category edit, which later runs keep. Categories are named, and notes given by their paths in the source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CategoryPin {
    /// A category the user named or kept. Later runs find it by its notes (subcategories included): it takes every
    /// category most of whose notes, and most of each of whose subcategories' notes, are among them. A rename or a
    /// merge lists the names it replaces, and pins naming those then mean this one.
    Category {
        name: String,
        notes: Vec<String>,
        #[serde(default)]
        replaces: Vec<String>,
    },
    /// The category is a subcategory of `parent`, a pinned category, or at the top level when `parent` is null.
    Parent {
        category: String,
        parent: Option<String>,
    },
    /// The note is in the category, a pinned one, and in none of its subcategories.
    Note { path: String, category: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Decision {
    /// About the link suggested between two notes, by their paths in the source, in sorted order.
    Suggestion {
        notes: [String; 2],
        state: SuggestionState,
    },
    CategoryPin {
        pin: CategoryPin,
    },
}

impl Decision {
    pub fn suggestion(a: &str, b: &str, state: SuggestionState) -> Self {
        Self::Suggestion {
            notes: note_pair(a, b),
            state,
        }
    }
}

/// Two notes' paths in sorted order, which names their pair whichever way round it was given.
pub fn note_pair(a: &str, b: &str) -> [String; 2] {
    let mut notes = [a.to_owned(), b.to_owned()];
    notes.sort();
    notes
}

#[derive(Serialize, Deserialize)]
struct Record {
    at: String,
    #[serde(flatten)]
    decision: Decision,
}

#[derive(Debug, Error)]
pub enum DecisionError {
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("`at` is not an RFC 3339 time")]
    Time(#[from] humantime::TimestampError),
}

impl SyncStore {
    /// Appends a decision made now on the device `device_id`.
    pub fn record_decision(&self, device_id: &str, decision: Decision) -> Result<(), StoreError> {
        self.record_decisions(device_id, [decision])
    }

    /// Appends decisions made now on the device `device_id`, in order, so a later one wins over an earlier one.
    pub fn record_decisions(
        &self,
        device_id: &str,
        decisions: impl IntoIterator<Item = Decision>,
    ) -> Result<(), StoreError> {
        let path = self
            .root
            .join(DECISIONS_FOLDER)
            .join(format!("{device_id}.{EXTENSION}"));
        let mut bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(source) => return Err(StoreError::Read { path, source }),
        };
        if bytes.last().is_some_and(|&byte| byte != b'\n') {
            bytes.push(b'\n');
        }

        let at = humantime::format_rfc3339_nanos(SystemTime::now()).to_string();
        for decision in decisions {
            let record = Record {
                at: at.clone(),
                decision,
            };
            serde_json::to_writer(&mut bytes, &record).expect("a decision always serialises");
            bytes.push(b'\n');
        }

        // The whole file is rewritten rather than appended to, so a sync tool never picks up half a line.
        write_atomically(&path, &bytes)
    }

    /// The latest state of every suggestion any device decided about, by its pair of notes.
    pub fn suggestion_states(&self) -> Result<HashMap<[String; 2], SuggestionState>, StoreError> {
        let mut latest = HashMap::new();
        for decision in self.decisions()? {
            if let Decision::Suggestion {
                notes: [a, b],
                state,
            } = decision
            {
                latest.insert(note_pair(&a, &b), state);
            }
        }

        Ok(latest)
    }

    /// Every device's category pins, oldest first. A pin overrides what earlier ones said about the same category or
    /// note, so the latest wins.
    pub fn category_pins(&self) -> Result<Vec<CategoryPin>, StoreError> {
        Ok(self
            .decisions()?
            .into_iter()
            .filter_map(|decision| match decision {
                Decision::CategoryPin { pin } => Some(pin),
                Decision::Suggestion { .. } => None,
            })
            .collect())
    }

    /// Every device's decisions, oldest first.
    fn decisions(&self) -> Result<Vec<Decision>, StoreError> {
        let mut decisions: Vec<(Order, Decision)> = Vec::new();
        for (device, path) in self.decision_files()? {
            let text = fs::read_to_string(&path).map_err(|source| StoreError::Read {
                path: path.clone(),
                source,
            })?;

            for (line, record) in text.lines().enumerate() {
                if record.trim().is_empty() {
                    continue;
                }
                let malformed = |source: DecisionError| StoreError::MalformedDecision {
                    path: path.clone(),
                    line: line + 1,
                    source,
                };
                let record: Record = serde_json::from_str(record)
                    .map_err(|error| malformed(DecisionError::Json(error)))?;
                let at = humantime::parse_rfc3339(&record.at)
                    .map_err(|error| malformed(DecisionError::Time(error)))?;

                let order = Order {
                    at,
                    device: device.clone(),
                    line,
                };
                decisions.push((order, record.decision));
            }
        }
        decisions.sort_by(|a, b| a.0.cmp(&b.0));

        Ok(decisions
            .into_iter()
            .map(|(_, decision)| decision)
            .collect())
    }

    /// Every device's decisions file with the device id it is named after.
    fn decision_files(&self) -> Result<Vec<(String, PathBuf)>, StoreError> {
        let folder = self.root.join(DECISIONS_FOLDER);
        let entries = match fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(StoreError::Read {
                    path: folder,
                    source,
                });
            }
        };

        let mut files = Vec::new();
        for entry in entries {
            let path = entry
                .map_err(|source| StoreError::Read {
                    path: folder.clone(),
                    source,
                })?
                .path();
            if path
                .extension()
                .is_some_and(|extension| extension == EXTENSION)
                && let Some(device) = file_stem(&path)
            {
                files.push((device, path));
            }
        }

        Ok(files)
    }
}

fn file_stem(path: &Path) -> Option<String> {
    Some(path.file_stem()?.to_str()?.to_owned())
}

/// Which of two decisions about the same thing wins: the later one, then (for the same time) the device id and the
/// line, so every device picks the same one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Order {
    at: SystemTime,
    device: String,
    line: usize,
}
