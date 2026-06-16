use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

/// Application-level errors.
///
/// Each variant maps to a specific HTTP status code when returned from a
/// route handler. The `#[from]` attributes allow using `?` to convert
/// lower-level errors automatically.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Job not found")]
    NotFound,

    #[error("Invalid job status: {0}")]
    InvalidStatus(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

/// Convert AppError into an Axum HTTP response.
///
/// Axum requires route handlers to return types that implement IntoResponse.
/// By implementing this, we can use `?` in route handlers and get proper
/// JSON error bodies automatically.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::InvalidStatus(s) => {
                (StatusCode::BAD_REQUEST, format!("Invalid status: {}", s))
            }
            AppError::Database(e) => {
                tracing::error!(error = %e, "Database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Database error".to_string(),
                )
            }
            AppError::Internal(e) => {
                tracing::error!(error = %e, "Internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
        };

        let body = Json(json!({ "error": message }));
        (status, body).into_response()
    }
}
