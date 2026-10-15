//! Categories: hierarchical Leiden communities of the notes over the combined graph, each with a start-here note and a
//! name made of its most distinctive terms.

mod names;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Mutex,
};

use network_partitions::{errors::CoreError, leiden, network::prelude::LabeledNetworkBuilder};
use rand::{SeedableRng, rngs::Xoshiro256PlusPlus};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::ToSchema;

use crate::{
    graph::{Graph, Node, VaultGraph},
    index::{self, Index},
};
use names::NoteTerms;

/// A community of at least this many notes is split into subcategories (GraphRAG's default). The eval on a real vault
/// sets the final value.
const SPLIT_SIZE: u32 = 10;
/// Seeds Leiden, so the same graph always gives the same hierarchy.
const SEED: u64 = 14;

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

#[derive(Debug, Error)]
pub enum CategoriesError {
    #[error(transparent)]
    Index(#[from] rusqlite::Error),
    #[error("community detection failed: {0:?}")]
    Leiden(CoreError),
}

struct Community {
    parent: Option<usize>,
    /// Its notes, subcategories included.
    members: BTreeSet<i64>,
}

/// Groups the indexed notes into a hierarchy of categories.
pub fn categories(index: &Mutex<Index>) -> Result<CategoriesResponse, CategoriesError> {
    let (graph, notes, terms) = {
        let index = index::lock(index);
        let conn = index.connection();
        (
            VaultGraph::load(conn)?,
            notes(conn)?,
            NoteTerms::load(conn)?,
        )
    };

    let edges = note_edges(&graph);
    if edges.is_empty() {
        return Ok(CategoriesResponse {
            categories: Vec::new(),
        });
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

    let mut direct: HashMap<usize, Vec<CategoryNote>> = HashMap::new();
    for (file_id, (_, cluster)) in deepest {
        direct
            .entry(cluster)
            .or_default()
            .push(notes[&file_id].clone());
    }

    let names = names_by_cluster(&communities, &terms);
    let categories = communities
        .iter()
        .map(|(&(_, id), community)| {
            let mut members = direct.remove(&id).unwrap_or_default();
            members.sort();
            Category {
                id,
                parent: community.parent,
                name: names[&id].clone(),
                start_here: notes[&most_central(&community.members, &edges)].clone(),
                notes: members,
            }
        })
        .collect();

    Ok(CategoriesResponse { categories })
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
/// other subcategories of their parent.
fn names_by_cluster(
    communities: &BTreeMap<(u32, usize), Community>,
    terms: &NoteTerms,
) -> HashMap<usize, String> {
    let mut siblings: BTreeMap<Option<usize>, Vec<usize>> = BTreeMap::new();
    for (&(_, id), community) in communities {
        siblings.entry(community.parent).or_default().push(id);
    }
    let members: HashMap<usize, &BTreeSet<i64>> = communities
        .iter()
        .map(|(&(_, id), community)| (id, &community.members))
        .collect();

    let mut names = HashMap::new();
    for ids in siblings.values() {
        let counts: Vec<BTreeMap<&str, u32>> =
            ids.iter().map(|id| terms.counts(members[id])).collect();
        names.extend(ids.iter().copied().zip(names::name(&counts)));
    }

    names
}
