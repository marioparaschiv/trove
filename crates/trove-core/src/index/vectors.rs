use half::f16;
use rusqlite::{OptionalExtension, types::Type};
use thiserror::Error;

use super::Index;

/// A chunk whose text has no vector yet.
pub(crate) struct UnembeddedChunk {
    pub(crate) text_hash: Vec<u8>,
    pub(crate) note_title: String,
    pub(crate) heading_path: String,
    pub(crate) text: String,
}

#[derive(Debug, Error)]
#[error("a stored vector has {found} bytes, but the query has {expected} dimensions")]
struct WrongLength {
    found: usize,
    expected: usize,
}

impl Index {
    /// Drops every vector when they were made under another `vector_version`, so vectors that don't compare never mix.
    pub(crate) fn ensure_vector_version(&mut self, version: &str) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        let stored: Option<String> = tx
            .query_row(
                "SELECT value FROM meta WHERE key = 'vector_version'",
                [],
                |row| row.get(0),
            )
            .optional()?;

        if stored.as_deref() != Some(version) {
            tx.execute("DELETE FROM vectors", [])?;
            tx.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('vector_version', ?1)",
                [version],
            )?;
        }

        tx.commit()
    }

    /// Up to `limit` chunks whose text has no vector, one per distinct text.
    pub(crate) fn unembedded_chunks(&self, limit: usize) -> rusqlite::Result<Vec<UnembeddedChunk>> {
        // With `min`, SQLite takes the bare columns from the row with the lowest id, so the choice is stable.
        let mut statement = self.conn.prepare(
            "SELECT c.text_hash, f.title, c.heading_path, t.text, min(c.id)
             FROM chunks c
             JOIN files f ON f.id = c.file_id
             JOIN chunks_fts t ON t.rowid = c.id
             WHERE NOT EXISTS (SELECT 1 FROM vectors v WHERE v.text_hash = c.text_hash)
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

    /// Each embedded chunk's cosine similarity to `query`, a unit vector.
    pub(crate) fn chunk_scores(&self, query: &[f32]) -> rusqlite::Result<Vec<(i64, f32)>> {
        let mut statement = self
            .conn
            .prepare("SELECT c.id, v.vector FROM chunks c JOIN vectors v USING (text_hash)")?;

        statement
            .query_map([], |row| {
                let vector = row.get_ref(1)?.as_blob()?;
                if vector.len() != query.len() * 2 {
                    return Err(rusqlite::Error::FromSqlConversionFailure(
                        1,
                        Type::Blob,
                        Box::new(WrongLength {
                            found: vector.len(),
                            expected: query.len(),
                        }),
                    ));
                }

                let score = vector
                    .chunks_exact(2)
                    .zip(query)
                    .map(|(bytes, &value)| {
                        f16::from_le_bytes([bytes[0], bytes[1]]).to_f32() * value
                    })
                    .sum();

                Ok((row.get(0)?, score))
            })?
            .collect()
    }
}
