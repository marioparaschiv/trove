use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;

use super::{
    Indexer, IndexerError, NoteFile,
    embed::{EmbedProgress, embed_missing},
    frontmatter::properties,
    neighbours::update_graph_edges,
    store::{chunks_for, import_vectors, read_vector_file, write_vector_files},
};
use crate::{
    config::Config,
    embedder::{Embedder, vector_version},
    error::full_message,
    index::{self, Index, IndexError, Source},
    sync_store::{SyncStore, VectorFile},
};

const NOTE_EXTENSION: &str = "md";

/// Which notes an index run chunks and which chunk texts it embeds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IndexMode {
    /// Chunks the notes whose content changed, finding them by modification time first, then by content hash.
    #[default]
    Changed,
    /// Reads and chunks every note, but still embeds only chunk texts without a vector.
    Full,
    /// Drops every vector, then reads, chunks and embeds every note.
    Reembed,
}

/// What an index run did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IndexReport {
    /// Notes read and chunked.
    pub files: u64,
    /// Notes dropped from the index because they no longer exist.
    pub removed: u64,
    /// Chunk texts embedded; texts that already had a vector are not counted.
    pub embedded: u64,
    /// Files and folders that could not be indexed, or were indexed only in part.
    pub problems: Vec<IndexProblem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IndexProblem {
    pub path: String,
    pub message: String,
}

impl IndexProblem {
    pub(super) fn new(path: &Path, error: &dyn std::error::Error) -> Self {
        Self {
            path: path.display().to_string(),
            message: full_message(error),
        }
    }
}

/// A note found in a source.
struct Found {
    path: PathBuf,
    /// Relative to the source, `/`-separated.
    relative: String,
}

/// The `files` row of a note as last indexed.
struct Stored {
    mtime: i64,
    hash: Vec<u8>,
}

/// Chunks the notes `mode` picks in every source, drops notes that are gone and the vectors no chunk uses any more,
/// then embeds the chunk texts that have no vector and writes the vector files of the notes that changed to the sync
/// store. A note whose vector file matches it takes its chunks and vectors from there, without the model. The index is
/// locked one note or one batch at a time, so searches go on meanwhile.
pub fn index_sources(
    index: &Mutex<Index>,
    embedder: &Arc<Embedder>,
    config: &Config,
    mode: IndexMode,
    progress: &mut dyn FnMut(EmbedProgress),
) -> Result<IndexReport, IndexerError> {
    let vector_version = vector_version(&config.model.sha256);
    let (sources, mode) = {
        let mut index = index::lock(index);
        // Another version may chunk differently, so every note is chunked again, not only the changed ones.
        let mode = if index.ensure_vector_version(&vector_version)? && mode == IndexMode::Changed {
            IndexMode::Full
        } else {
            mode
        };
        if mode == IndexMode::Reembed {
            index.connection().execute("DELETE FROM vectors", [])?;
        }

        (index.sources()?, mode)
    };

    let indexer = Indexer::standard(Arc::clone(embedder));
    let mut report = IndexReport::default();

    let mut written = Vec::new();
    for source in &sources {
        written.extend(index_source(
            index,
            &indexer,
            source,
            config,
            mode,
            &vector_version,
            &mut report,
        )?);
    }

    let changed = {
        let index = index::lock(index);
        // Only once every source is scanned, so a note moved between folders keeps its vectors.
        index.connection().execute(
            "DELETE FROM vectors WHERE text_hash NOT IN (SELECT text_hash FROM chunks)",
            [],
        )?;
        notes_to_store(&index, &written)?
    };
    report.embedded = embed_missing(index, embedder, progress)?;
    write_vector_files(
        index,
        &config.store_folder,
        &changed,
        &vector_version,
        &mut report,
    )?;
    update_graph_edges(index)?;
    index::lock(index)
        .rebuild_vector_scan()
        .map_err(IndexError::from)?;

    Ok(report)
}

/// The notes chunked in this run and those with a chunk text about to be embedded, whose vector files are out of date.
fn notes_to_store(index: &Index, written: &[i64]) -> rusqlite::Result<Vec<i64>> {
    let written = serde_json::to_string(written).expect("a list of ids serialises");
    let mut statement = index.connection().prepare(
        "SELECT value FROM json_each(?1)
         UNION
         SELECT c.file_id FROM chunks c
         WHERE c.has_words AND NOT EXISTS (SELECT 1 FROM vectors v WHERE v.text_hash = c.text_hash)",
    )?;

    statement.query_map([written], |row| row.get(0))?.collect()
}

