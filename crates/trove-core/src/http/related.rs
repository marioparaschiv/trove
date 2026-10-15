use std::sync::Arc;

use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
    http::StatusCode,
};

use super::{
    AppState,
    error::{ApiError, ErrorBody},
    openapi::Rejected,
};
use crate::{
    related::{self, RelatedError, RelatedRequest},
    search::SearchResponse,
};

impl From<RelatedError> for ApiError {
    fn from(error: RelatedError) -> Self {
        let status = match error {
            RelatedError::NotIndexed { .. } => StatusCode::NOT_FOUND,
            RelatedError::ZeroLimit => StatusCode::BAD_REQUEST,
            RelatedError::Index(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        Self::from_error(status, &error)
    }
}

/// The notes closest to a note in the graph of links and meaning, each pointing at its best section.
#[utoipa::path(
    get,
    path = "/related",
    params(RelatedRequest),
    responses(
        (status = OK, description = "The related notes, best first", body = SearchResponse),
        (status = BAD_REQUEST, description = "The request is invalid", body = ErrorBody),
        (status = NOT_FOUND, description = "The note is not indexed", body = ErrorBody),
        (status = INTERNAL_SERVER_ERROR, description = "Reading the index failed", body = ErrorBody),
        Rejected,
    ),
)]
pub(super) async fn related(
    State(state): State<Arc<AppState>>,
    request: Result<Query<RelatedRequest>, QueryRejection>,
) -> Result<Json<SearchResponse>, ApiError> {
    let Query(request) = request?;
    let response = tokio::task::spawn_blocking(move || related::related(&state.index, &request))
        .await
        .map_err(|error| ApiError::internal(&error))??;

    Ok(Json(response))
}
