use std::{
    fs::File,
    io::{self, BufWriter, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use half::f16;
use memmap2::Mmap;
use rusqlite::{Connection, Row};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use thiserror::Error;
use zerocopy::{FromBytes, IntoBytes};

/// The vector scan file in the data folder: a copy of every embedded chunk's vector, laid out for search to scan.
pub const VECTOR_SCAN_FILE: &str = "vectors.scan";

const MAGIC: &[u8; 8] = b"TROVEVEC";
const FORMAT: u32 = 1;
/// One signed byte per value and one f32 scale per vector: half the bytes of the index's fp16 to scan. On the eval
/// vault it ranks as fp16 does, but for four answers that move one place.
const INT8: u32 = 2;
const VECTOR_VERSION_LEN: usize = 64;
const HEADER_LEN: usize = 128;

/// Separate running sums, so the compiler can keep them in vector registers.
const LANES: usize = 32;

const _: () = assert!(
    cfg!(target_endian = "little"),
    "the vector scan file is little-endian and read in place"
);

/// The rows a scan file holds, in its order. The vector's rowid changes whenever the vector is written again.
const ROWS: &str = "SELECT c.id, c.text_hash, v.rowid, v.vector
                    FROM chunks c JOIN vectors v USING (text_hash)
                    ORDER BY c.id";
const FINGERPRINT_ROWS: &str = "SELECT c.id, c.text_hash, v.rowid
                                FROM chunks c JOIN vectors v USING (text_hash)
                                ORDER BY c.id";

#[derive(Debug, Error)]
pub enum VectorScanError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error("failed to read the vector scan file {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to write the vector scan file {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "chunk {chunk_id} has a stored vector of {found} bytes, unlike the {expected} bytes of the others"
    )]
    UnevenVectors {
        chunk_id: i64,
        found: usize,
        expected: usize,
    },
    #[error("the stored vectors have {stored} dimensions, but the query has {query}")]
    WrongLength { stored: usize, query: usize },
}

/// The header: what the rows are and which index state they were copied from. Little-endian, padded to `HEADER_LEN`:
/// magic, format, precision, dimensions, 4 bytes unused, rows, `vector_version` (zero-padded), fingerprint. Then each
/// row's vector as `i8`s, then each row's scale as an `f32`, then each row's chunk id as an `i64`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Header {
    precision: u32,
    dims: u32,
    rows: u64,
    /// The model identity the vectors were made under; empty before the first index run.
    vector_version: String,
    fingerprint: [u8; 32],
}

impl Header {
    fn to_bytes(&self) -> [u8; HEADER_LEN] {
        let mut bytes = [0; HEADER_LEN];
        bytes[0..8].copy_from_slice(MAGIC);
        bytes[8..12].copy_from_slice(&FORMAT.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.precision.to_le_bytes());
        bytes[16..20].copy_from_slice(&self.dims.to_le_bytes());
        bytes[24..32].copy_from_slice(&self.rows.to_le_bytes());
        bytes[32..32 + self.vector_version.len()].copy_from_slice(self.vector_version.as_bytes());
        bytes[96..128].copy_from_slice(&self.fingerprint);

        bytes
    }

    /// `None` for anything but a whole file in this format.
    fn parse(file: &[u8]) -> Option<Self> {
        let bytes = file.get(..HEADER_LEN)?;
        let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        if &bytes[0..8] != MAGIC || u32_at(8) != FORMAT || u32_at(12) != INT8 {
            return None;
        }

        let vector_version = &bytes[32..32 + VECTOR_VERSION_LEN];
        let header = Self {
            precision: INT8,
            dims: u32_at(16),
            rows: u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
            vector_version: String::from_utf8(
                vector_version
                    .iter()
                    .copied()
                    .take_while(|&byte| byte != 0)
                    .collect(),
            )
            .ok()?,
            fingerprint: bytes[96..128].try_into().unwrap(),
        };

        (header.file_len()? == file.len() && (header.rows == 0 || header.dims > 0))
            .then_some(header)
    }

