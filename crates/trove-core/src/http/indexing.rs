use std::sync::Arc;

use axum::{Json, extract::State};

use super::{
    AppState,
    error::{ApiError, ErrorBody},
    openapi::Rejected,
};
use crate::indexer::{self, IndexReport};

/// Indexes every source now and returns when done: reads and chunks the notes, then embeds new chunk texts.
#[utoipa::path(
    post,
    path = "/index",
    responses(
        (status = OK, description = "What the run did", body = IndexReport),
        (status = INTERNAL_SERVER_ERROR, description = "Indexing failed", body = ErrorBody),
        Rejected,
    ),
)]
pub(super) async fn index(
    State(state): State<Arc<AppState>>,
) -> Result<Json<IndexReport>, ApiError> {
    let report = tokio::task::spawn_blocking(move || {
        indexer::index_sources(&state.index, &state.embedder, &state.config)
    })
    .await
    .map_err(|error| ApiError::internal(&error))?
    .map_err(|error| ApiError::internal(&error))?;

    Ok(Json(report))
}
