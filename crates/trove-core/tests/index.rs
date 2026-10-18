use std::{collections::BTreeSet, path::PathBuf, sync::Mutex, time::Duration};

use rusqlite::Connection;
use trove_core::{
    data_dir::DataDir,
    embedder::Embedder,
    index::{INDEX_FILE, Index, SCHEMA_VERSION},
    search::{SearchRequest, Signal, search},
};

#[test]
fn opening_an_empty_data_folder_creates_the_schema_and_reopening_keeps_it() {
    let temp = tempfile::tempdir().unwrap();
    let data_dir = DataDir::at(temp.path().join("data")).unwrap();

    let index = Index::open(&data_dir).unwrap();
    assert_eq!(index.schema_version().unwrap(), SCHEMA_VERSION);
    let device_id = index.device_id().unwrap();
    drop(index);

    let reopened = Index::open(&data_dir).unwrap();
    assert_eq!(reopened.schema_version().unwrap(), SCHEMA_VERSION);
    assert_eq!(reopened.device_id().unwrap(), device_id);
}

/// An index as schema version 1 made it: keyword search with plain `unicode61` over text as written, and a vector for
/// every chunk, even one of digits.
const VERSION_1: &str = "
    DROP TRIGGER chunks_fts_delete;
    DROP TABLE chunks_fts;
    DROP TABLE chunks_trigram;
    CREATE VIRTUAL TABLE chunks_fts USING fts5 (title, heading_path, text, tokenize = 'porter unicode61');
    CREATE TRIGGER chunks_fts_delete AFTER DELETE ON chunks BEGIN
        DELETE FROM chunks_fts WHERE rowid = old.id;
    END;
    ALTER TABLE chunks DROP COLUMN has_words;
    UPDATE meta SET value = 1 WHERE key = 'schema_version';

    INSERT INTO sources (id, path) VALUES (1, '/vault');
    INSERT INTO files (id, source_id, path, mtime, hash, title, properties)
    VALUES (1, 1, 'School.md', 0, x'01', 'School', '{}'), (2, 1, 'Readings.md', 0, x'02', 'Readings', '{}');
    INSERT INTO chunks (id, file_id, ordinal, heading_path, span_start, span_end, text_hash)
    VALUES (1, 1, 0, '', 0, 9, x'01'), (2, 2, 0, '', 0, 9, x'02');
    INSERT INTO chunks_fts (rowid, title, heading_path, text)
    VALUES (1, 'School', '', 'ذَهَبْتُ إِلَى الْمَدْرَسَةِ'), (2, 'Readings', '', '| 4815162342 | 7730021894 |');
    INSERT INTO vectors (text_hash, vector) VALUES (x'01', x'003c'), (x'02', x'003c');
";

/// The notes keyword search alone finds for `query`.
fn word_matches(index: &Mutex<Index>, query: &str) -> Vec<String> {
    // Never loaded: the search leaves out meaning.
    let embedder = Embedder::new(PathBuf::from("no model"), Duration::from_secs(1));
    let request = SearchRequest {
        query: query.to_owned(),
        limit: None,
        skip: BTreeSet::from([Signal::Meaning]),
    };

    search(index, &embedder, &request)
        .unwrap()
        .results
        .into_iter()
        .map(|result| result.path)
        .collect()
}

#[test]
fn a_version_1_index_is_migrated_to_keyword_search_over_normalised_text() {
    let temp = tempfile::tempdir().unwrap();
    let data_dir = DataDir::at(temp.path().join("data")).unwrap();
    drop(Index::open(&data_dir).unwrap());
    let path = data_dir.path().join(INDEX_FILE);
    Connection::open(&path)
        .unwrap()
        .execute_batch(VERSION_1)
        .unwrap();

    let index = Index::open(&data_dir).unwrap();
    assert_eq!(index.schema_version().unwrap(), SCHEMA_VERSION);

    let index = Mutex::new(index);
    assert_eq!(word_matches(&index, "الى المدرسة"), ["School.md"]);
    assert_eq!(word_matches(&index, "4815162342"), ["Readings.md"]);

    // The digits lose their vector.
    let vectors: Vec<Vec<u8>> = Connection::open(&path)
        .unwrap()
        .prepare("SELECT text_hash FROM vectors")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(vectors, [vec![0x01]]);
}
