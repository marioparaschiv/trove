use std::{
    collections::{BTreeSet, HashSet},
    sync::Mutex,
};

use super::{Candidate, Query, SearchError, Signal, Stage};
use crate::index::{self, Index};

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

        let graph = index::lock(self.index).graph()?;
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

        let note_of = |chunk_id| {
            graph
                .chunk_node(chunk_id)
                .and_then(|node| graph.nodes[node].file_id())
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

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, path::PathBuf, sync::Arc, time::Duration};

    use super::*;
    use crate::{
        data_dir::DataDir,
        embedder::Embedder,
        search::{SearchRequest, search},
    };

    const NOTES: &str = "
        INSERT INTO sources (id, path) VALUES (1, '/vault');
        INSERT INTO files (id, source_id, path, mtime, hash, title, properties)
        VALUES (1, 1, 'Bread.md', 0, x'01', 'Bread', '{}'), (2, 1, 'Flour.md', 0, x'02', 'Flour', '{}');
        INSERT INTO chunks (id, file_id, ordinal, heading_path, span_start, span_end, text_hash, has_words)
        VALUES (1, 1, 0, '', 0, 9, x'01', 1), (2, 2, 0, '', 0, 5, x'02', 1);
        INSERT INTO chunks_fts (rowid, title, heading_path, text)
        VALUES (1, 'Bread', '', 'sourdough'), (2, 'Flour', '', 'flour');
        INSERT INTO edges (src, dst, kind, weight) VALUES ('note:1', 'link:Flour', 'link', 1.0);
    ";

    #[test]
    fn repeated_searches_reuse_the_loaded_graph_until_the_index_changes() {
        let temp = tempfile::tempdir().unwrap();
        let index = Mutex::new(Index::open(&DataDir::at(temp.path().to_owned()).unwrap()).unwrap());
        index::lock(&index)
            .connection()
            .execute_batch(NOTES)
            .unwrap();
        // Never loaded: the searches leave out meaning.
        let embedder = Embedder::new(PathBuf::from("no model"), Duration::from_secs(1));
        let request = SearchRequest {
            query: "sourdough".to_owned(),
            limit: None,
            skip: BTreeSet::from([Signal::Meaning]),
        };

        let paths = |index: &Mutex<Index>| -> Vec<String> {
            search(index, &embedder, &request)
                .unwrap()
                .results
                .into_iter()
                .map(|result| result.path)
                .collect()
        };
        assert_eq!(paths(&index), ["Bread.md", "Flour.md"]);
        let loaded = index::lock(&index).graph().unwrap();
        assert_eq!(paths(&index), ["Bread.md", "Flour.md"]);
        assert!(Arc::ptr_eq(&loaded, &index::lock(&index).graph().unwrap()));

        index::lock(&index)
            .connection()
            .execute(
                "INSERT INTO edges (src, dst, kind, weight) VALUES ('note:2', 'tag:baking', 'tag', 1.0)",
                [],
            )
            .unwrap();
        search(&index, &embedder, &request).unwrap();
        assert!(!Arc::ptr_eq(&loaded, &index::lock(&index).graph().unwrap()));
    }
}
