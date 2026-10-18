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

/// Keyword search as schema version 1 made it: plain `unicode61`, text as written.
const VERSION_1: &str = "
    DROP TRIGGER chunks_fts_delete;
    DROP TABLE chunks_fts;
    CREATE VIRTUAL TABLE chunks_fts USING fts5 (title, heading_path, text, tokenize = 'porter unicode61');
    CREATE TRIGGER chunks_fts_delete AFTER DELETE ON chunks BEGIN
        DELETE FROM chunks_fts WHERE rowid = old.id;
    END;
    UPDATE meta SET value = 1 WHERE key = 'schema_version';

    INSERT INTO sources (id, path) VALUES (1, '/vault');
    INSERT INTO files (id, source_id, path, mtime, hash, title, properties)
    VALUES (1, 1, 'School.md', 0, x'01', 'School', '{}');
    INSERT INTO chunks (id, file_id, ordinal, heading_path, span_start, span_end, text_hash)
    VALUES (1, 1, 0, '', 0, 9, x'01');
    INSERT INTO chunks_fts (rowid, title, heading_path, text) VALUES (1, 'School', '', 'ذَهَبْتُ إِلَى الْمَدْرَسَةِ');
";

#[test]
fn a_version_1_index_is_migrated_to_keyword_search_over_normalised_text() {
    let temp = tempfile::tempdir().unwrap();
    let data_dir = DataDir::at(temp.path().join("data")).unwrap();
    drop(Index::open(&data_dir).unwrap());
    Connection::open(data_dir.path().join(INDEX_FILE))
        .unwrap()
        .execute_batch(VERSION_1)
        .unwrap();

    let index = Index::open(&data_dir).unwrap();
    assert_eq!(index.schema_version().unwrap(), SCHEMA_VERSION);

    let index = Mutex::new(index);
    // Never loaded: the search leaves out meaning.
    let embedder = Embedder::new(PathBuf::from("no model"), Duration::from_secs(1));
    let request = SearchRequest {
        query: "الى المدرسة".to_owned(),
        limit: None,
        skip: BTreeSet::from([Signal::Meaning]),
    };
    let paths: Vec<String> = search(&index, &embedder, &request)
        .unwrap()
        .results
        .into_iter()
        .map(|result| result.path)
        .collect();
    assert_eq!(paths, ["School.md"]);
}
