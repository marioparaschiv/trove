use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use rusqlite::{OptionalExtension, Transaction};
use sha2::{Digest, Sha256};

use super::{
    IndexProblem, IndexReport, Indexer, IndexerError, NoteFile, neighbours::update_graph_edges,
};
use crate::{
    config::Config,
    embedder::{Embedder, vector_version},
    index::{self, Index},
    sync_store::{StoredChunk, SyncStore, VectorFile, vector_file_name},
};

/// The note's vector file when it was made under `vector_version`. A file that can't be read is reported and treated
/// as missing, so the note is chunked and embedded again and the file replaced.
pub(super) fn read_vector_file(
    store: &SyncStore,
    relative: &str,
    vector_version: &str,
    report: &mut IndexReport,
) -> Option<VectorFile> {
    store
        .read(relative, vector_version)
        .unwrap_or_else(|error| {
            report
                .problems
                .push(IndexProblem::new(&store.vector_file_path(relative), &error));
            None
        })
}

/// The stored chunks when `file` was made from exactly `text`, checked span by span.
pub(super) fn chunks_for<'a>(
    file: &'a VectorFile,
    text: &str,
    content_hash: &[u8],
) -> Option<&'a [StoredChunk]> {
    if file.content_hash != content_hash {
        return None;
    }

    let matches = |chunk: &StoredChunk| {
        text.get(chunk.span.clone())
            .is_some_and(|chunk_text| Sha256::digest(chunk_text).as_slice() == chunk.text_hash)
    };

    file.chunks.iter().all(matches).then_some(&file.chunks[..])
}

/// Adds the file's vectors to the index. Vectors already there are kept.
pub(super) fn import_vectors(tx: &Transaction<'_>, file: &VectorFile) -> rusqlite::Result<()> {
    insert_vectors(
        tx,
        file,
        "INSERT OR IGNORE INTO vectors (text_hash, vector) VALUES (?1, ?2)",
    )
}

fn insert_vectors(
    tx: &Transaction<'_>,
    file: &VectorFile,
    statement: &'static str,
) -> rusqlite::Result<()> {
    let mut insert = tx.prepare(statement)?;
    for chunk in &file.chunks {
        insert.execute((&chunk.text_hash, &chunk.vector))?;
    }

    Ok(())
}

/// Writes the vector file of each note in `file_ids` from the index. Failures are reported, since the local index is
/// still right without them.
pub(super) fn write_vector_files(
    index: &Mutex<Index>,
    store_folder: &str,
    file_ids: &[i64],
    vector_version: &str,
    report: &mut IndexReport,
) -> Result<(), IndexerError> {
    for &file_id in file_ids {
        let Some((source, relative, file)) = vector_file_from_index(index, file_id)? else {
            continue;
        };

        let store = SyncStore::new(Path::new(&source), store_folder);
        if let Err(error) = store.write(&relative, vector_version, &file) {
            report.problems.push(IndexProblem::new(
                &store.vector_file_path(&relative),
                &error,
            ));
        }
    }

    Ok(())
}

