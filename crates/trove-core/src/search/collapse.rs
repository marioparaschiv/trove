use std::{collections::HashMap, sync::Mutex};

use super::{Candidate, Query, SearchError, Stage, note_ids};
use crate::index::{self, Index};

/// Keeps each note's best chunk, so results are notes that point at their best heading, listing every stage that found
/// any of the note's chunks. Orders them best first.
pub struct CollapseToNotes<'a> {
    index: &'a Mutex<Index>,
}

impl<'a> CollapseToNotes<'a> {
    pub fn new(index: &'a Mutex<Index>) -> Self {
        Self { index }
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
        for candidate in candidates {
            let Some(&note) = notes.get(&candidate.chunk_id) else {
                continue;
            };

            match places.get(&note) {
                Some(&place) => collapsed[place].why.extend(candidate.why),
                None => {
                    places.insert(note, collapsed.len());
                    collapsed.push(candidate);
                }
            }
        }

        Ok(collapsed)
    }
}
