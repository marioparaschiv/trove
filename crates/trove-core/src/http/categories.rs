use std::sync::Arc;

use axum::{Json, extract::State};

use super::{
    AppState,
    error::{ApiError, ErrorBody},
    openapi::Rejected,
};
use crate::categories::{self, CategoriesResponse};

/// The category hierarchy of the indexed notes, each category with its name, start-here note and notes.
#[utoipa::path(
    get,
    path = "/categories",
    responses(
        (status = OK, description = "The categories", body = CategoriesResponse),
        (status = INTERNAL_SERVER_ERROR, description = "Building the categories failed", body = ErrorBody),
        Rejected,
    ),
)]
pub(super) async fn categories(
    State(state): State<Arc<AppState>>,
) -> Result<Json<CategoriesResponse>, ApiError> {
    let response = tokio::task::spawn_blocking(move || categories::categories(&state.index))
        .await
        .map_err(|error| ApiError::internal(&error))?
        .map_err(|error| ApiError::internal(&error))?;

    Ok(Json(response))
}
