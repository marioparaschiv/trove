use std::sync::Arc;

use axum::{
    Json,
    extract::{
        Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::StatusCode,
};

use super::{
    AppState,
    error::{ApiError, ErrorBody},
    openapi::Rejected,
};
use crate::suggestions::{
    self, OrphansRequest, OrphansResponse, SuggestionDecision, SuggestionsError,
    SuggestionsRequest, SuggestionsResponse,
};

impl From<SuggestionsError> for ApiError {
    fn from(error: SuggestionsError) -> Self {
        let status = match error {
            SuggestionsError::UnknownSource(_) | SuggestionsError::NotIndexed { .. } => {
                StatusCode::NOT_FOUND
            }
            SuggestionsError::SameNote | SuggestionsError::ZeroLimit => StatusCode::BAD_REQUEST,
            SuggestionsError::Index(_) | SuggestionsError::Store(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };

        Self::from_error(status, &error)
    }
}

/// Links worth adding: notes close in meaning with no link either way that weren't accepted or dismissed before.
#[utoipa::path(
    get,
    path = "/suggestions",
    params(SuggestionsRequest),
    responses(
        (status = OK, description = "The suggestions, most similar first", body = SuggestionsResponse),
        (status = BAD_REQUEST, description = "The request is invalid", body = ErrorBody),
        (status = NOT_FOUND, description = "The folder is not a source", body = ErrorBody),
        (status = INTERNAL_SERVER_ERROR, description = "Reading the index or the decisions failed", body = ErrorBody),
        Rejected,
    ),
)]
pub(super) async fn suggestions(
    State(state): State<Arc<AppState>>,
    request: Result<Query<SuggestionsRequest>, QueryRejection>,
) -> Result<Json<SuggestionsResponse>, ApiError> {
    let Query(request) = request?;
    let response = tokio::task::spawn_blocking(move || {
        suggestions::suggestions(&state.index, &state.config.store_folder, &request)
    })
    .await
    .map_err(|error| ApiError::internal(&error))??;

    Ok(Json(response))
}

/// Accepts or dismisses a suggested link (or sets it back to pending). The decision is kept in the source's sync
/// store, so it holds on every device that syncs the vault.
#[utoipa::path(
    post,
    path = "/suggestions/decisions",
    request_body = SuggestionDecision,
    responses(
        (status = NO_CONTENT, description = "The decision is recorded"),
        (status = BAD_REQUEST, description = "The request is invalid", body = ErrorBody),
        (status = NOT_FOUND, description = "The folder is not a source or a note is not indexed", body = ErrorBody),
        (status = INTERNAL_SERVER_ERROR, description = "Writing the decision failed", body = ErrorBody),
        Rejected,
    ),
)]
pub(super) async fn decide(
    State(state): State<Arc<AppState>>,
    request: Result<Json<SuggestionDecision>, JsonRejection>,
) -> Result<StatusCode, ApiError> {
    let Json(request) = request?;
    tokio::task::spawn_blocking(move || {
        suggestions::decide(&state.index, &state.config.store_folder, &request)
    })
    .await
    .map_err(|error| ApiError::internal(&error))??;

    Ok(StatusCode::NO_CONTENT)
}

/// The notes with no link in or out.
#[utoipa::path(
    get,
    path = "/orphans",
    params(OrphansRequest),
    responses(
        (status = OK, description = "The orphan notes, by path", body = OrphansResponse),
        (status = BAD_REQUEST, description = "The request is invalid", body = ErrorBody),
        (status = NOT_FOUND, description = "The folder is not a source", body = ErrorBody),
        (status = INTERNAL_SERVER_ERROR, description = "Reading the index failed", body = ErrorBody),
        Rejected,
    ),
)]
pub(super) async fn orphans(
    State(state): State<Arc<AppState>>,
    request: Result<Query<OrphansRequest>, QueryRejection>,
) -> Result<Json<OrphansResponse>, ApiError> {
    let Query(request) = request?;
    let response =
        tokio::task::spawn_blocking(move || suggestions::orphans(&state.index, &request))
            .await
            .map_err(|error| ApiError::internal(&error))??;

    Ok(Json(response))
}
