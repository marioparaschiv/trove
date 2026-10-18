use std::{
    collections::{BTreeSet, HashMap},
    iter,
    sync::Mutex,
};

use super::{Candidate, Coverage, Query, SearchError, Signal, Stage, Weights, note_ids, words};
use crate::index::{self, Index};

/// How many standard deviations above this query's average a note's best chunk must score to match by meaning. Set by
/// the eval on the fixture vault's 34 chunks: the weakest wanted best match scores 2.47, while queries on topics the
/// vault lacks top out at 2.27 (DigUp's 2.0 lets them through). On the 300-note eval vault every cutoff from 2.0 to
/// 2.65 ranks alike; 2.8 and up lose answers from the top 10.
pub(super) const MIN_Z: f32 = 2.35;
/// The share of the query's words a note must have to match by words alone.
const MIN_COVERAGE: f32 = 0.5;
/// Coverage counts to this power, so a note with every word of the query pulls ahead of one with most of them.
const COVERAGE_POWER: f32 = 1.5;
/// How many of a note's best chunks by meaning count ([`Signals::next_z`]).
const TOP_CHUNKS: usize = 3;

/// Scores notes as DigUp does, from the meaning and word candidates before it. A note matches when its best chunk's
/// meaning stands out ([`MIN_Z`]) or it has at least [`MIN_COVERAGE`] of the query's words, all words alike, and
/// scores by its [`Signals`] ([`Weights::score`]). Adds one candidate per matching note, best first: its best chunk by
/// meaning when that stands out, else its best word match. It lists meaning when meaning stood out and words when the
/// note has any of the words.
pub struct Scoring<'a> {
    index: &'a Mutex<Index>,
    weights: Weights,
}

impl<'a> Scoring<'a> {
    pub fn new(index: &'a Mutex<Index>, weights: Weights) -> Self {
        Self { index, weights }
    }
}

/// What a matching note is scored by.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Signals {
    /// The z-score of the note's best chunk by meaning; 0 for a note meaning didn't add (it adds only chunks above the
    /// query's average).
    pub best_z: f32,
    /// The z-scores of its next best chunks that stand out too ([`MIN_Z`]), up to [`TOP_CHUNKS`] in all, each counting
    /// half as much as the one before. Counting chunks that don't stand out loses on the eval: a long note gathers many
    /// middling ones.
    pub next_z: f32,
    pub coverage: Coverage,
}

impl Weights {
    /// A note's score: `meaning × (best z + next chunk share × next z) + words × weighted coverage ^ COVERAGE_POWER +
    /// plain words × coverage share ^ COVERAGE_POWER + title × title coverage`, with the lookup weights for meaning
    /// and words when `lookup`.
    pub fn score(&self, signals: &Signals, lookup: bool) -> f32 {
        let (meaning, words) = if lookup {
            (self.lookup_meaning, self.lookup_words)
        } else {
            (self.meaning, self.words)
        };
        let coverage = signals.coverage;

        meaning * (signals.best_z + self.next_chunk_share * signals.next_z)
            + words * coverage.weighted.powf(COVERAGE_POWER)
            + self.plain_words * coverage.share.powf(COVERAGE_POWER)
            + self.title * coverage.in_title
    }
}

/// [`Signals::next_z`] from a note's chunks' z-scores.
fn next_z(scores: &mut [f32]) -> f32 {
    scores.sort_by(|a, b| b.total_cmp(a));
    let shares = iter::successors(Some(1.0), |share| Some(share / 2.0));

    scores
        .iter()
        .skip(1)
        .take(TOP_CHUNKS - 1)
        .take_while(|&&z| z >= MIN_Z)
        .zip(shares)
        .map(|(z, share)| share * z)
        .sum()
}

/// A note that matches.
pub(super) struct Match {
    pub file_id: i64,
    /// The chunk the note matched by.
    pub chunk_id: i64,
    pub why: BTreeSet<Signal>,
    pub signals: Signals,
}

