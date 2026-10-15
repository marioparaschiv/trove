use std::collections::HashMap;

use super::{Candidate, Query, SearchError, Signal, Stage};

/// Reciprocal rank fusion's constant: how little a better rank counts over a worse one. 60 is the value from the
/// original paper (Cormack, Clarke and Buettcher, 2009).
const K: f32 = 60.0;

/// Merges what the finding stages added into one list, best first. Each stage adds its matches best first, so a
/// chunk's rank in a stage is its place among that stage's candidates. A chunk scores the sum of `1 / (K + rank)` over
/// the stages that found it, and lists all of them.
pub struct Fusion;

impl Stage for Fusion {
    fn run(
        &self,
        _query: &Query,
        candidates: Vec<Candidate>,
    ) -> Result<Vec<Candidate>, SearchError> {
        let mut ranks: HashMap<Signal, usize> = HashMap::new();
        let mut places: HashMap<i64, usize> = HashMap::new();
        let mut fused: Vec<Candidate> = Vec::new();

        for candidate in candidates {
            let place = *places.entry(candidate.chunk_id).or_insert_with(|| {
                fused.push(Candidate {
                    chunk_id: candidate.chunk_id,
                    score: 0.0,
                    why: Default::default(),
                });
                fused.len() - 1
            });

            for signal in candidate.why {
                let rank = ranks.entry(signal).or_default();
                *rank += 1;
                fused[place].score += 1.0 / (K + *rank as f32);
                fused[place].why.insert(signal);
            }
        }

        fused.sort_by(|a, b| b.score.total_cmp(&a.score));

        Ok(fused)
    }
}
