use std::{collections::HashMap, sync::Mutex};

use super::{Candidate, Query, SearchError, Stage};
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

        let chunk_ids: Vec<i64> = candidates
            .iter()
            .map(|candidate| candidate.chunk_id)
            .collect();
        let notes: HashMap<i64, i64> = {
            let index = index::lock(self.index);
            let mut statement = index.connection().prepare(
                "SELECT id, file_id FROM chunks WHERE id IN (SELECT value FROM json_each(?1))",
            )?;
            statement
                .query_map([serde_json::Value::from(chunk_ids).to_string()], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })?
                .collect::<Result<_, _>>()?
        };

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