/// What the stages before found in one note.
#[derive(Default)]
struct Note {
    /// The best chunk by meaning, and its z-score.
    meaning: Option<(i64, f32)>,
    /// The z-scores of the chunks meaning added.
    meaning_scores: Vec<f32>,
    /// The best chunk by words, and the note's coverage.
    words: Option<(i64, Coverage)>,
}

/// The notes that match, from the candidates of the stages before, in no order.
pub(super) fn matches(index: &Index, candidates: Vec<Candidate>) -> rusqlite::Result<Vec<Match>> {
    let note_of = note_ids(index, &candidates)?;
    let mut notes: HashMap<i64, Note> = HashMap::new();
    for candidate in candidates {
        let Some(&file_id) = note_of.get(&candidate.chunk_id) else {
            continue;
        };

        let note = notes.entry(file_id).or_default();
        if let Some(coverage) = candidate.coverage {
            if note.words.is_none() {
                note.words = Some((candidate.chunk_id, coverage));
            }
        } else {
            if note.meaning.is_none_or(|(_, z)| candidate.score > z) {
                note.meaning = Some((candidate.chunk_id, candidate.score));
            }
            note.meaning_scores.push(candidate.score);
        }
    }

    Ok(notes
        .into_iter()
        .filter_map(|(file_id, mut note)| {
            let best_z = note.meaning.map_or(0.0, |(_, z)| z);
            let coverage = note.words.map(|(_, coverage)| coverage).unwrap_or_default();
            let stands_out = best_z >= MIN_Z;
            if !stands_out && coverage.share < MIN_COVERAGE {
                return None;
            }

            let mut why = BTreeSet::new();
            if stands_out {
                why.insert(Signal::Meaning);
            }
            if note.words.is_some() {
                why.insert(Signal::Words);
            }
            let chunk_id = if stands_out {
                note.meaning.map(|(chunk_id, _)| chunk_id)
            } else {
                note.words.map(|(chunk_id, _)| chunk_id)
            }
            .expect("a note that matches has the chunk it matched by");

            Some(Match {
                file_id,
                chunk_id,
                why,
                signals: Signals {
                    best_z,
                    next_z: next_z(&mut note.meaning_scores),
                    coverage,
                },
            })
        })
        .collect())
}

impl Stage for Scoring<'_> {
    fn run(
        &self,
        query: &Query,
        candidates: Vec<Candidate>,
    ) -> Result<Vec<Candidate>, SearchError> {
        let lookup = words::is_lookup(&words::terms(&query.text));
        let mut scored: Vec<(i64, Candidate)> = matches(&index::lock(self.index), candidates)?
            .into_iter()
            .map(|note| {
                let candidate = Candidate {
                    chunk_id: note.chunk_id,
                    score: self.weights.score(&note.signals, lookup),
                    why: note.why,
                    coverage: None,
                };
                (note.file_id, candidate)
            })
            .collect();
        scored.sort_by(|(a_note, a), (b_note, b)| {
            b.score.total_cmp(&a.score).then(a_note.cmp(b_note))
        });

        Ok(scored.into_iter().map(|(_, candidate)| candidate).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::WEIGHTS;

    #[test]
    fn a_lookup_ranks_a_note_with_every_word_above_a_stronger_meaning_match() {
        let exact = Signals {
            best_z: 0.5,
            next_z: 0.0,
            coverage: Coverage {
                share: 1.0,
                weighted: 1.0,
                in_title: 0.0,
            },
        };
        let meaning = Signals {
            best_z: 3.5,
            next_z: 0.0,
            coverage: Coverage::default(),
        };

        assert!(WEIGHTS.score(&exact, true) > WEIGHTS.score(&meaning, true));
        assert!(WEIGHTS.score(&exact, false) < WEIGHTS.score(&meaning, false));
    }
}
