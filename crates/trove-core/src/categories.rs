//! Categories: hierarchical Leiden communities of the notes over the combined graph, each with a start-here note and a
//! name made of its most distinctive terms. The user's category pins, kept in each source's sync store, override
//! them.

mod names;
mod pins;
mod tree;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
    sync::Mutex,
};

use network_partitions::{errors::CoreError, leiden, network::prelude::LabeledNetworkBuilder};
use rand::{SeedableRng, rngs::Xoshiro256PlusPlus};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::{IntoParams, ToSchema};

use crate::{
    graph::{ExcludedFolder, Graph, Node, VaultGraph},
    index::{self, Index, IndexError},
    sync_store::{CategoryPin, Decision, StoreError, SyncStore},
};
use names::NoteTerms;
use pins::Pins;
use tree::{Community, Place, Tree};

/// A community of at least this many notes is split into subcategories (GraphRAG's default). The eval on a real vault
/// sets the final value.
const SPLIT_SIZE: u32 = 10;
/// Seeds Leiden, so the same graph always gives the same hierarchy.
const SEED: u64 = 14;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct CategoriesRequest {
    /// A folder inside each source, `/`-separated, whose notes are left out along with every link to them. The
    /// Obsidian plugin names its category folder, so the category notes it writes don't change the categories.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[param(value_type = Option<String>)]
    pub exclude_folder: Option<ExcludedFolder>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CategoriesResponse {
    /// Every category, each parent before its subcategories. Notes with no edge to another note are in none.
    pub categories: Vec<Category>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Category {
    /// Identifies the category within one response; the same index gives the same ids.
    pub id: usize,
    /// The category this one is a subcategory of; null at the top level.
    pub parent: Option<usize>,
    /// The terms most distinctive of the category's notes against its sibling categories, joined with `, `.
    pub name: String,
    /// The most central note of the category, subcategories included.
    pub start_here: CategoryNote,
    /// The notes in this category and in none of its subcategories, by path.
    pub notes: Vec<CategoryNote>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
pub struct CategoryNote {
    /// The source folder the note is in, as registered.
    pub source: String,
    /// The note's path in its source, `/`-separated.
    pub path: String,
    pub title: String,
}

/// Manual category edits in one source, kept in its sync store so they hold on every device that syncs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CategoryPins {
    /// The source folder, as registered.
    pub source: String,
    /// In order: a pin overrides earlier ones about the same category or note.
    pub pins: Vec<CategoryPin>,
}

#[derive(Debug, Error)]
pub enum CategoriesError {
    #[error("{0} is not a source")]
    UnknownSource(String),
    #[error("a category name can't be empty")]
    EmptyName,
    #[error(transparent)]
    Index(#[from] IndexError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("community detection failed: {0:?}")]
    Leiden(CoreError),
}

impl From<rusqlite::Error> for CategoriesError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Index(error.into())
    }
}

/// A run's communities by `(level, cluster id)`, and the deepest community of each note.
type Hierarchy = (BTreeMap<(u32, usize), Community>, BTreeMap<i64, usize>);

/// Groups the indexed notes into a hierarchy of categories, following the category pins in each source's store.
pub fn categories(
    index: &Mutex<Index>,
    store_folder: &str,
    request: &CategoriesRequest,
) -> Result<CategoriesResponse, CategoriesError> {
    let (graph, notes, terms, sources) = {
        let index = index::lock(index);
        let conn = index.connection();
        (
            VaultGraph::load_without(conn, request.exclude_folder.as_ref())?,
            notes(conn)?,
            NoteTerms::load(conn)?,
            index.sources()?,
        )
    };

    let file_ids: HashMap<(&str, &str), i64> = notes
        .iter()
        .filter(|(_, note)| {
            request
                .exclude_folder
                .as_ref()
                .is_none_or(|folder| !folder.contains(&note.path))
        })
        .map(|(&file_id, note)| ((note.source.as_str(), note.path.as_str()), file_id))
        .collect();
    let mut source_pins = Vec::new();
    for source in sources {
        let store = SyncStore::new(Path::new(&source.path), store_folder);
        for pin in store.category_pins()? {
            source_pins.push((source.path.clone(), pin));
        }
    }
    let pins = Pins::resolve(source_pins, |source, path| {
        file_ids.get(&(source, path)).copied()
    });

    let edges = note_edges(&graph);
    let (communities, deepest) = communities(&edges)?;
    let Tree {
        places,
        direct,
        members,
    } = tree::tree(&communities, &deepest, &pins);

    let ids: HashMap<&Place, usize> = places
        .iter()
        .enumerate()
        .map(|(id, (place, _))| (place, id))
        .collect();
    let names = names(&places, &members, &terms);
    let categories = places
        .iter()
        .map(|(place, parent)| {
            let mut notes_here: Vec<CategoryNote> = direct
                .get(place)
                .into_iter()
                .flatten()
                .map(|file_id| notes[file_id].clone())
                .collect();
            notes_here.sort();
            Category {
                id: ids[place],
                parent: parent.as_ref().map(|parent| ids[parent]),
                name: names[place].clone(),
                start_here: notes[&most_central(&members[place], &edges)].clone(),
                notes: notes_here,
            }
        })
        .collect();

    Ok(CategoriesResponse { categories })
}

/// Records category pins in the source's sync store, as made on this device now.
pub fn pin(
    index: &Mutex<Index>,
    store_folder: &str,
    request: CategoryPins,
) -> Result<(), CategoriesError> {
    let names_empty = request.pins.iter().any(|pin| match pin {
        CategoryPin::Category { name, replaces, .. } => {
            name.trim().is_empty() || replaces.iter().any(|name| name.trim().is_empty())
        }
        CategoryPin::Parent { category, parent } => {
            category.trim().is_empty() || parent.as_ref().is_some_and(|name| name.trim().is_empty())
        }
        CategoryPin::Note { category, .. } => category.trim().is_empty(),
    });
    if names_empty {
        return Err(CategoriesError::EmptyName);
    }

    // Held while writing, so two requests at once can't both rewrite this device's file from the same contents.
    let index = index::lock(index);
    let registered: Option<i64> = index
        .connection()
        .query_row(
            "SELECT id FROM sources WHERE path = ?1",
            [&request.source],
            |row| row.get(0),
        )
        .optional()?;
    if registered.is_none() {
        return Err(CategoriesError::UnknownSource(request.source));
    }

    SyncStore::new(Path::new(&request.source), store_folder).record_decisions(
        &index.device_id()?,
        request
            .pins
            .into_iter()
            .map(|pin| Decision::CategoryPin { pin }),
    )?;

    Ok(())
}

fn communities(edges: &BTreeMap<(i64, i64), f64>) -> Result<Hierarchy, CategoriesError> {
    if edges.is_empty() {
        return Ok(Hierarchy::default());
    }

    let network = LabeledNetworkBuilder::new()
        .build(edges.iter().map(|(&(a, b), &weight)| (a, b, weight)), true);
    let clusters = leiden::hierarchical_leiden(
        network.compact(),
        None,
        None,
        None,
        None,
        &mut Xoshiro256PlusPlus::seed_from_u64(SEED),
        true,
        SPLIT_SIZE,
        None,
    )
    .map_err(CategoriesError::Leiden)?;

    // Each note has one entry per level down to the deepest community it is in.
    let mut communities: BTreeMap<(u32, usize), Community> = BTreeMap::new();
    let mut deepest: BTreeMap<i64, (u32, usize)> = BTreeMap::new();
    for cluster in &clusters {
        let file_id = *network.label_for(cluster.node);
        communities
            .entry((cluster.level, cluster.cluster))
            .or_insert_with(|| Community {
                parent: cluster.parent_cluster,
                members: BTreeSet::new(),
            })
            .members
            .insert(file_id);

        let place = deepest
            .entry(file_id)
            .or_insert((cluster.level, cluster.cluster));
        *place = (*place).max((cluster.level, cluster.cluster));
    }
    let deepest = deepest
        .into_iter()
        .map(|(file_id, (_, cluster))| (file_id, cluster))
        .collect();

    Ok((communities, deepest))
}

fn notes(conn: &Connection) -> rusqlite::Result<HashMap<i64, CategoryNote>> {
    conn.prepare(
        "SELECT f.id, s.path, f.path, f.title FROM files f JOIN sources s ON s.id = f.source_id",
    )?
    .query_map([], |row| {
        Ok((
            row.get(0)?,
            CategoryNote {
                source: row.get(1)?,
                path: row.get(2)?,
                title: row.get(3)?,
            },
        ))
    })?
    .collect()
}

/// The combined graph between notes. An edge between two notes' own nodes (the note or its sections) adds its weight.
/// A tag on k notes joins each two of them with 1/(k-1) of its weight, so each note's tag weight is what it is in the
/// full graph.
fn note_edges(graph: &VaultGraph) -> BTreeMap<(i64, i64), f64> {
    let note_of = |node: usize| match graph.nodes[node] {
        Node::Note(file_id) => Some(file_id),
        Node::Section(section) => Some(section.file_id),
        Node::Tag => None,
    };

    let mut weights: BTreeMap<(i64, i64), f64> = BTreeMap::new();
    let mut add = |a: i64, b: i64, weight: f64| {
        if a != b {
            *weights.entry((a.min(b), a.max(b))).or_default() += weight;
        }
    };
    for node in 0..graph.graph.len() {
        let edges = graph.graph.edges(node);
        if let Some(note) = note_of(node) {
            // Each edge is listed at both ends; take it from the lower one.
            for &(other, weight) in edges.iter().filter(|(other, _)| *other > node) {
                if let Some(other_note) = note_of(other) {
                    add(note, other_note, weight.into());
                }
            }
            continue;
        }

        let tagged: Vec<(i64, f32)> = edges
            .iter()
            .filter_map(|&(other, weight)| Some((note_of(other)?, weight)))
            .collect();
        if tagged.len() < 2 {
            continue;
        }

        let share = 1.0 / (tagged.len() - 1) as f64;
        for (place, &(a, a_weight)) in tagged.iter().enumerate() {
            for &(b, b_weight) in &tagged[place + 1..] {
                add(a, b, f64::from(a_weight.min(b_weight)) * share);
            }
        }
    }

    weights
}

/// The member with the highest PageRank over the edges among the members; the lowest file id on a tie.
fn most_central(members: &BTreeSet<i64>, edges: &BTreeMap<(i64, i64), f64>) -> i64 {
    let mut graph = Graph::default();
    let nodes: HashMap<i64, usize> = members
        .iter()
        .map(|&file_id| (file_id, graph.add_node()))
        .collect();
    for (&(a, b), &weight) in edges {
        if let (Some(&a), Some(&b)) = (nodes.get(&a), nodes.get(&b)) {
            graph.add_edge(a, b, weight as f32);
        }
    }

    let seeds: Vec<(usize, f32)> = (0..graph.len()).map(|node| (node, 1.0)).collect();
    let ranks = graph.personalized_pagerank(&seeds);

    members
        .iter()
        .max_by(|a, b| ranks[nodes[a]].total_cmp(&ranks[nodes[b]]).then(b.cmp(a)))
        .copied()
        .expect("every community has a member")
}

/// Names each community against its siblings: the top-level ones against each other, and subcategories against the
/// other subcategories of their parent. A pinned category keeps its pinned name.
fn names<'a>(
    places: &'a [(Place, Option<Place>)],
    members: &BTreeMap<Place, BTreeSet<i64>>,
    terms: &NoteTerms,
) -> HashMap<&'a Place, String> {
    let mut siblings: BTreeMap<Option<&Place>, Vec<&Place>> = BTreeMap::new();
    for (place, parent) in places {
        siblings.entry(parent.as_ref()).or_default().push(place);
    }

    let mut names = HashMap::new();
    for group in siblings.values() {
        let counts: Vec<BTreeMap<&str, u32>> = group
            .iter()
            .map(|place| terms.counts(&members[*place]))
            .collect();
        for (&place, generated) in group.iter().zip(names::name(&counts)) {
            let name = match place {
                Place::Pinned(name) => name.clone(),
                Place::Community(_) => generated,
            };
            names.insert(place, name);
        }
    }

    names
}
