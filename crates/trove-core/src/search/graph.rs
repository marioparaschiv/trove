use std::{
    collections::{BTreeSet, HashSet},
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
/// a note it links to (or one linked from the same note) gets about 0.3, one sharing a tag with it 0.04 to 0.08, and
/// one with no path to it none. The eval sets the final value.
const MIN_RANK: f32 = 0.025;

/// Graph rerank: personalised PageRank over the graph, seeded by the candidates in proportion to their scores. A note's
/// rank is that of its note node and sections together. Each candidate's score becomes a blend of its own score and
/// its note's rank, both relative to the best, so the graph reorders notes but leaves the best heading within a note to
/// the match. Notes with no candidate whose rank is high enough are added at their best-ranked section, which is the
/// heading or block a link names.
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

        let notes = graph.note_ranks(&ranks);
        let best_note_rank = notes.values().map(|note| note.rank).fold(0.0, f32::max);
        if best_note_rank <= 0.0 {
            return Ok(candidates);
        }

        let note_of = |chunk_id| match graph.chunk_node(chunk_id).map(|node| graph.nodes[node]) {
            Some(Node::Section(section)) => Some(section.file_id),
            _ => None,
        };
        let found_notes: HashSet<i64> = candidates
            .iter()
            .filter_map(|candidate| note_of(candidate.chunk_id))
            .collect();
        for candidate in &mut candidates {
            let rank = note_of(candidate.chunk_id)
                .and_then(|file_id| notes.get(&file_id))
                .map_or(0.0, |note| note.rank);
            candidate.score = (1.0 - GRAPH_SHARE) * candidate.score / best_score
                + GRAPH_SHARE * rank / best_note_rank;
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
