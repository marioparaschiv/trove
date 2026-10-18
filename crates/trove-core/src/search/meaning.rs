use std::{collections::BTreeSet, sync::Mutex};

use super::{Candidate, Query, SearchError, Signal, Stage};
use crate::{
    embedder::Embedder,
    index::{self, Index},
};

/// Search by meaning: the query's cosine similarity to every chunk, put on this query's own scale (z-scores), so that
/// scoring can tell the chunks that clearly stand out and a query that matches nothing finds nothing. Adds every chunk
/// above the query's average, best first, with its z-score as the score.
pub struct Meaning<'a> {
    index: &'a Mutex<Index>,
    embedder: &'a Embedder,
}

impl<'a> Meaning<'a> {
    pub fn new(index: &'a Mutex<Index>, embedder: &'a Embedder) -> Self {
        Self { index, embedder }
    }
}

impl Stage for Meaning<'_> {
    fn run(
        &self,
        query: &Query,
        mut candidates: Vec<Candidate>,
    ) -> Result<Vec<Candidate>, SearchError> {
        if !index::lock(self.index).has_vectors()? {
            return Ok(candidates);
        }

        let query_vector = self.embedder.embed_query(&query.text)?;
        let scores = index::lock(self.index).chunk_scores(&query_vector)?;

        let count = scores.len() as f32;
        let mean = scores.iter().map(|(_, score)| score).sum::<f32>() / count;
        let deviation = (scores
            .iter()
            .map(|(_, score)| (score - mean).powi(2))
            .sum::<f32>()
            / count)
            .sqrt();
        // One chunk, or all alike: nothing stands out.
        if deviation <= f32::EPSILON {
            return Ok(candidates);
        }

        let mut matches: Vec<Candidate> = scores
            .into_iter()
            .map(|(chunk_id, score)| Candidate {
                chunk_id,
                score: (score - mean) / deviation,
                why: BTreeSet::from([Signal::Meaning]),
            })
            .filter(|candidate| candidate.score > 0.0)
            .collect();
        matches.sort_by(|a, b| b.score.total_cmp(&a.score));
        candidates.extend(matches);

        Ok(candidates)
    }
}
