use std::sync::Arc;

use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
};

use super::{
    AppState,
    error::{ApiError, ErrorBody},
    openapi::Rejected,
};
use crate::categories::{self, CategoriesRequest, CategoriesResponse};

/// The category hierarchy of the indexed notes, each category with its name, start-here note and notes.
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
    let response =
        tokio::task::spawn_blocking(move || categories::categories(&state.index, &request))
            .await
            .map_err(|error| ApiError::internal(&error))?
            .map_err(|error| ApiError::internal(&error))?;

    Ok(Json(response))
}
