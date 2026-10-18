use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use rusqlite::Connection;

/// How many terms make a name.
const NAME_TERMS: usize = 3;
/// Shorter words are left out of names.
const MIN_TERM_CHARS: usize = 3;

/// How often each term appears in each note's title and text.
pub(super) struct NoteTerms {
    by_note: HashMap<i64, HashMap<String, u32>>,
}

impl NoteTerms {
    pub(super) fn load(conn: &Connection) -> rusqlite::Result<Self> {
        let stop_words: HashSet<&str> = stop_words::get(stop_words::Language::English)
            .iter()
            .copied()
            .collect();
        let mut by_note: HashMap<i64, HashMap<String, u32>> = HashMap::new();
        let mut add = |file_id: i64, text: &str| {
            let counts = by_note.entry(file_id).or_default();
            for term in terms(text).filter(|term| !stop_words.contains(term.as_str())) {
                *counts.entry(term).or_default() += 1;
            }
        };

        let mut titles = conn.prepare("SELECT id, title FROM files")?;
        let mut rows = titles.query([])?;
        while let Some(row) = rows.next()? {
            add(row.get(0)?, row.get_ref(1)?.as_str()?);
        }
        let mut texts = conn.prepare(
            "SELECT c.file_id, coalesce(t.original, t.text) FROM chunks c JOIN chunks_fts t ON t.rowid = c.id",
        )?;
        let mut rows = texts.query([])?;
        while let Some(row) = rows.next()? {
            add(row.get(0)?, row.get_ref(1)?.as_str()?);
        }

        Ok(Self { by_note })
    }

    /// The term counts of some notes together.
    pub(super) fn counts(&self, notes: &BTreeSet<i64>) -> BTreeMap<&str, u32> {
        let mut counts = BTreeMap::new();
        for counted in notes.iter().filter_map(|file_id| self.by_note.get(file_id)) {
            for (term, &count) in counted {
                *counts.entry(term.as_str()).or_default() += count;
            }
        }

        counts
    }
}

/// Lowercased words of at least [`MIN_TERM_CHARS`] letters or digits (apostrophes kept inside), not all digits.
fn terms(text: &str) -> impl Iterator<Item = String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '\'')
        .map(|word| word.trim_matches('\'').to_lowercase())
        .filter(|word| {
            word.chars().count() >= MIN_TERM_CHARS && !word.chars().all(char::is_numeric)
        })
}

/// Names each class of a group by c-TF-IDF: its [`NAME_TERMS`] terms with the highest frequency in the class times
/// ln(1 + A / the term's count across the group), A being the average number of terms per class. Ties go to the
/// alphabetically first term.
pub(super) fn name(classes: &[BTreeMap<&str, u32>]) -> Vec<String> {
    let totals: Vec<u32> = classes.iter().map(|counts| counts.values().sum()).collect();
    let average = f64::from(totals.iter().sum::<u32>()) / classes.len() as f64;
    let mut across: HashMap<&str, u32> = HashMap::new();
    for counts in classes {
        for (&term, &count) in counts {
            *across.entry(term).or_default() += count;
        }
    }

    classes
        .iter()
        .zip(&totals)
        .map(|(counts, &total)| {
            let mut scored: Vec<(f64, &str)> = counts
                .iter()
                .map(|(&term, &count)| {
                    let frequency = f64::from(count) / f64::from(total);
                    let rarity = (1.0 + average / f64::from(across[term])).ln();
                    (frequency * rarity, term)
                })
                .collect();
            scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(b.1)));

            scored
                .iter()
                .take(NAME_TERMS)
                .map(|&(_, term)| term)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .collect()
}