    fn vectors_len(&self) -> Option<usize> {
        usize::try_from(self.rows)
            .ok()?
            .checked_mul(self.dims as usize)
    }

    fn scales_len(&self) -> Option<usize> {
        usize::try_from(self.rows)
            .ok()?
            .checked_mul(size_of::<f32>())
    }

    fn file_len(&self) -> Option<usize> {
        let ids_len = usize::try_from(self.rows)
            .ok()?
            .checked_mul(size_of::<i64>())?;

        HEADER_LEN
            .checked_add(self.vectors_len()?)?
            .checked_add(self.scales_len()?)?
            .checked_add(ids_len)
    }
}

/// The vector scan file, memory-mapped: every embedded chunk's vector quantised to int8 in one flat block, one row per
/// chunk in chunk id order, then each vector's scale and the chunk ids. Derived from the index and written again
/// whenever the index's vectors change.
pub(crate) struct VectorScan {
    header: Header,
    map: Mmap,
}

impl VectorScan {
    /// The file at `path`, or `None` when there is none or it isn't a whole file in this format.
    pub(crate) fn open(path: &Path) -> Result<Option<Self>, VectorScanError> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(VectorScanError::Read {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        let map = map(&file).map_err(|source| VectorScanError::Read {
            path: path.to_owned(),
            source,
        })?;

        Ok(Header::parse(&map).map(|header| Self { header, map }))
    }

    /// Writes the file at `path` from the index's vectors, replacing any file there in one step.
    pub(crate) fn build(
        conn: &Connection,
        path: &Path,
        vector_version: &str,
    ) -> Result<Self, VectorScanError> {
        let write_error = |source| VectorScanError::Write {
            path: path.to_owned(),
            source,
        };
        let folder = path.parent().expect("the file is in the data folder");
        let mut file = NamedTempFile::new_in(folder).map_err(write_error)?;
        let mut writer = BufWriter::new(&mut file);
        writer.write_all(&[0; HEADER_LEN]).map_err(write_error)?;

        let mut statement = conn.prepare(ROWS)?;
        let mut rows = statement.query([])?;
        let mut fingerprint = Sha256::new();
        let mut ids = Vec::new();
        let mut scales = Vec::new();
        let mut vector_len = None;
        while let Some(row) = rows.next()? {
            let chunk_id = add_to_fingerprint(&mut fingerprint, row)?;
            let vector = row.get_ref(3)?.as_blob().map_err(rusqlite::Error::from)?;
            let expected = *vector_len.get_or_insert(vector.len());
            if vector.len() != expected || vector.is_empty() || vector.len() % 2 != 0 {
                return Err(VectorScanError::UnevenVectors {
                    chunk_id,
                    found: vector.len(),
                    expected,
                });
            }

            let values: Vec<f32> = vector
                .chunks_exact(size_of::<f16>())
                .map(|bytes| f16::from_le_bytes([bytes[0], bytes[1]]).to_f32())
                .collect();
            let (scale, quantised) = quantised(&values);
            writer
                .write_all(quantised.as_bytes())
                .map_err(write_error)?;
            scales.push(scale);
            ids.push(chunk_id);
        }
        for scale in &scales {
            writer
                .write_all(&scale.to_le_bytes())
                .map_err(write_error)?;
        }
        for id in &ids {
            writer.write_all(&id.to_le_bytes()).map_err(write_error)?;
        }

        let header = Header {
            precision: INT8,
            dims: (vector_len.unwrap_or(0) / size_of::<f16>()) as u32,
            rows: ids.len() as u64,
            vector_version: vector_version.to_owned(),
            fingerprint: fingerprint.finalize().into(),
        };
        writer.seek(SeekFrom::Start(0)).map_err(write_error)?;
        writer.write_all(&header.to_bytes()).map_err(write_error)?;
        writer.flush().map_err(write_error)?;
        drop(writer);

        let file = file
            .persist(path)
            .map_err(|error| write_error(error.error))?;
        let map = map(&file).map_err(|source| VectorScanError::Read {
            path: path.to_owned(),
            source,
        })?;

        Ok(Self { header, map })
    }

    /// Whether this file holds the index's vectors as they are now.
    pub(crate) fn is_current(&self, vector_version: &str, fingerprint: &[u8; 32]) -> bool {
        self.header.vector_version == vector_version && &self.header.fingerprint == fingerprint
    }

    /// The vectors of `rows` (positions in [`Self::scores`]), back from int8: each about the vector the index holds.
    pub(crate) fn vectors(&self, rows: &[usize]) -> Vec<Vec<f32>> {
        let dims = self.header.dims as usize;
        let (vectors, scales, _) = self.sections();

        rows.iter()
            .map(|&row| {
                let at = row * size_of::<f32>();
                let scale =
                    f32::from_le_bytes(scales[at..at + size_of::<f32>()].try_into().unwrap());
                vectors[row * dims..(row + 1) * dims]
                    .iter()
                    .map(|&value| f32::from(value) * scale)
                    .collect()
            })
            .collect()
    }

    /// Each row's chunk id and dot product with `query`, quantised as the rows are: about the cosine similarity, both
    /// being unit vectors.
    pub(crate) fn scores(&self, query: &[f32]) -> Result<Vec<(i64, f32)>, VectorScanError> {
        if self.header.rows == 0 {
            return Ok(Vec::new());
        }
        let dims = self.header.dims as usize;
        if query.len() != dims {
            return Err(VectorScanError::WrongLength {
                stored: dims,
                query: query.len(),
            });
        }
        let (query_scale, query) = quantised(query);
        let (vectors, scales, ids) = self.sections();

        Ok(ids
            .chunks_exact(size_of::<i64>())
            .zip(scales.chunks_exact(size_of::<f32>()))
            .zip(vectors.chunks_exact(dims))
            .map(|((id, scale), vector)| {
                let scale = f32::from_le_bytes(scale.try_into().unwrap()) * query_scale;
                (
                    i64::from_le_bytes(id.try_into().unwrap()),
                    dot(vector, &query) as f32 * scale,
                )
            })
            .collect())
    }

    /// The file after its header: the vectors, the scales' bytes and the chunk ids' bytes.
    fn sections(&self) -> (&[i8], &[u8], &[u8]) {
        let checked = "checked when the file was read";
        let (vectors, rest) =
            self.map[HEADER_LEN..].split_at(self.header.vectors_len().expect(checked));
        let (scales, ids) = rest.split_at(self.header.scales_len().expect(checked));

        (
            <[i8]>::ref_from_bytes(vectors).expect("any bytes are i8s"),
            scales,
            ids,
        )
    }
}

/// Identifies the rows a scan file of the index would hold now, without reading any vector.
pub(crate) fn fingerprint(conn: &Connection) -> rusqlite::Result<[u8; 32]> {
    let mut statement = conn.prepare(FINGERPRINT_ROWS)?;
    let mut rows = statement.query([])?;
    let mut fingerprint = Sha256::new();
    while let Some(row) = rows.next()? {
        add_to_fingerprint(&mut fingerprint, row)?;
    }

    Ok(fingerprint.finalize().into())
}

/// Adds a row's chunk id, text hash and vector rowid, and returns the chunk id.
fn add_to_fingerprint(fingerprint: &mut Sha256, row: &Row<'_>) -> rusqlite::Result<i64> {
    let chunk_id: i64 = row.get(0)?;
    let vector_rowid: i64 = row.get(2)?;
    fingerprint.update(chunk_id.to_le_bytes());
    fingerprint.update(row.get_ref(1)?.as_blob()?);
    fingerprint.update(vector_rowid.to_le_bytes());

    Ok(chunk_id)
}

fn map(file: &File) -> io::Result<Mmap> {
    // SAFETY: the file is Trove's own, in the data folder. It is written whole under a temporary name and renamed into
    // place, never changed in place, so the mapped bytes don't change while they are read.
    #[allow(unsafe_code)]
    unsafe {
        Mmap::map(file)
    }
}

/// `values` as int8 with one scale for all of them: each value is about the scale times its int8, the largest ±127.
fn quantised(values: &[f32]) -> (f32, Vec<i8>) {
    let largest = values
        .iter()
        .fold(0.0f32, |largest, value| largest.max(value.abs()));
    if largest == 0.0 {
        return (0.0, vec![0; values.len()]);
    }

    let scale = largest / f32::from(i8::MAX);
    (
        scale,
        values
            .iter()
            .map(|value| (value / scale).round() as i8)
            .collect(),
    )
}

/// The integer dot product. Each product is at most 127², so an `i32` holds the sum of far more dimensions than a
/// model has.
fn dot(stored: &[i8], query: &[i8]) -> i32 {
    let (stored, stored_rest) = stored.as_chunks::<LANES>();
    let (query, query_rest) = query.as_chunks::<LANES>();
    let mut sums = [0i32; LANES];
    for (stored, query) in stored.iter().zip(query) {
        for ((sum, &stored), &value) in sums.iter_mut().zip(stored).zip(query) {
            *sum += i32::from(stored) * i32::from(value);
        }
    }

    sums.iter().sum::<i32>()
        + stored_rest
            .iter()
            .zip(query_rest)
            .map(|(&stored, &value)| i32::from(stored) * i32::from(value))
            .sum::<i32>()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::{data_dir::DataDir, index::Index};

    const DIMS: usize = 768;

    /// The unit vector along `axis`, as the index stores it.
    fn stored_axis(axis: usize) -> Vec<u8> {
        (0..DIMS)
            .flat_map(|at| f16::from_f32(if at == axis { 1.0 } else { 0.0 }).to_le_bytes())
            .collect()
    }

    #[test]
    fn a_file_of_another_precision_or_dimensions_is_written_again() {
        let temp = tempfile::tempdir().unwrap();
        let data_dir = DataDir::at(temp.path().to_owned()).unwrap();
        let mut index = Index::open(&data_dir).unwrap();
        index
            .connection()
            .execute_batch(
                "INSERT INTO sources (id, path) VALUES (1, '/vault');
                 INSERT INTO files (id, source_id, path, mtime, hash, title, properties)
                 VALUES (1, 1, 'Bread.md', 0, x'01', 'Bread', '{}');
                 INSERT INTO chunks (id, file_id, ordinal, heading_path, span_start, span_end, text_hash, has_words)
                 VALUES (1, 1, 0, '', 0, 9, x'01', 1), (2, 1, 1, '', 9, 18, x'02', 1);",
            )
            .unwrap();
        for (text_hash, axis) in [([1u8], 0), ([2u8], 1)] {
            index
                .connection()
                .execute(
                    "INSERT INTO vectors (text_hash, vector) VALUES (?1, ?2)",
                    (&text_hash[..], stored_axis(axis)),
                )
                .unwrap();
        }

        let mut query = vec![0.0; DIMS];
        query[0] = 1.0;
        let scores = index.vector_scan().unwrap().scores(&query).unwrap();
        assert_eq!(scores, [(1, 1.0), (2, 0.0)]);
        drop(index);

        let path = data_dir.path().join(VECTOR_SCAN_FILE);
        let written = fs::read(&path).unwrap();
        let header = Header::parse(&written).unwrap();
        let stale_headers = [
            Header {
                precision: INT8 + 1,
                ..header.clone()
            },
            Header {
                dims: header.dims / 2,
                ..header
            },
        ];
        for stale in stale_headers {
            let mut file = written.clone();
            file[..HEADER_LEN].copy_from_slice(&stale.to_bytes());
            fs::write(&path, &file).unwrap();

            let mut index = Index::open(&data_dir).unwrap();
            assert_eq!(index.vector_scan().unwrap().scores(&query).unwrap(), scores);
            assert_eq!(fs::read(&path).unwrap(), written);
        }
    }
}
