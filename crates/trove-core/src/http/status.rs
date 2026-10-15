use std::sync::{Arc, PoisonError};

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{
    AppState,
    error::{ApiError, ErrorBody},
    openapi::Rejected,
};
use crate::index::IndexStats;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Status {
    pub version: String,
    pub pid: u32,
    pub index: IndexStats,
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
    let index = Arc::clone(&state.index);
    let stats = tokio::task::spawn_blocking(move || {
        // A panic mid-query leaves the connection usable: SQLite rolls back the open transaction.
        index.lock().unwrap_or_else(PoisonError::into_inner).stats()
    })
    .await
    .map_err(|error| ApiError::internal(&error))?
    .map_err(|error| ApiError::internal(&error))?;

    Ok(Json(Status {
        version: state.runtime.version.clone(),
        pid: state.runtime.pid,
        index: stats,
    }))
}
