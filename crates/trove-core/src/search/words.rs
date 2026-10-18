use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::{LazyLock, Mutex},
};

use rusqlite::Connection;

use super::{Candidate, Query, SearchError, Signal, Stage};
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

/// Search by words, as DigUp does: full-text search over each chunk's note title, heading path and text, normalised
/// and Porter-stemmed. A chunk matches when it has any of the query's words ([`terms`]). Adds its matches, best BM25
/// first, each scored with its note's coverage: the share of the query's words found anywhere in the note.
pub struct Words<'a> {
    index: &'a Mutex<Index>,
}

impl<'a> Words<'a> {
    pub fn new(index: &'a Mutex<Index>) -> Self {
        Self { index }
    }
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

        let mut words_in_note: HashMap<i64, usize> = HashMap::new();
        let mut notes_with = conn.prepare(
            "SELECT DISTINCT c.file_id FROM chunks_fts JOIN chunks c ON c.id = chunks_fts.rowid
             WHERE chunks_fts MATCH ?1",
        )?;
        for pattern in &patterns {
            for file_id in notes_with.query_map([pattern], |row| row.get(0))? {
                *words_in_note.entry(file_id?).or_default() += 1;
            }
        }

        // Weights per column: title, heading path, text.
        let mut matches = conn.prepare(
            "SELECT chunks_fts.rowid, c.file_id FROM chunks_fts JOIN chunks c ON c.id = chunks_fts.rowid
             WHERE chunks_fts MATCH ?1 ORDER BY bm25(chunks_fts, 3.0, 2.0, 1.0)",
        )?;
        let rows = matches.query_map([patterns.join(" OR ")], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (chunk_id, file_id) = row?;
            candidates.push(Candidate {
                chunk_id,
                score: words_in_note[&file_id] as f32 / patterns.len() as f32,
                why: BTreeSet::from([Signal::Words]),
            });
        }

        Ok(candidates)
    }
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

/// One FTS5 pattern per term, quoted. A term of a language that attaches endings ([`inflects`]) is a prefix, so is a
/// term of [`MIN_PREFIX_CHARS`] or more that is in no chunk as typed: "grok" finds "grokipedia". A term that is in the
/// index as typed stays whole, so "cat" doesn't bring in "category".
fn patterns(conn: &Connection, terms: &[String]) -> rusqlite::Result<Vec<String>> {
    let mut is_indexed =
        conn.prepare("SELECT 1 FROM chunks_fts WHERE chunks_fts MATCH ?1 LIMIT 1")?;

    terms
        .iter()
        .map(|term| {
            let whole = format!("\"{term}\"");
            let prefix = inflects(term)
                || (term.chars().count() >= MIN_PREFIX_CHARS && !is_indexed.exists([&whole])?);

            Ok(if prefix { whole + "*" } else { whole })
        })
        .collect()
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