fn index_source(
    index: &Mutex<Index>,
    indexer: &Indexer,
    source: &Source,
    config: &Config,
    mode: IndexMode,
    vector_version: &str,
    report: &mut IndexReport,
) -> Result<Vec<i64>, IndexerError> {
    let root = Path::new(&source.path);
    let mut listing = Listing::default();
    // A source that can't be read now (an unplugged drive) keeps what was indexed from it.
    if let Err(error) = find_notes(root, "", &config.store_folder, &mut listing, report) {
        report.problems.push(IndexProblem::new(root, &error));
        return Ok(Vec::new());
    }
    let found = listing.found;

    let store = SyncStore::new(root, &config.store_folder);
    for error in store.remove_conflict_copies() {
        report.problems.push(IndexProblem::new(root, &error));
    }

    let mut written = Vec::new();

    let stored = stored_notes(index, source.id)?;
    for note in &found {
        let stored = stored.get(&note.relative);
        let mtime = match modified(&note.path) {
            Ok(mtime) => mtime,
            Err(error) => {
                report.problems.push(IndexProblem::new(&note.path, &error));
                continue;
            }
        };
        if mode == IndexMode::Changed && stored.is_some_and(|stored| stored.mtime == mtime) {
            continue;
        }

        let read = match read_note(&note.path, mtime) {
            Ok(read) => read,
            Err(error) => {
                report.problems.push(IndexProblem::new(&note.path, &error));
                continue;
            }
        };
        if mode == IndexMode::Changed && stored.is_some_and(|stored| stored.hash == read.hash) {
            index::lock(index).connection().execute(
                "UPDATE files SET mtime = ?1 WHERE source_id = ?2 AND path = ?3",
                (mtime, source.id, &note.relative),
            )?;
            continue;
        }

        if let Some(problem) = &read.properties_problem {
            report.problems.push(problem.clone());
        }
        // Re-embedding must not take vectors from the store.
        let stored = (mode != IndexMode::Reembed)
            .then(|| read_vector_file(&store, &note.relative, vector_version, report))
            .flatten();
        written.push(write_note(
            index,
            indexer,
            source.id,
            note,
            &read,
            stored.as_ref(),
        )?);
        report.files += 1;
    }

    let found: HashSet<&str> = found.iter().map(|note| note.relative.as_str()).collect();
    // A note in a folder that couldn't be listed is not gone, so it keeps its index rows and vector file.
    let removed: Vec<&str> = stored
        .keys()
        .map(String::as_str)
        .filter(|path| {
            !found.contains(path) && !listing.unlisted.iter().any(|folder| is_in(path, folder))
        })
        .collect();
    {
        let removed = serde_json::to_string(&removed).expect("a list of strings serialises");
        index::lock(index).connection().execute(
            "DELETE FROM files WHERE source_id = ?1 AND path IN (SELECT value FROM json_each(?2))",
            (source.id, removed),
        )?;
    }
    for relative in &removed {
        if let Err(error) = store.remove(relative) {
            report
                .problems
                .push(IndexProblem::new(&store.vector_file_path(relative), &error));
        }
    }
    report.removed += removed.len() as u64;

    Ok(written)
}

