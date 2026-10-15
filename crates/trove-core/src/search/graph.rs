use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    sync::Mutex,
};

use super::{Candidate, Query, SearchError, Signal, Stage};
use crate::{
    graph::{Node, VaultGraph},
    index::{self, Index},
};

/// The graph's share of a blended score. Below one half, so the best direct match stays above anything the graph adds.
/// The eval sets the final value.
const GRAPH_SHARE: f32 = 0.3;
/// How much of the best note's PageRank another note needs to be added. On the fixture vault, with one note matching,
/// a note it links to gets about 0.2, one sharing a tag with it 0.05, one three steps away 0.013 and one with no path
/// to it none. The eval sets the final value.
const MIN_RANK: f32 = 0.025;

/// Graph rerank: personalised PageRank over the graph, seeded by the candidates in proportion to their scores. Each
/// candidate's score becomes a blend of its own score and its section's rank, both relative to the best. Notes with no
/// candidate whose rank (their note and sections together) is high enough are added at their best-ranked section.
pub struct GraphRerank<'a> {
    index: &'a Mutex<Index>,
}

impl<'a> GraphRerank<'a> {
    pub fn new(index: &'a Mutex<Index>) -> Self {
        Self { index }
    }
}

#[derive(Default)]
struct NoteRank {
    rank: f32,
    best_section: Option<(i64, f32)>,
}

impl Stage for GraphRerank<'_> {
    fn run(
        &self,
        _query: &Query,
        mut candidates: Vec<Candidate>,
    ) -> Result<Vec<Candidate>, SearchError> {
        let best_score = candidates
            .iter()
            .map(|candidate| candidate.score)
            .fold(0.0, f32::max);
        if best_score <= 0.0 {
            return Ok(candidates);
        }

        let graph = VaultGraph::load(index::lock(self.index).connection())?;
        let seeds: Vec<(usize, f32)> = candidates
            .iter()
            .filter_map(|candidate| {
                let node = graph.chunk_node(candidate.chunk_id)?;
                Some((node, candidate.score.max(0.0)))
            })
            .collect();
        let ranks = graph.graph.personalized_pagerank(&seeds);

        let mut notes: BTreeMap<i64, NoteRank> = BTreeMap::new();
        let mut best_section_rank: f32 = 0.0;
        for (node, &rank) in graph.nodes.iter().zip(&ranks) {
            match *node {
                Node::Note(file_id) => notes.entry(file_id).or_default().rank += rank,
                Node::Section(section) => {
                    best_section_rank = best_section_rank.max(rank);
                    let note = notes.entry(section.file_id).or_default();
                    note.rank += rank;
                    if note.best_section.is_none_or(|(_, best)| rank > best) {
                        note.best_section = Some((section.chunk_id, rank));
                    }
                }
                Node::Tag => {}
            }
        }
        let best_note_rank = notes.values().map(|note| note.rank).fold(0.0, f32::max);
        if best_section_rank <= 0.0 || best_note_rank <= 0.0 {
            return Ok(candidates);
        }

        let mut found_notes = HashSet::new();
        for candidate in &mut candidates {
            let rank = match graph.chunk_node(candidate.chunk_id) {
                Some(node) => {
                    if let Node::Section(section) = graph.nodes[node] {
                        found_notes.insert(section.file_id);
                    }
                    ranks[node]
                }
                None => 0.0,
            };
            candidate.score = (1.0 - GRAPH_SHARE) * candidate.score / best_score
                + GRAPH_SHARE * rank / best_section_rank;
        }

        for (file_id, note) in notes {
            let relative = note.rank / best_note_rank;
            if relative < MIN_RANK || found_notes.contains(&file_id) {
                continue;
            }
            let Some((chunk_id, _)) = note.best_section else {
                continue;
            };

            candidates.push(Candidate {
                chunk_id,
                score: GRAPH_SHARE * relative,
                why: BTreeSet::from([Signal::Graph]),
            });
        }

        candidates.sort_by(|a, b| b.score.total_cmp(&a.score));

        Ok(candidates)
    }
}