/// The note's source folder, path and vector file, or `None` when the note is gone or a chunk has no vector.
fn vector_file_from_index(
    index: &Mutex<Index>,
    file_id: i64,
) -> Result<Option<(String, String, VectorFile)>, IndexerError> {
    let index = index::lock(index);
    let conn = index.connection();
    let Some((source, relative, content_hash)) = conn
        .query_row(
            "SELECT s.path, f.path, f.hash FROM files f JOIN sources s ON s.id = f.source_id WHERE f.id = ?1",
            [file_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
    else {
        return Ok(None);
    };

    let mut statement = conn.prepare(
        "SELECT c.text_hash, c.heading_path, c.span_start, c.span_end, v.vector
         FROM chunks c LEFT JOIN vectors v USING (text_hash)
         WHERE c.file_id = ?1
         ORDER BY c.ordinal",
    )?;
    let rows = statement
        .query_map([file_id], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, usize>(2)?,
                row.get::<_, usize>(3)?,
                row.get::<_, Option<Vec<u8>>>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut chunks = Vec::with_capacity(rows.len());
    for (text_hash, heading_path, start, end, vector) in rows {
        let Some(vector) = vector else {
            return Ok(None);
        };

        chunks.push(StoredChunk {
            text_hash,
            heading_path,
            span: start..end,
            vector,
        });
    }

    let dims = chunks.first().map_or(0, |chunk| chunk.vector.len() / 2);
    let file = VectorFile {
        content_hash,
        dims: u32::try_from(dims).expect("a vector has far fewer than 2^32 dimensions"),
        chunks,
    };

    Ok(Some((source, relative, file)))
}

/// Imports the vector files at `paths`, as the watcher reported them, whose note is indexed and unchanged on disk,
/// matches the file and has other vectors in the index. The file's vectors replace the index's, so both devices end up
/// with the same bytes. Never loads the model. `files` in the report counts the notes imported.
pub fn import_vector_files(
    index: &Mutex<Index>,
    embedder: &Arc<Embedder>,
    config: &Config,
    paths: &[PathBuf],
) -> Result<IndexReport, IndexerError> {
    let vector_version = vector_version(&config.model.sha256);
    let sources = index::lock(index).sources()?;
    let indexer = Indexer::standard(Arc::clone(embedder));
    let mut report = IndexReport::default();

    for source in &sources {
        let root = Path::new(&source.path);
        let names: HashSet<String> = paths
            .iter()
            .filter_map(|path| arrived_name(path, root, &config.store_folder))
            .collect();
        if names.is_empty() {
            continue;
        }

        let store = SyncStore::new(root, &config.store_folder);
        for note in notes_named(index, source.id, &names)? {
            if import_note(
                index,
                &indexer,
                root,
                &store,
                &note,
                &vector_version,
                &mut report,
            )? {
                report.files += 1;
            }
        }
    }

    if report.files > 0 {
        update_graph_edges(index)?;
    }

    Ok(report)
}

/// The note hash `path` names when it is a vector file in `root`'s store folder. Watchers may report a resolved
/// spelling of the source (macOS reports `/var/...` as `/private/var/...`).
fn arrived_name(path: &Path, root: &Path, store_folder: &str) -> Option<String> {
    let roots = [Some(root.to_path_buf()), fs::canonicalize(root).ok()];
    let paths = [Some(path.to_path_buf()), fs::canonicalize(path).ok()];

    roots.iter().flatten().find_map(|root| {
        paths.iter().flatten().find_map(|path| {
            let relative = path.strip_prefix(root).ok()?;
            vector_file_name(relative, store_folder).map(str::to_owned)
        })
    })
}

struct IndexedNote {
    id: i64,
    path: String,
    hash: Vec<u8>,
    title: String,
}

/// The source's notes whose vector file names are among `names`.
fn notes_named(
    index: &Mutex<Index>,
    source_id: i64,
    names: &HashSet<String>,
) -> Result<Vec<IndexedNote>, IndexerError> {
    let index = index::lock(index);
    let mut statement = index
        .connection()
        .prepare("SELECT id, path, hash, title FROM files WHERE source_id = ?1")?;
    let notes = statement
        .query_map([source_id], |row| {
            Ok(IndexedNote {
                id: row.get(0)?,
                path: row.get(1)?,
                hash: row.get(2)?,
                title: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(notes
        .into_iter()
        .filter(|note| names.contains(&hex::encode(Sha256::digest(&note.path))))
        .collect())
}

/// Whether the note took its chunks and vectors from its vector file.
fn import_note(
    index: &Mutex<Index>,
    indexer: &Indexer,
    root: &Path,
    store: &SyncStore,
    note: &IndexedNote,
    vector_version: &str,
    report: &mut IndexReport,
) -> Result<bool, IndexerError> {
    let path = root.join(&note.path);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            report.problems.push(IndexProblem::new(&path, &error));
            return Ok(false);
        }
    };
    // A note changed since it was indexed is chunked by its own run, which reads the file then.
    if Sha256::digest(&bytes).as_slice() != note.hash {
        return Ok(false);
    }
    let text = String::from_utf8(bytes).expect("an indexed note's text is UTF-8");

    let Some(file) = read_vector_file(store, &note.path, vector_version, report) else {
        return Ok(false);
    };
    let Some(chunks) = chunks_for(&file, &text, &note.hash) else {
        return Ok(false);
    };
    if vector_file_from_index(index, note.id)?.is_some_and(|(_, _, current)| current == file) {
        return Ok(false);
    }

    let mut index = index::lock(index);
    let tx = index.transaction()?;
    insert_vectors(
        &tx,
        &file,
        "INSERT OR REPLACE INTO vectors (text_hash, vector) VALUES (?1, ?2)",
    )?;
    indexer.process(
        &tx,
        &NoteFile {
            id: note.id,
            title: &note.title,
            text: &text,
            stored_chunks: Some(chunks),
        },
    )?;
    tx.commit()?;

    Ok(true)
}
