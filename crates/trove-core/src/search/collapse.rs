use std::{collections::HashMap, sync::Mutex};

use super::{Candidate, MAX_MATCHES, Query, SearchError, Stage, note_ids};
use crate::index::{self, Index};

/// Keeps one candidate per note, with up to [`MAX_MATCHES`] of the note's best chunks, so results are notes that point
/// at their best headings, listing every stage that found any of the note's chunks. Orders them best first.
pub struct CollapseToNotes<'a> {
    index: &'a Mutex<Index>,
}

impl<'a> CollapseToNotes<'a> {
    pub fn new(index: &'a Mutex<Index>) -> Self {
        Self { index }
    }
}

/// A candidate's chunks: those of its note, or itself for a single chunk.
fn chunks_of(candidate: &Candidate) -> Vec<(i64, f32)> {
    if candidate.chunks.is_empty() {
        vec![(candidate.chunk_id, candidate.score)]
    } else {
        candidate.chunks.clone()
    }
}

impl Stage for CollapseToNotes<'_> {
    fn run(
        &self,
        _query: &Query,
        mut candidates: Vec<Candidate>,
    ) -> Result<Vec<Candidate>, SearchError> {
        if candidates.is_empty() {
            return Ok(candidates);
        }

        let notes = note_ids(&index::lock(self.index), &candidates)?;

        candidates.sort_by(|a, b| b.score.total_cmp(&a.score));
        let mut places: HashMap<i64, usize> = HashMap::new();
        let mut collapsed: Vec<Candidate> = Vec::new();
        for mut candidate in candidates {
            let Some(&note) = notes.get(&candidate.chunk_id) else {
                continue;
            };

            match places.get(&note) {
                Some(&place) => {
                    let kept = &mut collapsed[place];
                    for chunk in chunks_of(&candidate) {
                        if kept.chunks.len() < MAX_MATCHES
                            && kept.chunks.iter().all(|&(id, _)| id != chunk.0)
                        {
                            kept.chunks.push(chunk);
                        }
                    }
                    kept.why.extend(candidate.why);
                }
                None => {
                    places.insert(note, collapsed.len());
                    candidate.chunks = chunks_of(&candidate);
                    candidate.chunks.truncate(MAX_MATCHES);
                    collapsed.push(candidate);
                }
            }
        }

        Ok(collapsed)
    }
}
