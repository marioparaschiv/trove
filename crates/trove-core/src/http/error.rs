use std::error::Error;

use axum::{
    Json,
    extract::rejection::{JsonRejection, QueryRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::full_message;

/// The body of every error response.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ErrorBody {
    pub error: String,
}

pub(crate) struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub(crate) fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    /// A response whose message includes the whole chain of causes.
    pub(crate) fn from_error(status: StatusCode, error: &dyn Error) -> Self {
        Self::new(status, full_message(error))
    }

    pub(crate) fn internal(error: &dyn Error) -> Self {
        Self::from_error(StatusCode::INTERNAL_SERVER_ERROR, error)
    }
}

// Every malformed request is a 400 with an `ErrorBody`, so clients handle one shape.
impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        Self::new(StatusCode::BAD_REQUEST, rejection.body_text())
    }
}

impl From<QueryRejection> for ApiError {
    fn from(rejection: QueryRejection) -> Self {
        Self::new(StatusCode::BAD_REQUEST, rejection.body_text())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                error: self.message,
            }),
        )
            .into_response()
    }
}
