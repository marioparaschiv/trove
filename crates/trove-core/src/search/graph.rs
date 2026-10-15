use std::{
    collections::{BTreeSet, HashSet},
    sync::Mutex,
};

use super::{Candidate, Query, SearchError, Signal, Stage};
use crate::{
    graph::VaultGraph,
    index::{self, Index},
};

/// The graph's share of a blended score. Below one half, so the best direct match stays above anything the graph adds.
/// The eval sets the final value.
const GRAPH_SHARE: f32 = 0.3;
/// How much of the best-ranked section's PageRank a section of another note needs to be added. On the fixture vault
/// TODO
const MIN_RANK: f32 = 0.1;

/// Graph rerank: personalised PageRank over the graph, seeded by the candidates in proportion to their scores. Each
/// candidate's score becomes a blend of its own score and its rank, both relative to the best, and sections of notes
/// with no candidate that rank high enough are added, found by the graph.
pub struct GraphRerank<'a> {
    index: &'a Mutex<Index>,
}

impl<'a> GraphRerank<'a> {
    pub fn new(index: &'a Mutex<Index>) -> Self {
        Self { index }
    }
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

        let best_rank = (0..ranks.len())
            .filter(|&node| graph.section(node).is_some())
            .map(|node| ranks[node])
            .fold(0.0, f32::max);
        if best_rank <= 0.0 {
            return Ok(candidates);
        }

        let mut found_notes = HashSet::new();
        for candidate in &mut candidates {
            let rank = match graph.chunk_node(candidate.chunk_id) {
                Some(node) => {
                    found_notes.extend(graph.section(node).map(|section| section.file_id));
                    ranks[node]
                }
                None => 0.0,
            };
            candidate.score =
                (1.0 - GRAPH_SHARE) * candidate.score / best_score + GRAPH_SHARE * rank / best_rank;
        }

        for (node, &rank) in ranks.iter().enumerate() {
            let relative = rank / best_rank;
            eprintln!("DEBUG {node} {:?} {relative:.4}", graph.section(node));
            if relative < MIN_RANK {
                continue;
            }
            let Some(section) = graph.section(node) else {
                continue;
            };
            if found_notes.contains(&section.file_id) {
                continue;
            }

            candidates.push(Candidate {
                chunk_id: section.chunk_id,
                score: GRAPH_SHARE * relative,
                why: BTreeSet::from([Signal::Graph]),
            });
        }

        candidates.sort_by(|a, b| b.score.total_cmp(&a.score));

        Ok(candidates)
    }
}
