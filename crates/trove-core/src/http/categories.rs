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
use crate::categories::{
    self, CategoriesError, CategoriesRequest, CategoriesResponse, CategoryPins,
};

impl From<CategoriesError> for ApiError {
    fn from(error: CategoriesError) -> Self {
        let status = match error {
            CategoriesError::UnknownSource(_) => StatusCode::NOT_FOUND,
            CategoriesError::EmptyName => StatusCode::BAD_REQUEST,
            CategoriesError::Index(_) | CategoriesError::Store(_) | CategoriesError::Leiden(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };

        Self::from_error(status, &error)
    }
}

/// The category hierarchy of the indexed notes, each category with its name, start-here note and notes. The category
/// pins in each source's store override the automatic grouping.
#[utoipa::path(
    get,
    path = "/categories",
    params(CategoriesRequest),
    responses(
        (status = OK, description = "The categories", body = CategoriesResponse),
        (status = BAD_REQUEST, description = "The request is invalid", body = ErrorBody),
        (status = INTERNAL_SERVER_ERROR, description = "Building the categories failed", body = ErrorBody),
        Rejected,
    ),
)]
pub(super) async fn categories(
    State(state): State<Arc<AppState>>,
    request: Result<Query<CategoriesRequest>, QueryRejection>,
) -> Result<Json<CategoriesResponse>, ApiError> {
    let Query(request) = request?;
    let response = tokio::task::spawn_blocking(move || {
        categories::categories(&state.index, &state.config.store_folder, &request)
    })
    .await
    .map_err(|error| ApiError::internal(&error))??;

    Ok(Json(response))
}

/// Pins manual category edits (renames, merges, moves) so later runs keep them. The pins are kept in the source's sync
/// store, so they hold on every device that syncs the vault.
#[utoipa::path(
    put,
    path = "/categories/pins",
    request_body = CategoryPins,
    responses(
        (status = NO_CONTENT, description = "The pins are recorded"),
        (status = BAD_REQUEST, description = "The request is invalid", body = ErrorBody),
        (status = NOT_FOUND, description = "The folder is not a source", body = ErrorBody),
        (status = INTERNAL_SERVER_ERROR, description = "Writing the pins failed", body = ErrorBody),
        Rejected,
    ),
)]
pub(super) async fn pin(
    State(state): State<Arc<AppState>>,
    request: Result<Json<CategoryPins>, JsonRejection>,
) -> Result<StatusCode, ApiError> {
    let Json(request) = request?;
    tokio::task::spawn_blocking(move || {
        categories::pin(&state.index, &state.config.store_folder, request)
    })
    .await
    .map_err(|error| ApiError::internal(&error))??;

    Ok(StatusCode::NO_CONTENT)
}
