//! The sync store: a folder inside each source holding one vector file per note, so whatever syncs the vault also
//! syncs its vectors and another device can rebuild its index without the model.
//!
//! A vector file is `<store>/vectors/<first two hex>/<SHA-256 of the note's path>.vec`, all integers little-endian:
//! magic `TROVEVEC`, format version (u32), `vector_version` (64 ASCII hex bytes), the note's content hash (32 bytes),
//! dims (u32) and chunk count (u32), then per chunk: text hash (32 bytes), heading path length (u32) and UTF-8 bytes,
//! span start and end (u64 each), and the vector as `dims` fp16 values.

mod decisions;

use std::{
    fs, io,
    io::Write,
    ops::Range,
    path::{Component, Path, PathBuf},
};

use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use thiserror::Error;

pub use decisions::{CategoryPin, Decision, DecisionError, SuggestionState, note_pair};

const VECTORS_FOLDER: &str = "vectors";
const EXTENSION: &str = "vec";
const MAGIC: &[u8; 8] = b"TROVEVEC";
const FORMAT_VERSION: u32 = 1;
const HASH_LEN: usize = 32;
const VECTOR_VERSION_LEN: usize = 64;

/// The store folder of one source.
pub struct SyncStore {
    root: PathBuf,
}

/// Everything the store holds for one note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorFile {
    /// SHA-256 of the note's bytes when it was chunked.
    pub content_hash: Vec<u8>,
    pub dims: u32,
    /// In note order.
    pub chunks: Vec<StoredChunk>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredChunk {
    pub text_hash: Vec<u8>,
    /// Headings joined by `\n`, as in the index.
    pub heading_path: String,
    /// Byte offsets of the chunk text in the note.
    pub span: Range<usize>,
    /// Little-endian fp16, `dims` values.
    pub vector: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("failed to read {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to write {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to remove {path}")]
    Remove {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path} is not a valid vector file: {reason}")]
    Malformed { path: PathBuf, reason: &'static str },
    #[error("line {line} of {path} is not a valid decision")]
    MalformedDecision {
        path: PathBuf,
        line: usize,
        #[source]
        source: DecisionError,
    },
}

impl SyncStore {
    pub fn new(source: &Path, store_folder: &str) -> Self {
        Self {
            root: source.join(store_folder),
        }
    }

    /// The file holding the vectors of the note at `relative`, the note's `/`-separated path inside the source.
    pub fn vector_file_path(&self, relative: &str) -> PathBuf {
        let name = hex::encode(Sha256::digest(relative.as_bytes()));
        self.root
            .join(VECTORS_FOLDER)
            .join(&name[..2])
            .join(format!("{name}.{EXTENSION}"))
    }

    /// The note's vector file, or `None` when there is none or it was written under another format or
    /// `vector_version`.
    pub fn read(
        &self,
        relative: &str,
        vector_version: &str,
    ) -> Result<Option<VectorFile>, StoreError> {
        let path = self.vector_file_path(relative);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(StoreError::Read { path, source }),
        };

        decode(&bytes, vector_version).map_err(|reason| StoreError::Malformed { path, reason })
    }

    /// Writes the note's vector file unless it already holds these bytes. The file appears whole or not at all.
    pub fn write(
        &self,
        relative: &str,
        vector_version: &str,
        file: &VectorFile,
    ) -> Result<(), StoreError> {
        let path = self.vector_file_path(relative);
        let bytes = encode(vector_version, file);
        // Rewriting identical bytes would still look like a change to sync tools.
        match fs::read(&path) {
            Ok(existing) if existing == bytes => return Ok(()),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => return Err(StoreError::Read { path, source }),
        }

        write_atomically(&path, &bytes)
    }

    /// Removes the note's vector file if there is one.
    pub fn remove(&self, relative: &str) -> Result<(), StoreError> {
        let path = self.vector_file_path(relative);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(StoreError::Remove { path, source }),
        }
    }

    /// Removes the conflict copies sync tools leave anywhere in the store folder. Everything in them can be rebuilt.
    pub fn remove_conflict_copies(&self) -> Vec<StoreError> {
        let mut errors = Vec::new();
        remove_conflict_copies_in(&self.root, &mut errors);
        errors
    }
}

/// Writes `bytes` to `path` through a temporary file in the same folder, so the file appears whole or not at all.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let write_error = |source| StoreError::Write {
        path: path.to_path_buf(),
        source,
    };
    let folder = path.parent().expect("store files are inside a folder");
    fs::create_dir_all(folder).map_err(write_error)?;
    let mut temporary = NamedTempFile::new_in(folder).map_err(write_error)?;
    temporary.write_all(bytes).map_err(write_error)?;
    temporary.flush().map_err(write_error)?;
    temporary
        .persist(path)
        .map_err(|error| write_error(error.error))?;

    Ok(())
}

