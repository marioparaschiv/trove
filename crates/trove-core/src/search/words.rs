use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::{LazyLock, Mutex},
};

use rusqlite::Connection;

use super::{Candidate, Coverage, Query, SearchError, Signal, Stage};
use crate::index::{self, Index, keyword_text};

/// DigUp's stopwords (`Searcher.stopwords`), English then Bengali and Arabic. They say how to ask, not what to find, so
/// they are left out of a query's words. Kept verbatim (see THIRD_PARTY_NOTICES.md), and compared normalised.
const STOPWORDS: &[&str] = &[
    "a",
    "an",
    "the",
    "of",
    "in",
    "on",
    "at",
    "to",
    "for",
    "with",
    "and",
    "or",
    "my",
    "me",
    "that",
    "this",
    "these",
    "those",
    "from",
    "about",
    "where",
    "what",
    "which",
    "who",
    "when",
    "is",
    "was",
    "are",
    "were",
    "be",
    "it",
    "its",
    "some",
    "any",
    "find",
    "show",
    "file",
    "files",
    "photo",
    "photos",
    "picture",
    "image",
    "images",
    "screenshot",
    "screenshots",
    "video",
    "videos",
    "pdf",
    "document",
    "documents",
    "recording",
    "এবং",
    "ও",
    "এই",
    "সেই",
    "একটি",
    "আমার",
    "থেকে",
    "জন্য",
    "সাথে",
    "মধ্যে",
    "কোথায়",
    "কোন",
    "কী",
    "কি",
    "যে",
    "ছবি",
    "স্ক্রিনশট",
    "ভিডিও",
    "ফাইল",
    "পিডিএফ",
    "في",
    "من",
    "على",
    "الى",
    "عن",
    "مع",
    "هذا",
    "هذه",
    "ذلك",
    "التي",
    "الذي",
    "اين",
    "ما",
    "صورة",
    "صور",
    "لقطة",
    "شاشة",
    "فيديو",
    "ملف",
    "مستند",
];

static NORMALIZED_STOPWORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
    STOPWORDS
        .iter()
        .map(|word| keyword_text::normalized(word))
        .collect()
});

/// A word this long or longer that is in no chunk as typed matches as the start of longer words. Shorter words start
/// too many words to stand for one.
const MIN_PREFIX_CHARS: usize = 3;

/// How many chunks sharing the most character runs with a misspelt word are read for the word it stands for. Bounds
/// the work for a word that matches nothing.
const MAX_SPELLING_CHUNKS: usize = 20;
/// A misspelt word this long or longer may be two typing slips from the word it stands for, a shorter one only one.
/// Shorter words two slips apart are as often other words: "growth" isn't "grow", nor "summit" "summa".
const TWO_SLIP_CHARS: usize = 8;

/// Search by words, as DigUp does: full-text search over each chunk's note title, heading path and text, normalised
/// and Porter-stemmed. A chunk matches when it has any of the query's words ([`terms`]). Adds its matches, best BM25
/// first, each scored with its note's coverage: the share of the query's words found anywhere in the note.
///
/// A word that matches nothing as typed is looked for in the trigram table, which only adds matches: a word in Chinese,
/// Japanese or Thai as a run of characters inside longer text, and a misspelt word as the closest word in the index.
pub struct Words<'a> {
    index: &'a Mutex<Index>,
}

impl<'a> Words<'a> {
    pub fn new(index: &'a Mutex<Index>) -> Self {
        Self { index }
    }
}

/// How one query word is looked for.
enum Pattern {
    /// An FTS5 pattern over `chunks_fts`.
    Words(String),
    /// A run of characters anywhere in a chunk.
    Substring(String),
}

