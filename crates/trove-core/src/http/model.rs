use std::sync::Arc;

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{
    AppState,
    error::{ApiError, ErrorBody},
    openapi::Rejected,
};

/// Whether the embedding model is in memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ModelStatus {
    pub loaded: bool,
}

/// Loads the model now, so the first search doesn't wait for it. Returns once it is loaded.
#[utoipa::path(
    post,
    path = "/prewarm",
    responses(
        (status = OK, description = "The model is loaded", body = ModelStatus),
        (status = INTERNAL_SERVER_ERROR, description = "Loading the model failed", body = ErrorBody),
        Rejected,
    ),
)]
pub(super) async fn prewarm(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ModelStatus>, ApiError> {
    let embedder = Arc::clone(&state.embedder);
    tokio::task::spawn_blocking(move || embedder.load())
        .await
        .map_err(|error| ApiError::internal(&error))?
        .map_err(|error| ApiError::internal(&error))?;

    Ok(Json(ModelStatus { loaded: true }))
}

/// Frees the model's memory now; the next search or index run loads it again.
#[utoipa::path(
    post,
    path = "/unload",
    responses(
        (status = OK, description = "The model is unloaded", body = ModelStatus),
        (status = INTERNAL_SERVER_ERROR, description = "Unloading failed", body = ErrorBody),
        Rejected,
    ),
)]
pub(super) async fn unload(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ModelStatus>, ApiError> {
    let embedder = Arc::clone(&state.embedder);
    // Waits for an embedding in progress to finish.
    tokio::task::spawn_blocking(move || embedder.unload())
        .await
        .map_err(|error| ApiError::internal(&error))?;

    Ok(Json(ModelStatus { loaded: false }))
}