/// Whether the note at `path` is at or under `folder`, the empty folder being the source's root.
fn is_in(path: &str, folder: &str) -> bool {
    folder.is_empty()
        || path
            .strip_prefix(folder)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// What listing a source found.
#[derive(Default)]
struct Listing {
    found: Vec<Found>,
    /// Folders and entries that could not be listed in full, relative to the source.
    unlisted: Vec<String>,
}

fn stored_notes(
    index: &Mutex<Index>,
    source_id: i64,
) -> Result<HashMap<String, Stored>, IndexerError> {
    let index = index::lock(index);
    let mut statement = index
        .connection()
        .prepare("SELECT path, mtime, hash FROM files WHERE source_id = ?1")?;
    let stored = statement
        .query_map([source_id], |row| {
            Ok((
                row.get(0)?,
                Stored {
                    mtime: row.get(1)?,
                    hash: row.get(2)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;

    Ok(stored)
}

/// Collects the notes under `folder`, skipping hidden entries (`.obsidian`, `.trash`, as Obsidian does), the store
/// folder and symbolic links. Fails only when `folder` itself can't be listed; a subfolder or an entry that can't be is
/// reported and recorded as unlisted.
fn find_notes(
    folder: &Path,
    relative: &str,
    store_folder: &str,
    listing: &mut Listing,
    report: &mut IndexReport,
) -> io::Result<()> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(folder)? {
        match entry {
            Ok(entry) => entries.push(entry),
            Err(error) => {
                report.problems.push(IndexProblem::new(folder, &error));
                listing.unlisted.push(relative.to_owned());
            }
        }
    }
    entries.sort_by_key(fs::DirEntry::file_name);

    for entry in entries {
        let path = entry.path();
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            report.problems.push(IndexProblem {
                path: path.display().to_string(),
                message: "the name is not valid Unicode".to_owned(),
            });
            continue;
        };
        if name.starts_with('.') || name == store_folder {
            continue;
        }

        let child = if relative.is_empty() {
            name.clone()
        } else {
            format!("{relative}/{name}")
        };
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                report.problems.push(IndexProblem::new(&path, &error));
                listing.unlisted.push(child);
                continue;
            }
        };

        if file_type.is_dir() {
            if let Err(error) = find_notes(&path, &child, store_folder, listing, report) {
                report.problems.push(IndexProblem::new(&path, &error));
                listing.unlisted.push(child);
            }
        } else if file_type.is_file()
            && Path::new(&name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case(NOTE_EXTENSION))
        {
            listing.found.push(Found {
                path,
                relative: child,
            });
        }
    }

    Ok(())
}

struct ReadNote {
    text: String,
    title: String,
    mtime: i64,
    hash: Vec<u8>,
    properties: String,
    properties_problem: Option<IndexProblem>,
}

#[derive(Debug, thiserror::Error)]
enum ReadError {
    #[error("failed to read the note")]
    Io(#[from] io::Error),
    #[error("the note is not valid UTF-8")]
    NotUtf8,
    #[error("the note's modification time is before 1970")]
    Mtime,
}

/// The note's modification time in nanoseconds since the Unix epoch.
fn modified(path: &Path) -> Result<i64, ReadError> {
    fs::metadata(path)?
        .modified()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_nanos()).ok())
        .ok_or(ReadError::Mtime)
}

/// Reads a note whose modification time was `mtime` just before. Taking it first means a write in between leaves an
/// older time behind, so the next run reads the note again.
fn read_note(path: &Path, mtime: i64) -> Result<ReadNote, ReadError> {
    let bytes = fs::read(path)?;
    let hash = Sha256::digest(&bytes).to_vec();
    let text = String::from_utf8(bytes).map_err(|_| ReadError::NotUtf8)?;
    let title = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .expect("notes are found by their Unicode name")
        .to_owned();

    // A note with broken frontmatter is still worth finding, so it is indexed without properties.
    let (properties, properties_problem) = match properties(&text) {
        Ok(properties) => (properties, None),
        Err(error) => (Default::default(), Some(IndexProblem::new(path, &error))),
    };

    Ok(ReadNote {
        title,
        mtime,
        hash,
        properties: serde_json::Value::Object(properties).to_string(),
        properties_problem,
        text,
    })
}

fn write_note(
    index: &Mutex<Index>,
    indexer: &Indexer,
    source_id: i64,
    note: &Found,
    read: &ReadNote,
    stored: Option<&VectorFile>,
) -> Result<i64, IndexerError> {
    let mut index = index::lock(index);
    let tx = index.transaction()?;
    let id: i64 = tx.query_row(
        "INSERT INTO files (source_id, path, mtime, hash, title, properties)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (source_id, path) DO UPDATE SET
             mtime = excluded.mtime,
             hash = excluded.hash,
             title = excluded.title,
             properties = excluded.properties
         RETURNING id",
        (
            source_id,
            &note.relative,
            read.mtime,
            &read.hash,
            &read.title,
            &read.properties,
        ),
        |row| row.get(0),
    )?;

    // A changed note still reuses the stored vectors of its unchanged chunk texts.
    if let Some(stored) = stored {
        import_vectors(&tx, stored)?;
    }
    indexer.process(
        &tx,
        &NoteFile {
            id,
            title: &read.title,
            text: &read.text,
            stored_chunks: stored.and_then(|stored| chunks_for(stored, &read.text, &read.hash)),
        },
    )?;
    tx.commit()?;

    Ok(id)
}