impl Stage for Words<'_> {
    fn run(
        &self,
        query: &Query,
        mut candidates: Vec<Candidate>,
    ) -> Result<Vec<Candidate>, SearchError> {
        let terms = terms(&query.text);
        if terms.is_empty() {
            return Ok(candidates);
        }

        let index = index::lock(self.index);
        let conn = index.connection();
        let patterns = patterns(conn, &terms)?;

        let mut word_patterns = Vec::new();
        let mut substring_matches = Vec::new();
        for pattern in patterns {
            match pattern {
                Pattern::Words(pattern) => word_patterns.push(pattern),
                Pattern::Substring(run) => substring_matches.push(substring_chunks(conn, &run)?),
            }
        }

        let mut notes_with = conn.prepare(
            "SELECT DISTINCT c.file_id FROM chunks_fts JOIN chunks c ON c.id = chunks_fts.rowid
             WHERE chunks_fts MATCH ?1",
        )?;
        let mut notes_by_word: Vec<HashSet<i64>> = word_patterns
            .iter()
            .map(|pattern| notes_with.query_map([pattern], |row| row.get(0))?.collect())
            .collect::<rusqlite::Result<_>>()?;
        notes_by_word.extend(
            substring_matches
                .iter()
                .map(|chunks| chunks.iter().map(|&(_, file_id)| file_id).collect()),
        );
        let coverage = coverage(conn, &notes_by_word)?;

        let mut matched = Vec::new();
        if !word_patterns.is_empty() {
            // Weights per column: title, heading path, text.
            let mut matches = conn.prepare(
                "SELECT chunks_fts.rowid, c.file_id FROM chunks_fts JOIN chunks c ON c.id = chunks_fts.rowid
                 WHERE chunks_fts MATCH ?1 ORDER BY bm25(chunks_fts, 3.0, 2.0, 1.0)",
            )?;
            for row in matches.query_map([word_patterns.join(" OR ")], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
            })? {
                matched.push(row?);
            }
        }
        matched.extend(substring_matches.into_iter().flatten());

        let mut seen = HashSet::new();
        for (chunk_id, file_id) in matched {
            if !seen.insert(chunk_id) {
                continue;
            }

            candidates.push(Candidate {
                chunk_id,
                score: coverage[&file_id].share,
                why: BTreeSet::from([Signal::Words]),
                coverage: Some(coverage[&file_id]),
            });
        }

        Ok(candidates)
    }
}

/// How much of the query each note has, from the notes each of the query's words is in. A word weighs its inverse
/// document frequency over notes (BM25's), so a note with "sumac" ranks above one with only "recipe" for "recipe with
/// sumac".
fn coverage(
    conn: &Connection,
    notes_by_word: &[HashSet<i64>],
) -> rusqlite::Result<HashMap<i64, Coverage>> {
    let notes: f32 = conn.query_row("SELECT count(*) FROM files", [], |row| row.get(0))?;
    let weights: Vec<f32> = notes_by_word
        .iter()
        .map(|with| {
            let with = with.len() as f32;
            (1.0 + (notes - with + 0.5) / (with + 0.5)).ln()
        })
        .collect();
    let total_weight: f32 = weights.iter().sum();

    let mut found: HashMap<i64, (usize, f32)> = HashMap::new();
    for (with, weight) in notes_by_word.iter().zip(&weights) {
        for &file_id in with {
            let (words, words_weight) = found.entry(file_id).or_default();
            *words += 1;
            *words_weight += weight;
        }
    }

    Ok(found
        .into_iter()
        .map(|(file_id, (words, words_weight))| {
            let coverage = Coverage {
                share: words as f32 / notes_by_word.len() as f32,
                weighted: words_weight / total_weight,
            };
            (file_id, coverage)
        })
        .collect())
}

/// The content words of a query, normalised as keyword search keeps text: each run of word characters, lowercased,
/// of two characters or more, stopwords and repeats left out.
pub(super) fn terms(text: &str) -> Vec<String> {
    let lowercase = keyword_text::normalized(text).to_lowercase();
    let mut seen = HashSet::new();

    lowercase
        .split(|c: char| !keyword_text::is_word_char(c))
        .filter(|word| {
            word.chars().count() >= 2 && !NORMALIZED_STOPWORDS.contains(*word) && seen.insert(*word)
        })
        .map(str::to_owned)
        .collect()
}

/// Whether the query is a lookup: every word is a code or an ID (`K7Q2LM`, `TRX50`) or a long number (an order or a
/// phone number). They mean nothing to an embedding but everything to keyword search.
pub(super) fn is_lookup(terms: &[String]) -> bool {
    !terms.is_empty()
        && terms.iter().all(|term| {
            term.chars().any(char::is_numeric)
                && (term.chars().any(char::is_alphabetic) || term.chars().count() >= 4)
        })
}

