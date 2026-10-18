//! Link suggestions (pairs of notes close in meaning with no link either way, minus the ones the user decided about)
//! and orphans (notes with no link in or out). Decisions live in the source's sync store, so they hold on every device
//! and after the local index is rebuilt.

use std::{
    collections::{BTreeSet, HashMap},
    path::Path,
    sync::Mutex,
};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::{IntoParams, ToSchema};

use crate::{
    categories::CategoryNote,
    graph::{EdgeKind, ExcludedFolder, NodeKey, VaultGraph},
    index::{self, Index, IndexError},
    search::DEFAULT_LIMIT,
    sync_store::{Decision, StoreError, SuggestionState, SyncStore, note_pair},
};

/// Two notes are suggested when two of their sections are at least this similar (cosine). On the fixture vaults,
/// sections on unrelated topics reach 0.73 and notes on one topic 0.77 to 0.85 (two ways of brewing coffee 0.81). The
/// eval on a real vault sets the final value.
const SUGGESTION_FLOOR: f32 = 0.78;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct SuggestionsRequest {
    /// The source folder, as registered.
    pub source: String,
    /// The most suggestions to return. Defaults to 10.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(minimum = 1)]
    pub limit: Option<u32>,
    /// A folder inside the source, `/`-separated, whose notes are left out along with every link to them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>)]
    pub exclude_folder: Option<ExcludedFolder>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SuggestionsResponse {
    /// Most similar first.
    pub suggestions: Vec<Suggestion>,
}

/// Two notes that could link to each other. Accepting means linking `note` to `target`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Suggestion {
    pub note: CategoryNote,
    pub target: CategoryNote,
    /// Cosine similarity of the two notes' closest sections.
    pub similarity: f32,
}

