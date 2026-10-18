use std::{collections::BTreeSet, sync::Mutex};

use super::{Candidate, Query, SearchError, Signal, Stage, scoring::MIN_Z, words};
use crate::{
    embedder::Embedder,
    index::{self, Index},
};

/// Queries of up to this many content words are expanded with what they find ([`feedback_rows`]): a short query
/// says little about what is wanted, and the chunks it finds best say more. On the eval, up to 3 words gains less and
/// every query loses a question.
const FEEDBACK_MAX_WORDS: usize = 5;
/// How many of the best chunks the query is expanded with. On the eval, the second and third best pull the query off
/// its topic as often as not.
const FEEDBACK_CHUNKS: usize = 1;
/// How much the mean of those chunks counts in the expanded query, the query itself the rest. Tuned on the eval (0.1
/// and 0.3 rank a little worse).
const FEEDBACK_WEIGHT: f32 = 0.2;

/// Search by meaning: the query's cosine similarity to every chunk, put on this query's own scale (z-scores), so that
/// scoring can tell the chunks that clearly stand out and a query that matches nothing finds nothing. Adds every chunk
/// above the query's average, best first, with its z-score as the score.
///
/// A short query is searched twice (pseudo-relevance feedback, Rocchio's): the second time as the query blended with
/// the mean of the chunks the first search found best, taken from the scan file, so without another model call.
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
        let mut index = index::lock(self.index);
        let mut scores = z_scores(index.chunk_scores(&query_vector)?);
        let rows = feedback_rows(&words::terms(&query.text), &scores);
        if !rows.is_empty() {
            let expanded = expanded(&query_vector, &index.chunk_vectors(&rows)?);
            scores = z_scores(index.chunk_scores(&expanded)?);
        }
        drop(index);

        let mut matches: Vec<Candidate> = scores
            .into_iter()
            .filter(|&(_, z)| z > 0.0)
            .map(|(chunk_id, z)| Candidate {
                chunk_id,
                score: z,
                why: BTreeSet::from([Signal::Meaning]),
                coverage: None,
            })
            .collect();
        matches.sort_by(|a, b| b.score.total_cmp(&a.score));
        candidates.extend(matches);

        Ok(candidates)
    }
}

/// Each chunk's similarity as a z-score, in the same order. Empty for one chunk or all alike: nothing stands out.
fn z_scores(scores: Vec<(i64, f32)>) -> Vec<(i64, f32)> {
    let count = scores.len() as f32;
    let mean = scores.iter().map(|(_, score)| score).sum::<f32>() / count;
    let deviation = (scores
        .iter()
        .map(|(_, score)| (score - mean).powi(2))
        .sum::<f32>()
        / count)
        .sqrt();
    if deviation <= f32::EPSILON {
        return Vec::new();
    }

    scores
        .into_iter()
        .map(|(chunk_id, score)| (chunk_id, (score - mean) / deviation))
        .collect()
}

/// The positions of the chunks to expand a query of content words `terms` with: its [`FEEDBACK_CHUNKS`] best that
/// stand out ([`MIN_Z`]). Empty for a longer query, for a query with a code or a number (its chunk is found by the
/// exact word, and its neighbours aren't about it), or when nothing stands out, so a query that matches nothing still
/// finds nothing.
fn feedback_rows(terms: &[String], scores: &[(i64, f32)]) -> Vec<usize> {
    if !(1..=FEEDBACK_MAX_WORDS).contains(&terms.len())
        || terms.iter().any(|term| term.chars().any(char::is_numeric))
    {
        return Vec::new();
    }

    let mut rows: Vec<usize> = (0..scores.len())
        .filter(|&row| scores[row].1 >= MIN_Z)
        .collect();
    rows.sort_by(|&a, &b| scores[b].1.total_cmp(&scores[a].1));
    rows.truncate(FEEDBACK_CHUNKS);

    rows
}

/// The query moved towards the mean of `vectors` by [`FEEDBACK_WEIGHT`], back to unit length.
fn expanded(query: &[f32], vectors: &[Vec<f32>]) -> Vec<f32> {
    let share = FEEDBACK_WEIGHT / vectors.len() as f32;
    let mut expanded: Vec<f32> = query
        .iter()
        .map(|value| (1.0 - FEEDBACK_WEIGHT) * value)
        .collect();
    for vector in vectors {
        for (value, added) in expanded.iter_mut().zip(vector) {
            *value += share * added;
        }
    }

    let length = expanded
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    expanded.iter().map(|value| value / length).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_query_is_expanded_with_its_best_chunks_that_stand_out() {
        let scores = [(10, 0.4), (11, 3.1), (12, 2.6), (13, -0.2), (14, 2.0)];

        assert_eq!(feedback_rows(&words::terms("knee physio"), &scores), [1]);
        assert_eq!(
            feedback_rows(
                &words::terms("which knee exercises did the physio give me"),
                &scores
            ),
            [1]
        );
    }

    #[test]
    fn a_long_query_a_code_or_nothing_standing_out_is_not_expanded() {
        let scores = [(10, 0.4), (11, 3.1), (12, 2.6)];

        assert!(
            feedback_rows(
                &words::terms("which knee exercises did the physio give me after the fall"),
                &scores
            )
            .is_empty()
        );
        assert!(feedback_rows(&words::terms("PAY-3918 breaker"), &scores).is_empty());
        assert!(feedback_rows(&words::terms("knee physio"), &[(10, 0.4), (11, 2.2)]).is_empty());
    }

    #[test]
    fn the_expanded_query_moves_towards_the_chunks_and_stays_unit_length() {
        let expanded = expanded(&[1.0, 0.0], &[vec![0.0, 1.0]]);

        assert!(expanded[0] > expanded[1] && expanded[1] > 0.0);
        assert!((expanded.iter().map(|value| value * value).sum::<f32>() - 1.0).abs() < 1e-6);
    }
}