/// How each term is looked for. A term in the index as typed stays whole, so "cat" doesn't bring in "category". Else
/// a term in Chinese, Japanese or Thai is a run of characters ([`is_unsegmented`]). A term of a language that attaches
/// endings ([`inflects`]) is a prefix, so is a term of [`MIN_PREFIX_CHARS`] or more: "grok" finds "grokipedia". A term
/// that matches nothing even so stands for the closest word in the index ([`closest_word`]) when it has one.
fn patterns(conn: &Connection, terms: &[String]) -> rusqlite::Result<Vec<Pattern>> {
    let mut is_indexed =
        conn.prepare("SELECT 1 FROM chunks_fts WHERE chunks_fts MATCH ?1 LIMIT 1")?;

    terms
        .iter()
        .map(|term| {
            let whole = format!("\"{term}\"");
            if !inflects(term) && is_indexed.exists([&whole])? {
                return Ok(Pattern::Words(whole));
            }
            if is_unsegmented(term) {
                return Ok(Pattern::Substring(term.clone()));
            }

            let pattern = if inflects(term) || term.chars().count() >= MIN_PREFIX_CHARS {
                whole + "*"
            } else {
                whole
            };
            if is_indexed.exists([&pattern])? || term.chars().any(char::is_numeric) {
                return Ok(Pattern::Words(pattern));
            }

            Ok(Pattern::Words(
                closest_word(conn, term)?.map_or(pattern, |word| format!("\"{word}\"")),
            ))
        })
        .collect()
}

/// The chunks with `run` in their title, heading path or text, and their notes, best BM25 first. The trigram table
/// matches runs of three characters or more; a shorter run is looked for in every chunk.
fn substring_chunks(conn: &Connection, run: &str) -> rusqlite::Result<Vec<(i64, i64)>> {
    let (sql, parameter) = if run.chars().count() >= 3 {
        (
            "SELECT t.rowid, c.file_id FROM chunks_trigram t JOIN chunks c ON c.id = t.rowid
             WHERE chunks_trigram MATCH ?1 ORDER BY t.rank",
            format!("\"{run}\""),
        )
    } else {
        (
            "SELECT f.rowid, c.file_id FROM chunks_fts f JOIN chunks c ON c.id = f.rowid
             WHERE instr(f.title, ?1) OR instr(f.heading_path, ?1) OR instr(f.text, ?1) ORDER BY f.rowid",
            run.to_owned(),
        )
    };

    conn.prepare(sql)?
        .query_map([parameter], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect()
}

/// The indexed word most like `term` ([`similarity`]) of those a typing slip or two away ([`slips`],
/// [`TWO_SLIP_CHARS`]), from the [`MAX_SPELLING_CHUNKS`] chunks that share the most of its runs of three characters.
/// "kubernets" finds "kubernetes". Words further apart are as often other words: "hotter" isn't "hot", nor "runaway"
/// "run".
fn closest_word(conn: &Connection, term: &str) -> rusqlite::Result<Option<String>> {
    let chars: Vec<char> = term.chars().collect();
    let max_slips = if chars.len() >= TWO_SLIP_CHARS { 2 } else { 1 };
    let runs: Vec<String> = chars
        .windows(3)
        .map(|run| format!("\"{}\"", run.iter().collect::<String>()))
        .collect();
    if runs.is_empty() {
        return Ok(None);
    }

    let mut chunks = conn.prepare(
        "SELECT f.title, f.heading_path, f.text FROM chunks_trigram t JOIN chunks_fts f ON f.rowid = t.rowid
         WHERE chunks_trigram MATCH ?1 ORDER BY t.rank LIMIT ?2",
    )?;
    let rows = chunks.query_map((runs.join(" OR "), MAX_SPELLING_CHUNKS), |row| {
        Ok([row.get::<_, String>(0)?, row.get(1)?, row.get(2)?].join(" "))
    })?;

    let mut seen = HashSet::new();
    let mut closest: Option<(f32, String)> = None;
    for text in rows {
        for word in text?
            .to_lowercase()
            .split(|c: char| !keyword_text::is_word_char(c))
        {
            if !seen.insert(word.to_owned()) || !(1..=max_slips).contains(&slips(term, word)) {
                continue;
            }

            let alike = similarity(term, word);
            if closest.as_ref().is_none_or(|(best, _)| alike > *best) {
                closest = Some((alike, word.to_owned()));
            }
        }
    }

    Ok(closest.map(|(_, word)| word))
}

/// How many typing slips turn `a` into `b`: letters changed, added or left out, and pairs next to each other swapped
/// (optimal string alignment distance).
fn slips(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut distance: Vec<Vec<usize>> = (0..=a.len())
        .map(|i| (0..=b.len()).map(|j| if i == 0 { j } else { i }).collect())
        .collect();

    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let changed = usize::from(a[i - 1] != b[j - 1]);
            distance[i][j] = (distance[i - 1][j] + 1)
                .min(distance[i][j - 1] + 1)
                .min(distance[i - 1][j - 1] + changed);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                distance[i][j] = distance[i][j].min(distance[i - 2][j - 2] + 1);
            }
        }
    }

    distance[a.len()][b.len()]
}