/// A decision about a suggested link. The pair is the same whichever way round its notes are given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SuggestionDecision {
    /// The source folder, as registered.
    pub source: String,
    /// One note's path in the source, `/`-separated.
    pub note: String,
    /// The other note's path in the source.
    pub target: String,
    pub state: SuggestionState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct OrphansRequest {
    /// The source folder, as registered.
    pub source: String,
    /// A folder inside the source, `/`-separated, whose notes are left out along with every link to them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>)]
    pub exclude_folder: Option<ExcludedFolder>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct OrphansResponse {
    /// The notes no other note links to and that link to no other note, by path.
    pub notes: Vec<CategoryNote>,
}

#[derive(Debug, Error)]
pub enum SuggestionsError {
    #[error("{0} is not a source")]
    UnknownSource(String),
    #[error("{path} in {source_folder} is not indexed")]
    NotIndexed { source_folder: String, path: String },
    #[error("a note can't be linked to itself")]
    SameNote,
    #[error("the limit must be at least 1")]
    ZeroLimit,
    #[error(transparent)]
    Index(#[from] IndexError),
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl From<rusqlite::Error> for SuggestionsError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Index(error.into())
    }
}

/// The pairs of notes in the source that are close in meaning, have no link either way and were not decided about,
/// most similar first.
pub fn suggestions(
    index: &Mutex<Index>,
    store_folder: &str,
    request: &SuggestionsRequest,
) -> Result<SuggestionsResponse, SuggestionsError> {
    let limit = request.limit.unwrap_or(DEFAULT_LIMIT);
    if limit == 0 {
        return Err(SuggestionsError::ZeroLimit);
    }

    let (notes, links, similar) = {
        let index = index::lock(index);
        let graph = VaultGraph::load_without(index.connection(), request.exclude_folder.as_ref())?;
        let notes = source_notes(&index, &request.source, &graph)?;

        (notes, graph.links, similar_notes(index.connection())?)
    };
    let states = SyncStore::new(Path::new(&request.source), store_folder).suggestion_states()?;

    let mut suggestions: Vec<Suggestion> = similar
        .into_iter()
        .filter(|((a, b), _)| !links.contains(&(*a, *b)) && !links.contains(&(*b, *a)))
        .filter_map(|((a, b), similarity)| {
            let [note, target] = sorted_notes(notes.get(&a)?, notes.get(&b)?);
            if states.contains_key(&note_pair(&note.path, &target.path)) {
                return None;
            }

            Some(Suggestion {
                note: note.clone(),
                target: target.clone(),
                similarity,
            })
        })
        .collect();
    suggestions.sort_by(|a, b| {
        b.similarity
            .total_cmp(&a.similarity)
            .then_with(|| (&a.note, &a.target).cmp(&(&b.note, &b.target)))
    });
    suggestions.truncate(limit as usize);

    Ok(SuggestionsResponse { suggestions })
}

/// Records a decision about a suggested link in the source's sync store, as made on this device now.
pub fn decide(
    index: &Mutex<Index>,
    store_folder: &str,
    request: &SuggestionDecision,
) -> Result<(), SuggestionsError> {
    if request.note == request.target {
        return Err(SuggestionsError::SameNote);
    }

    // Held while writing, so two decisions at once can't both rewrite this device's file from the same contents.
    let index = index::lock(index);
    let source_id = source_id(&index, &request.source)?;
    for path in [&request.note, &request.target] {
        let indexed: Option<i64> = index
            .connection()
            .query_row(
                "SELECT id FROM files WHERE source_id = ?1 AND path = ?2",
                (source_id, path),
                |row| row.get(0),
            )
            .optional()?;
        if indexed.is_none() {
            return Err(SuggestionsError::NotIndexed {
                source_folder: request.source.clone(),
                path: path.clone(),
            });
        }
    }

    SyncStore::new(Path::new(&request.source), store_folder).record_decision(
        &index.device_id()?,
        Decision::suggestion(&request.note, &request.target, request.state),
    )?;

    Ok(())
}

/// The notes in the source with no link in or out, by path.
pub fn orphans(
    index: &Mutex<Index>,
    request: &OrphansRequest,
) -> Result<OrphansResponse, SuggestionsError> {
    let (notes, links) = {
        let index = index::lock(index);
        let graph = VaultGraph::load_without(index.connection(), request.exclude_folder.as_ref())?;
        let notes = source_notes(&index, &request.source, &graph)?;

        (notes, graph.links)
    };

    let linked: BTreeSet<i64> = links.into_iter().flat_map(|(a, b)| [a, b]).collect();
    let mut orphans: Vec<CategoryNote> = notes
        .into_iter()
        .filter(|(file_id, _)| !linked.contains(file_id))
        .map(|(_, note)| note)
        .collect();
    orphans.sort();

    Ok(OrphansResponse { notes: orphans })
}

fn source_id(index: &Index, source: &str) -> Result<i64, SuggestionsError> {
    index
        .source_id(source)?
        .ok_or_else(|| SuggestionsError::UnknownSource(source.to_owned()))
}

/// The notes of the source that are in `graph`, which leaves out the excluded folder, by file id.
fn source_notes(
    index: &Index,
    source: &str,
    graph: &VaultGraph,
) -> Result<HashMap<i64, CategoryNote>, SuggestionsError> {
    let source_id = source_id(index, source)?;
    let notes = index
        .connection()
        .prepare("SELECT id, path, title FROM files WHERE source_id = ?1")?
        .query_map([source_id], |row| {
            Ok((
                row.get(0)?,
                CategoryNote {
                    source: source.to_owned(),
                    path: row.get(1)?,
                    title: row.get(2)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<(i64, CategoryNote)>>>()?
        .into_iter()
        .filter(|&(file_id, _)| graph.note_node(file_id).is_some())
        .collect();

    Ok(notes)
}

/// Pairs of notes, by file id with the lower first, with two sections at least [`SUGGESTION_FLOOR`] similar, and the
/// similarity of their closest sections.
fn similar_notes(conn: &Connection) -> rusqlite::Result<HashMap<(i64, i64), f32>> {
    let rows: Vec<(NodeKey, NodeKey, f32)> = conn
        .prepare("SELECT src, dst, weight FROM edges WHERE kind = ?1 AND weight >= ?2")?
        .query_map((EdgeKind::Semantic, SUGGESTION_FLOOR), |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<rusqlite::Result<_>>()?;

    let mut pairs: HashMap<(i64, i64), f32> = HashMap::new();
    for (src, dst, similarity) in rows {
        let (NodeKey::Section { file_id: a, .. }, NodeKey::Section { file_id: b, .. }) = (src, dst)
        else {
            continue;
        };
        if a == b {
            continue;
        }

        let best = pairs.entry((a.min(b), a.max(b))).or_insert(similarity);
        *best = best.max(similarity);
    }

    Ok(pairs)
}

fn sorted_notes<'a>(a: &'a CategoryNote, b: &'a CategoryNote) -> [&'a CategoryNote; 2] {
    if a.path <= b.path { [a, b] } else { [b, a] }
}
