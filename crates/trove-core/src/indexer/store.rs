use std::{path::Path, sync::Mutex};

use rusqlite::{OptionalExtension, Transaction};
use sha2::{Digest, Sha256};

use super::{IndexProblem, IndexReport, IndexerError};
use crate::{
    index::{self, Index},
    sync_store::{StoredChunk, SyncStore, VectorFile},
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
    let mut insert =
        tx.prepare("INSERT OR IGNORE INTO vectors (text_hash, vector) VALUES (?1, ?2)")?;
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