fn remove_conflict_copies_in(folder: &Path, errors: &mut Vec<StoreError>) {
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return,
        Err(source) => {
            errors.push(StoreError::Read {
                path: folder.to_path_buf(),
                source,
            });
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(source) => {
                errors.push(StoreError::Read {
                    path: folder.to_path_buf(),
                    source,
                });
                continue;
            }
        };
        let path = entry.path();
        match entry.file_type() {
            Ok(file_type) if file_type.is_dir() => remove_conflict_copies_in(&path, errors),
            Ok(file_type) if file_type.is_file() && is_conflict_copy(&entry.file_name()) => {
                if let Err(source) = fs::remove_file(&path) {
                    errors.push(StoreError::Remove { path, source });
                }
            }
            Ok(_) => {}
            Err(source) => errors.push(StoreError::Read { path, source }),
        }
    }
}

/// The name a vector file has for its note (the hex SHA-256 of the note's path) when `path`, relative to a source, is
/// one.
pub fn vector_file_name<'a>(path: &'a Path, store_folder: &str) -> Option<&'a str> {
    let mut parts = path.components().map(|component| match component {
        Component::Normal(part) => part.to_str(),
        _ => None,
    });
    let (Some(Some(store)), Some(Some(VECTORS_FOLDER)), Some(Some(shard)), Some(Some(file)), None) = (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) else {
        return None;
    };
    let name = file.strip_suffix(EXTENSION)?.strip_suffix('.')?;
    let is_hash = name.len() == 2 * HASH_LEN
        && name
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));

    (store == store_folder && is_hash && name.starts_with(shard) && shard.len() == 2)
        .then_some(name)
}

/// Syncthing names a conflict copy `name.sync-conflict-<date>-<time>-<device>.ext`, Dropbox
/// `name (<device>'s conflicted copy <date>).ext`.
fn is_conflict_copy(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy();
    name.contains(".sync-conflict-") || name.contains("conflicted copy")
}

fn encode(vector_version: &str, file: &VectorFile) -> Vec<u8> {
    assert_eq!(
        vector_version.len(),
        VECTOR_VERSION_LEN,
        "vector_version is a SHA-256 in hex"
    );

    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(vector_version.as_bytes());
    bytes.extend_from_slice(&file.content_hash);
    bytes.extend_from_slice(&file.dims.to_le_bytes());
    bytes.extend_from_slice(&len_u32(file.chunks.len()).to_le_bytes());

    for chunk in &file.chunks {
        bytes.extend_from_slice(&chunk.text_hash);
        bytes.extend_from_slice(&len_u32(chunk.heading_path.len()).to_le_bytes());
        bytes.extend_from_slice(chunk.heading_path.as_bytes());
        bytes.extend_from_slice(&(chunk.span.start as u64).to_le_bytes());
        bytes.extend_from_slice(&(chunk.span.end as u64).to_le_bytes());
        bytes.extend_from_slice(&chunk.vector);
    }

    bytes
}

fn len_u32(len: usize) -> u32 {
    u32::try_from(len).expect("notes and their chunks are far smaller than 4 GiB")
}

fn decode(bytes: &[u8], vector_version: &str) -> Result<Option<VectorFile>, &'static str> {
    let mut reader = Reader { bytes };
    if reader.take(MAGIC.len())? != MAGIC {
        return Err("wrong magic");
    }
    if reader.u32()? != FORMAT_VERSION
        || reader.take(VECTOR_VERSION_LEN)? != vector_version.as_bytes()
    {
        return Ok(None);
    }

    let content_hash = reader.take(HASH_LEN)?.to_vec();
    let dims = reader.u32()?;
    let count = reader.u32()?;
    let mut chunks = Vec::new();
    for _ in 0..count {
        let text_hash = reader.take(HASH_LEN)?.to_vec();
        let heading_len = reader.u32()? as usize;
        let heading_path = std::str::from_utf8(reader.take(heading_len)?)
            .map_err(|_| "a heading path is not UTF-8")?
            .to_owned();
        let start = reader.usize()?;
        let end = reader.usize()?;
        if start > end {
            return Err("a span ends before it starts");
        }
        let vector = reader.take(dims as usize * 2)?.to_vec();

        chunks.push(StoredChunk {
            text_hash,
            heading_path,
            span: start..end,
            vector,
        });
    }

    if !reader.bytes.is_empty() {
        return Err("trailing bytes");
    }

    Ok(Some(VectorFile {
        content_hash,
        dims,
        chunks,
    }))
}

struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], &'static str> {
        if self.bytes.len() < len {
            return Err("cut short");
        }

        let (taken, rest) = self.bytes.split_at(len);
        self.bytes = rest;
        Ok(taken)
    }

    fn u32(&mut self) -> Result<u32, &'static str> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes(
            bytes.try_into().expect("took four bytes"),
        ))
    }

    fn usize(&mut self) -> Result<usize, &'static str> {
        let bytes = self.take(8)?;
        usize::try_from(u64::from_le_bytes(
            bytes.try_into().expect("took eight bytes"),
        ))
        .map_err(|_| "a span is out of range")
    }
}
