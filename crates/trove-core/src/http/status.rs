use std::sync::Arc;

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{
    AppState,
    error::{ApiError, ErrorBody},
    indexing::IndexingStatus,
    model::ModelStatus,
    openapi::Rejected,
};
use crate::index::{self, IndexStats};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Status {
    pub version: String,
    pub pid: u32,
    pub index: IndexStats,
    pub model: ModelStatus,
    pub indexing: IndexingStatus,
}

#[utoipa::path(
    get,
    path = "/status",
    responses(
        (status = OK, description = "The service and index state", body = Status),
        (status = INTERNAL_SERVER_ERROR, description = "Reading the index failed", body = ErrorBody),
        Rejected,
    ),
)]
pub(super) async fn status(State(state): State<Arc<AppState>>) -> Result<Json<Status>, ApiError> {
    let blocking_state = Arc::clone(&state);
    // Both wait while indexing or a search holds the index or the model.
    let (stats, model) = tokio::task::spawn_blocking(move || {
        let stats = index::lock(&blocking_state.index).stats();
        (stats, ModelStatus::of(&blocking_state.embedder))
    })
    .await
    .map_err(|error| ApiError::internal(&error))?;
    let stats = stats.map_err(|error| ApiError::internal(&error))?;

    Ok(Json(Status {
        version: state.runtime.version.clone(),
        pid: state.runtime.pid,
        index: stats,
        model,
        indexing: state.indexing.status(),
    }))
}
