use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{Index, IndexError};

/// How big the local index is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IndexStats {
    pub files: u64,
    pub chunks: u64,
}

impl Index {
    pub fn stats(&self) -> Result<IndexStats, IndexError> {
        Ok(self.conn.query_row(
            "SELECT (SELECT count(*) FROM files), (SELECT count(*) FROM chunks)",
            [],
            |row| {
                Ok(IndexStats {
                    files: row.get(0)?,
                    chunks: row.get(1)?,
                })
            },
        )?)
    }
}