/// How alike two words are as `pg_trgm` counts it: the share of their runs of three characters they have in common,
/// each word padded with two spaces before and one after, so the start and the end of a word weigh more.
fn similarity(a: &str, b: &str) -> f32 {
    let runs = |word: &str| -> HashSet<[char; 3]> {
        let padded: Vec<char> = format!("  {word} ").chars().collect();
        padded
            .windows(3)
            .map(|run| [run[0], run[1], run[2]])
            .collect()
    };
    let (a, b) = (runs(a), runs(b));

    a.intersection(&b).count() as f32 / a.union(&b).count() as f32
}

/// Whether `term` is in Chinese, Japanese or Thai, which don't separate words, so the tokenizer keeps a whole run of
/// text as one word.
fn is_unsegmented(term: &str) -> bool {
    term.chars().any(|c| {
        matches!(c,
            '\u{0E00}'..='\u{0E7F}' // Thai
            | '\u{3040}'..='\u{30FF}' // Hiragana, Katakana
            | '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' // Han
            | '\u{20000}'..='\u{3134F}')
    })
}

/// Whether `term` is in a script that attaches endings to words (Indic scripts, Arabic): "مدرس" finds "مدرسة".
fn inflects(term: &str) -> bool {
    term.chars()
        .any(|c| matches!(c, '\u{0900}'..='\u{0DFF}' | '\u{0600}'..='\u{06FF}'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_are_normalised_lowercased_content_words() {
        assert_eq!(
            terms("Where is the Boiler-service log? the log!"),
            ["boiler", "service", "log"]
        );
        assert_eq!(terms("أين الْمَدْرَسَة في القاهرة"), ["المدرسة", "القاهرة"]);
        assert_eq!(terms("हिन्दी किताबें"), ["हिन्दी", "किताबें"]);
    }

    #[test]
    fn a_slip_is_a_letter_changed_added_left_out_or_swapped() {
        assert_eq!(slips("kubernetis", "kubernetes"), 1);
        assert_eq!(slips("aple", "apple"), 1);
        assert_eq!(slips("electrolytes", "electrolyts"), 1);
        assert_eq!(slips("bagle", "bagel"), 1);
        assert_eq!(slips("captial", "capital"), 1);
        assert_eq!(slips("bagel", "bagel"), 0);
        assert_eq!(slips("growth", "grow"), 2);
        assert_eq!(slips("zustimmen", "zustimmung"), 2);
        assert_eq!(slips("abcd", "badc"), 2);
    }

    #[test]
    fn a_lookup_is_made_of_codes_and_long_numbers() {
        let lookup = |text: &str| is_lookup(&terms(text));
        assert!(lookup("K7Q2LM"));
        assert!(lookup("4815162342"));
        assert!(lookup("TRX50 7Q4K-29XP"));
        assert!(!lookup("order 4815162342"));
        assert!(!lookup("room 12"));
        assert!(!lookup("the"));
    }
}
