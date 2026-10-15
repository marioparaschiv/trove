use std::{collections::BTreeSet, sync::Mutex};

use super::{Candidate, Query, SearchError, Signal, Stage};
use crate::{
    embedder::{Embedder, Input},
    index::{self, Index},
};

/// How many standard deviations above this query's average a chunk must score to count as a match. DigUp uses 2.0, but
/// on the fixture vault's 26 chunks unrelated queries reach 2.4 while paraphrases score 2.6 and up. The eval sets the
/// final value.
const MIN_Z: f32 = 2.5;

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

        let query_vector = self
            .embedder
            .embed(&[Input::Query(&query.text)])?
            .pop()
            .expect("one vector per input");
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
