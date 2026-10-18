use half::f16;
use rusqlite::OptionalExtension;

use super::{
    Index,
    vector_scan::{self, VectorScan, VectorScanError},
};

/// A chunk whose text has no vector yet.
pub(crate) struct UnembeddedChunk {
    pub(crate) text_hash: Vec<u8>,
    pub(crate) note_title: String,
    pub(crate) heading_path: String,
    pub(crate) text: String,
}

pub(crate) struct SectionVector {
    pub(crate) file_id: i64,
    pub(crate) text_hash: Vec<u8>,
    pub(crate) vector: Vec<f32>,
}

impl Index {
    /// Drops every vector when they were made under another `vector_version`, so vectors that don't compare never mix.
    /// Returns whether it did.
    pub(crate) fn ensure_vector_version(&mut self, version: &str) -> rusqlite::Result<bool> {
        let tx = self.conn.transaction()?;
        let stored: Option<String> = tx
            .query_row(
                "SELECT value FROM meta WHERE key = 'vector_version'",
                [],
                |row| row.get(0),
            )
            .optional()?;

        let changed = stored.as_deref() != Some(version);
        if changed {
            tx.execute("DELETE FROM vectors", [])?;
            tx.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('vector_version', ?1)",
                [version],
            )?;
        }
        tx.commit()?;

        Ok(changed)
    }

    /// How many distinct chunk texts with words have no vector.
    pub(crate) fn unembedded_count(&self) -> rusqlite::Result<u64> {
        self.conn.query_row(
            "SELECT count(DISTINCT c.text_hash)
             FROM chunks c
             WHERE c.has_words AND NOT EXISTS (SELECT 1 FROM vectors v WHERE v.text_hash = c.text_hash)",
            [],
            |row| row.get(0),
        )
    }

    /// Up to `limit` chunks whose text has words but no vector, one per distinct text.
    pub(crate) fn unembedded_chunks(&self, limit: usize) -> rusqlite::Result<Vec<UnembeddedChunk>> {
        // With `min`, SQLite takes the bare columns from the row with the lowest id, so the choice is stable.
        let mut statement = self.conn.prepare(
            "SELECT c.text_hash, f.title, c.heading_path, coalesce(t.original, t.text), min(c.id)
             FROM chunks c
             JOIN files f ON f.id = c.file_id
             JOIN chunks_fts t ON t.rowid = c.id
             WHERE c.has_words AND NOT EXISTS (SELECT 1 FROM vectors v WHERE v.text_hash = c.text_hash)
             GROUP BY c.text_hash
             LIMIT ?1",
        )?;

        statement
            .query_map([limit], |row| {
                Ok(UnembeddedChunk {
                    text_hash: row.get(0)?,
                    note_title: row.get(1)?,
                    heading_path: row.get(2)?,
                    text: row.get(3)?,
                })
            })?
            .collect()
    }

    pub(crate) fn insert_vectors<'a>(
        &mut self,
        vectors: impl IntoIterator<Item = (&'a [u8], &'a [f32])>,
    ) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut insert =
                tx.prepare("INSERT OR REPLACE INTO vectors (text_hash, vector) VALUES (?1, ?2)")?;
            for (text_hash, vector) in vectors {
                let bytes: Vec<u8> = vector
                    .iter()
                    .flat_map(|&value| f16::from_f32(value).to_le_bytes())
                    .collect();
                insert.execute((text_hash, bytes))?;
            }
        }

        tx.commit()
    }

    /// Whether any chunk has a vector to search.
    pub(crate) fn has_vectors(&self) -> rusqlite::Result<bool> {
        self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM chunks c JOIN vectors v USING (text_hash))",
            [],
            |row| row.get(0),
        )
    }

    /// Writes the vector scan file again from the index's vectors.
    pub(crate) fn rebuild_vector_scan(&mut self) -> Result<(), VectorScanError> {
        let version = self.version()?;
        let scan = VectorScan::build(&self.conn, &self.vector_scan_path, &self.vector_version()?)?;
        self.vector_scan = Some((version, scan));

        Ok(())
    }

    /// The vector scan file, checked against the index whenever the index has changed or the file is gone, and
    /// written again when it doesn't hold the index's current vectors. Row positions from one call hold only for the
    /// file it returns: another process's index run can replace it before the next.
    pub(crate) fn vector_scan(&mut self) -> Result<&VectorScan, VectorScanError> {
        let version = self.version()?;
        let exists =
            self.vector_scan_path
                .try_exists()
                .map_err(|source| VectorScanError::Read {
                    path: self.vector_scan_path.clone(),
                    source,
                })?;
        let checked = exists
            && self
                .vector_scan
                .as_ref()
                .is_some_and(|(checked_at, _)| *checked_at == version);

        if !checked {
            let vector_version = self.vector_version()?;
            let fingerprint = vector_scan::fingerprint(&self.conn)?;
            let scan = match VectorScan::open(&self.vector_scan_path)? {
                Some(scan) if scan.is_current(&vector_version, &fingerprint) => scan,
                _ => VectorScan::build(&self.conn, &self.vector_scan_path, &vector_version)?,
            };
            self.vector_scan = Some((version, scan));
        }

        let (_, scan) = self.vector_scan.as_ref().expect("checked or just set");
        Ok(scan)
    }

    /// The `vector_version` the stored vectors were made under; empty before the first index run.
    fn vector_version(&self) -> rusqlite::Result<String> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'vector_version'",
                [],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or_default())
    }

    /// Every section with a vector: its note, text hash and vector.
    pub(crate) fn section_vectors(&self) -> rusqlite::Result<Vec<SectionVector>> {
        let mut statement = self.conn.prepare(
            "SELECT DISTINCT c.file_id, c.text_hash, v.vector FROM chunks c JOIN vectors v USING (text_hash)",
        )?;

        statement
            .query_map([], |row| {
                Ok(SectionVector {
                    file_id: row.get(0)?,
                    text_hash: row.get(1)?,
                    vector: values(row.get_ref(2)?.as_blob()?).collect(),
                })
            })?
            .collect()
    }
}

/// A stored vector's values.
fn values(vector: &[u8]) -> impl Iterator<Item = f32> {
    vector
        .chunks_exact(2)
        .map(|bytes| f16::from_le_bytes([bytes[0], bytes[1]]).to_f32())
}
