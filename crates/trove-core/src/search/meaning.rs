use std::{collections::BTreeSet, sync::Mutex};

use super::{Candidate, Query, SearchError, Signal, Stage};
use crate::{
    embedder::Embedder,
    index::{self, Index},
};

/// How many standard deviations above this query's average a chunk must score to count as a match. Set by the eval on
/// the fixture vault's 34 chunks: the weakest wanted best match scores 2.47, while queries on topics the vault lacks
/// top out at 2.27 (DigUp's 2.0 lets them through). Worth measuring again on a real vault, where z-scores spread wider.
const MIN_Z: f32 = 2.35;

/// Search by meaning: the query's cosine similarity to every chunk, put on this query's own scale (z-scores), so only
/// chunks that clearly stand out match and a query that matches nothing finds nothing. Adds its matches, best first,
/// with their z-score as the score.
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
            .filter(|candidate| candidate.score >= MIN_Z)
            .collect();
        matches.sort_by(|a, b| b.score.total_cmp(&a.score));
        candidates.extend(matches);

        Ok(candidates)
    }
}
