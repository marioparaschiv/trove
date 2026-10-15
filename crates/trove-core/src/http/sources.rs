use std::{path::PathBuf, sync::Arc};

use axum::{
    Json,
    extract::{
        Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::{
    AppState,
    error::{ApiError, ErrorBody},
    openapi::Rejected,
};
use crate::index::{self, Index, Source, SourceError};

/// A source folder, by its absolute path on this device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct SourcePath {
    pub path: String,
}

impl From<SourceError> for ApiError {
    fn from(error: SourceError) -> Self {
        let status = match error {
            SourceError::NotAbsolute(_)
            | SourceError::NotUnicode(_)
            | SourceError::NotAFolder(_) => StatusCode::BAD_REQUEST,
            SourceError::AlreadyAdded(_) => StatusCode::CONFLICT,
            SourceError::NotFound(_) => StatusCode::NOT_FOUND,
            SourceError::Index(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        Self::from_error(status, &error)
    }
}

/// The registered source folders.
#[utoipa::path(
    get,
    path = "/sources",
    responses(
        (status = OK, description = "Every source", body = Vec<Source>),
        (status = INTERNAL_SERVER_ERROR, description = "Reading the index failed", body = ErrorBody),
        Rejected,
    ),
)]
pub(super) async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Source>>, ApiError> {
    let sources = with_index(&state, |index| Ok(index.sources()?)).await?;

    Ok(Json(sources))
}

/// Registers a folder to index. The next index run reads it.
#[utoipa::path(
    post,
    path = "/sources",
    request_body = SourcePath,
    responses(
        (status = CREATED, description = "The folder is a source", body = Source),
        (status = BAD_REQUEST, description = "The path is not an absolute path to a folder", body = ErrorBody),
        (status = CONFLICT, description = "The folder is already a source", body = ErrorBody),
        (status = INTERNAL_SERVER_ERROR, description = "Writing the index failed", body = ErrorBody),
        Rejected,
    ),
)]
pub(super) async fn add(
    State(state): State<Arc<AppState>>,
    request: Result<Json<SourcePath>, JsonRejection>,
) -> Result<(StatusCode, Json<Source>), ApiError> {
    let Json(SourcePath { path }) = request?;
    let source = with_index(&state, move |index| index.add_source(&PathBuf::from(path))).await?;
    state.watcher.watch(&source.path);

    Ok((StatusCode::CREATED, Json(source)))
}

/// Unregisters a folder and drops everything indexed from it.
#[utoipa::path(
    delete,
    path = "/sources",
    params(SourcePath),
    responses(
        (status = OK, description = "The removed source", body = Source),
        (status = BAD_REQUEST, description = "The path is not absolute", body = ErrorBody),
        (status = NOT_FOUND, description = "The folder is not a source", body = ErrorBody),
        (status = INTERNAL_SERVER_ERROR, description = "Writing the index failed", body = ErrorBody),
        Rejected,
    ),
)]
pub(super) async fn remove(
    State(state): State<Arc<AppState>>,
    request: Result<Query<SourcePath>, QueryRejection>,
) -> Result<Json<Source>, ApiError> {
    let Query(SourcePath { path }) = request?;
    let source = with_index(&state, move |index| {
        index.remove_source(&PathBuf::from(path))
    })
    .await?;
    state.watcher.unwatch(&source.path);

    Ok(Json(source))
}

async fn with_index<T: Send + 'static>(
    state: &Arc<AppState>,
    work: impl FnOnce(&mut Index) -> Result<T, SourceError> + Send + 'static,
) -> Result<T, ApiError> {
    let state = Arc::clone(state);
    let result = tokio::task::spawn_blocking(move || work(&mut index::lock(&state.index)))
        .await
        .map_err(|error| ApiError::internal(&error))?;

    Ok(result?)
}
