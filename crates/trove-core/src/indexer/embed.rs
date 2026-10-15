use std::sync::Mutex;

use super::{IndexerError, document_title};
use crate::{
    embedder::{Embedder, Input},
    index::{self, Index},
};

/// Chunk texts embedded per call. Small enough that a search waiting for the model isn't held up for long.
const BATCH: usize = 16;

/// Embeds every chunk text that has no vector yet, without holding the index while the model runs. Returns how many
/// texts were embedded.
pub(super) fn embed_missing(
    index: &Mutex<Index>,
    embedder: &Embedder,
) -> Result<u64, IndexerError> {
    let mut embedded = 0;
    loop {
        let chunks = index::lock(index).unembedded_chunks(BATCH)?;
        if chunks.is_empty() {
            return Ok(embedded);
        }

        let titles: Vec<String> = chunks
            .iter()
            .map(|chunk| document_title(&chunk.note_title, &chunk.heading_path))
            .collect();
        let inputs: Vec<Input<'_>> = chunks
            .iter()
            .zip(&titles)
            .map(|(chunk, title)| Input::Document {
                title,
                text: &chunk.text,
            })
            .collect();
        let vectors = embedder.embed(&inputs)?;

        index::lock(index).insert_vectors(
            chunks
                .iter()
                .zip(&vectors)
                .map(|(chunk, vector)| (chunk.text_hash.as_slice(), vector.as_slice())),
        )?;
        embedded += chunks.len() as u64;
    }
}
