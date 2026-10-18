//! Checks that each eval query keeps the rule of its kind, with words as search sees them: through the index's own
//! FTS5 tokenizer.

mod common;

use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::Path,
};

use common::{EVAL_QUERIES, EVAL_VAULT, fixture_notes};
use rusqlite::Connection;
use serde::Deserialize;

const INDEX_SCHEMA: &str = include_str!("../../trove-core/src/index/schema.sql");

/// A lookup query names a term that is in at most this many notes.
const LOOKUP_MAX_NOTES: usize = 3;

#[derive(Deserialize)]
struct EvalQuery {
    query: String,
    kind: String,
    lang: String,
    note: Option<String>,
}

/// The eval vault in an in-memory FTS5 table made with the index's tokenizer, so a term here is a term search
/// matches on.
struct Corpus {
    conn: Connection,
    notes_by_term: HashMap<String, BTreeSet<String>>,
    notes: BTreeSet<String>,
}

impl Corpus {
    fn load() -> Self {
        let tokenize = index_tokenizer();
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!(
            "CREATE VIRTUAL TABLE notes USING fts5 (path UNINDEXED, title, text, tokenize = {tokenize});
             CREATE VIRTUAL TABLE note_terms USING fts5vocab (notes, 'instance');
             CREATE VIRTUAL TABLE scratch USING fts5 (text, tokenize = {tokenize});
             CREATE VIRTUAL TABLE scratch_terms USING fts5vocab (scratch, 'row');"
        ))
        .unwrap();

        let notes = fixture_notes(Path::new(EVAL_VAULT));
        for path in &notes {
            let text = fs::read_to_string(Path::new(EVAL_VAULT).join(path)).unwrap();
            let title = Path::new(path).file_stem().unwrap().to_str().unwrap();
            conn.execute(
                "INSERT INTO notes (path, title, text) VALUES (?1, ?2, ?3)",
                (path, title, body(&text)),
            )
            .unwrap();
        }

        let mut notes_by_term: HashMap<String, BTreeSet<String>> = HashMap::new();
        let mut statement = conn
            .prepare(
                "SELECT DISTINCT note_terms.term, notes.path
                 FROM note_terms JOIN notes ON notes.rowid = note_terms.doc",
            )
            .unwrap();
        let rows = statement
            .query_map((), |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap();
        for row in rows {
            let (term, path) = row.unwrap();
            notes_by_term.entry(term).or_default().insert(path);
        }
        drop(statement);

        Self {
            conn,
            notes_by_term,
            notes,
        }
    }

    /// The terms the tokenizer makes of `text`.
    fn terms(&self, text: &str) -> BTreeSet<String> {
        self.conn.execute("DELETE FROM scratch", ()).unwrap();
        self.conn
            .execute("INSERT INTO scratch (text) VALUES (?1)", [text])
            .unwrap();
        let mut statement = self.conn.prepare("SELECT term FROM scratch_terms").unwrap();
        statement
            .query_map((), |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    fn notes_with(&self, term: &str) -> BTreeSet<String> {
        self.notes_by_term.get(term).cloned().unwrap_or_default()
    }
}

/// The `tokenize` option of the index's `chunks_fts` table, quoted as in the schema.
fn index_tokenizer() -> &'static str {
    let (_, table) = INDEX_SCHEMA
        .split_once("CREATE VIRTUAL TABLE chunks_fts")
        .expect("the schema creates chunks_fts");
    let (_, option) = table
        .split_once("tokenize = ")
        .expect("chunks_fts names its tokenizer");
    let end = option[1..].find('\'').expect("the tokenizer is quoted") + 2;

    &option[..end]
}

/// The note text after its frontmatter, which the index leaves out of chunks.
fn body(text: &str) -> &str {
    text.strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map_or(text, |(_, body)| body)
}

/// What is wrong with `query`, given its content terms, if anything.
fn rule_break(corpus: &Corpus, query: &EvalQuery, content: &BTreeSet<String>) -> Option<String> {
    if (query.kind == "no_answer") != query.note.is_none() {
        return Some("only a no_answer query has a null note".to_owned());
    }
    if let Some(note) = &query.note
        && !corpus.notes.contains(note)
    {
        return Some(format!("there is no note {note}"));
    }

    match (query.kind.as_str(), query.note.as_deref()) {
        ("paraphrase", Some(note)) => {
            let shared: Vec<&String> = content
                .iter()
                .filter(|term| corpus.notes_with(term).contains(note))
                .collect();

            (!shared.is_empty()).then(|| format!("shares {shared:?} with its note"))
        }
        ("lookup", Some(note)) => {
            let rare_in_note = content.iter().any(|term| {
                let notes = corpus.notes_with(term);
                notes.contains(note) && notes.len() <= LOOKUP_MAX_NOTES
            });

            (!rare_in_note).then(|| {
                format!("no term of it is in its note and in at most {LOOKUP_MAX_NOTES} notes")
            })
        }
        ("no_answer", None) => {
            let found: Vec<&String> = content
                .iter()
                .filter(|term| corpus.notes_by_term.contains_key(*term))
                .collect();

            (!found.is_empty()).then(|| format!("the corpus has {found:?}"))
        }
        _ => None,
    }
}

#[test]
fn every_eval_query_keeps_the_rule_of_its_kind() {
    let corpus = Corpus::load();
    let queries: Vec<EvalQuery> =
        serde_json::from_str(&fs::read_to_string(EVAL_QUERIES).unwrap()).unwrap();

    // Stopwords go through the same stemmer, so a content term is a query term that no stopword stems to.
    let mut stop_terms: HashMap<&str, BTreeSet<String>> = HashMap::new();
    let mut broken = Vec::new();
    for query in &queries {
        let stop = stop_terms.entry(&query.lang).or_insert_with(|| {
            let words = stop_words::lookup(&query.lang)
                .unwrap_or_else(|| panic!("no stopwords for language {:?}", query.lang));
            corpus.terms(&words.join(" "))
        });
        let content: BTreeSet<String> = corpus
            .terms(&query.query)
            .difference(stop)
            .cloned()
            .collect();

        if let Some(problem) = rule_break(&corpus, query, &content) {
            broken.push(format!("{:?} ({}): {problem}", query.query, query.kind));
        }
    }

    assert!(
        broken.is_empty(),
        "{} queries break the rule of their kind:\n{}",
        broken.len(),
        broken.join("\n")
    );
}
