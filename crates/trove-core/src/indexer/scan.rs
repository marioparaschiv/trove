use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;

use super::{Indexer, IndexerError, NoteFile, embed::embed_missing, frontmatter::properties};
use crate::{
    config::Config,
    embedder::{Embedder, vector_version},
    error::full_message,
    index::{self, Index, Source},
};

const NOTE_EXTENSION: &str = "md";

/// What an index run did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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
    fn new(path: &Path, error: &dyn std::error::Error) -> Self {
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

/// Reads and chunks every note in every source, drops notes that are gone, then embeds the chunk texts that have no
/// vector. The index is locked one note or one batch at a time, so searches go on meanwhile.
pub fn index_sources(
    index: &Mutex<Index>,
    embedder: &Arc<Embedder>,
    config: &Config,
) -> Result<IndexReport, IndexerError> {
    let sources = {
        let mut index = index::lock(index);
        index.ensure_vector_version(&vector_version(&config.model.sha256))?;
        index.sources()?
    };

    let indexer = Indexer::standard(Arc::clone(embedder));
    let mut report = IndexReport {
        files: 0,
        removed: 0,
        embedded: 0,
        problems: Vec::new(),
    };

    for source in &sources {
        index_source(index, &indexer, source, config, &mut report)?;
    }
    report.embedded = embed_missing(index, embedder)?;

    Ok(report)
}

fn index_source(
    index: &Mutex<Index>,
    indexer: &Indexer,
    source: &Source,
    config: &Config,
    report: &mut IndexReport,
) -> Result<(), IndexerError> {
    let root = Path::new(&source.path);
    let mut found = Vec::new();
    // A source that can't be read now (an unplugged drive) keeps what was indexed from it.
    if let Err(error) = find_notes(root, "", &config.store_folder, &mut found, report) {
        report.problems.push(IndexProblem::new(root, &error));
        return Ok(());
    }

    for note in &found {
        match read_note(&note.path) {
            Ok(read) => {
                if let Some(problem) = &read.properties_problem {
                    report.problems.push(problem.clone());
                }
                write_note(index, indexer, source.id, note, &read)?;
                report.files += 1;
            }
            Err(error) => report.problems.push(IndexProblem::new(&note.path, &error)),
        }
    }

    let paths: Vec<&str> = found.iter().map(|note| note.relative.as_str()).collect();
    let paths = serde_json::to_string(&paths).expect("a list of strings serialises");
    let removed = index::lock(index).connection().execute(
        "DELETE FROM files WHERE source_id = ?1 AND path NOT IN (SELECT value FROM json_each(?2))",
        (source.id, paths),
    )?;
    report.removed += removed as u64;

    Ok(())
}

/// Collects the notes under `folder`, skipping hidden entries (`.obsidian`, `.trash`, as Obsidian does), the store
/// folder and symbolic links. Fails only when `folder` itself can't be listed; a subfolder that can't be is reported.
fn find_notes(
    folder: &Path,
    relative: &str,
    store_folder: &str,
    found: &mut Vec<Found>,
    report: &mut IndexReport,
) -> io::Result<()> {
    let mut entries = fs::read_dir(folder)?.collect::<io::Result<Vec<_>>>()?;
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
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            if let Err(error) = find_notes(&path, &child, store_folder, found, report) {
                report.problems.push(IndexProblem::new(&path, &error));
            }
        } else if file_type.is_file()
            && Path::new(&name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case(NOTE_EXTENSION))
        {
            found.push(Found {
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

fn read_note(path: &Path) -> Result<ReadNote, ReadError> {
    let mtime = fs::metadata(path)?
        .modified()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_nanos()).ok())
        .ok_or(ReadError::Mtime)?;
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
) -> Result<(), IndexerError> {
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

    indexer.process(
        &tx,
        &NoteFile {
            id,
            title: &read.title,
            text: &read.text,
        },
    )?;

    Ok(tx.commit()?)
}
