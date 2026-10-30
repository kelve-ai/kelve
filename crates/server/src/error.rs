//! Errors as the web app and agents see them: a status and a sentence for
//! people. Anything unexpected is logged in full and shown as a plain apology.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
    /// More for the app to act on.
    pub extra: Option<serde_json::Value>,
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            extra: None,
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, message)
    }

    /// Something went wrong on our side. The cause goes to the log, not to people.
    pub fn internal(cause: impl std::fmt::Display) -> Self {
        tracing::error!(error = %cause, "request failed");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Something went wrong on our side. Try again in a moment.",
        )
    }

    pub fn with(mut self, extra: serde_json::Value) -> Self {
        self.extra = Some(extra);
        self
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut body = json!({ "error": self.message });
        if let (Some(serde_json::Value::Object(extra)), Some(map)) =
            (self.extra, body.as_object_mut())
        {
            map.extend(extra);
        }
        (self.status, Json(body)).into_response()
    }
}

impl From<kelve_store::Error> for ApiError {
    fn from(e: kelve_store::Error) -> Self {
        ApiError::internal(e)
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        ApiError::internal(e)
    }
}

impl From<crate::pipeline::Failed> for ApiError {
    fn from(e: crate::pipeline::Failed) -> Self {
        match e {
            crate::pipeline::Failed::Missing(message) => ApiError::not_found(message),
            other => ApiError::internal(other),
        }
    }
}
