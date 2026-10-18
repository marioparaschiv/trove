use std::{
    collections::{BTreeSet, HashMap},
    sync::Mutex,
};

use super::{Candidate, Query, SearchError, Signal, Stage, collapse::note_ids, words};
use crate::index::{self, Index};

/// How many standard deviations above this query's average a note's best chunk must score to match by meaning. Set by
/// the eval on the fixture vault's 34 chunks: the weakest wanted best match scores 2.47, while queries on topics the
/// vault lacks top out at 2.27 (DigUp's 2.0 lets them through). On the 300-note eval vault every cutoff from 2.0 to
/// 2.65 ranks alike; 2.8 and up lose answers from the top 10.
const MIN_Z: f32 = 2.35;
/// The share of the query's words a note must have to match by words alone.
const MIN_COVERAGE: f32 = 0.5;
/// Coverage counts to this power, so a note with every word of the query pulls ahead of one with most of them.
const COVERAGE_POWER: f32 = 1.5;
/// How much meaning and word coverage count. One common word can't beat a strong meaning match; all the words of an
/// exact lookup usually do. DigUp's weights.
const MEANING_WEIGHT: f32 = 1.0;
const WORD_WEIGHT: f32 = 2.0;
/// A lookup ([`words::is_lookup`]) weighs meaning half and words double, so its exact match beats stronger meaning
/// matches.
const LOOKUP_MEANING_WEIGHT: f32 = 0.5;
const LOOKUP_WORD_WEIGHT: f32 = 2.0 * WORD_WEIGHT;

/// Scores notes as DigUp does, from the meaning and word candidates before it. A note matches when its best chunk's
/// meaning stands out ([`MIN_Z`]) or it has at least [`MIN_COVERAGE`] of the query's words, and scores
/// `meaning weight × z + word weight × coverage ^ COVERAGE_POWER`, z being 0 for a note meaning didn't add (it adds
/// only chunks above the query's average). Adds one candidate per matching note, best first: its
/// best chunk by meaning when that stands out, else its best word match. It lists meaning when meaning stood out and
/// words when the note has any of the words.
pub struct Scoring<'a> {
    index: &'a Mutex<Index>,
}

impl<'a> Scoring<'a> {
    pub fn new(index: &'a Mutex<Index>) -> Self {
        Self { index }
    }
}

/// What the stages before found in one note.
#[derive(Default)]
struct Note {
    /// The best chunk by meaning, and its z-score.
    meaning: Option<(i64, f32)>,
    /// The best chunk by words, and the note's coverage.
    words: Option<(i64, f32)>,
}

impl Stage for Scoring<'_> {
    fn run(
        &self,
        query: &Query,
        candidates: Vec<Candidate>,
    ) -> Result<Vec<Candidate>, SearchError> {
        let note_of = note_ids(&index::lock(self.index), &candidates)?;
        let mut notes: HashMap<i64, Note> = HashMap::new();
        for candidate in candidates {
            let Some(&file_id) = note_of.get(&candidate.chunk_id) else {
                continue;
            };

            let note = notes.entry(file_id).or_default();
            let found = Some((candidate.chunk_id, candidate.score));
            if candidate.why.contains(&Signal::Meaning) {
                if note.meaning.is_none_or(|(_, z)| candidate.score > z) {
                    note.meaning = found;
                }
            } else if note.words.is_none() {
                note.words = found;
            }
        }

        let (meaning_weight, word_weight) = if words::is_lookup(&words::terms(&query.text)) {
            (LOOKUP_MEANING_WEIGHT, LOOKUP_WORD_WEIGHT)
        } else {
            (MEANING_WEIGHT, WORD_WEIGHT)
        };

        let mut scored: Vec<(i64, Candidate)> = notes
            .into_iter()
            .filter_map(|(file_id, note)| {
                let z = note.meaning.map_or(0.0, |(_, z)| z);
                let coverage = note.words.map_or(0.0, |(_, coverage)| coverage);
                let stands_out = z >= MIN_Z;
                if !stands_out && coverage < MIN_COVERAGE {
                    return None;
                }

                let mut why = BTreeSet::new();
                if stands_out {
                    why.insert(Signal::Meaning);
                }
                if note.words.is_some() {
                    why.insert(Signal::Words);
                }
                let (chunk_id, _) = if stands_out { note.meaning } else { note.words }
                    .expect("a note that matches has the chunk it matched by");

                Some((
                    file_id,
                    Candidate {
                        chunk_id,
                        score: meaning_weight * z + word_weight * coverage.powf(COVERAGE_POWER),
                        why,
                    },
                ))
            })
            .collect();
        scored.sort_by(|(a_note, a), (b_note, b)| {
            b.score.total_cmp(&a.score).then(a_note.cmp(b_note))
        });

        Ok(scored.into_iter().map(|(_, candidate)| candidate).collect())
    }
}
