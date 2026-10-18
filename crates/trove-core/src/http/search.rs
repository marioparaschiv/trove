use std::sync::Arc;

use axum::{Json, extract::State, extract::rejection::JsonRejection, http::StatusCode};

use super::{
    AppState,
    error::{ApiError, ErrorBody, blocking},
    openapi::Rejected,
};
use crate::search::{self, SearchError, SearchRequest, SearchResponse};

impl From<SearchError> for ApiError {
    fn from(error: SearchError) -> Self {
        let status = match error {
            SearchError::EmptyQuery | SearchError::ZeroLimit => StatusCode::BAD_REQUEST,
            SearchError::Index(_) | SearchError::Embedder(_) | SearchError::VectorScan(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };

        Self::from_error(status, &error)
    }
}

/// Finds the notes that best match a query, each pointing at its best heading.
#[utoipa::path(
    post,
    path = "/search",
    request_body = SearchRequest,
    responses(
        (status = OK, description = "The matching notes", body = SearchResponse),
        (status = BAD_REQUEST, description = "The request is invalid", body = ErrorBody),
        (status = INTERNAL_SERVER_ERROR, description = "Searching failed", body = ErrorBody),
        Rejected,
    ),
)]
pub(super) async fn search(
    State(state): State<Arc<AppState>>,
    request: Result<Json<SearchRequest>, JsonRejection>,
) -> Result<Json<SearchResponse>, ApiError> {
    let Json(request) = request?;
    let response =
        blocking(move || search::search(&state.index, &state.embedder, &request)).await?;

    Ok(Json(response))
}
