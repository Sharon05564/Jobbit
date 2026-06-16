pub mod health;
pub mod jobs;
pub mod metrics;

use axum::{
    routing::{get, post},
    Router,
};
use sqlx::PgPool;

/// Build the complete Axum router with all routes registered.
///
/// The database pool is stored in Axum's shared state so every handler
/// can access it without using global variables.
pub fn build_router(pool: PgPool) -> Router {
    Router::new()
        // Health
        .route("/health", get(health::health_check))
        // Jobs
        .route("/api/jobs", post(jobs::create_job))
        .route("/api/jobs", get(jobs::list_jobs))
        .route("/api/jobs/:id", get(jobs::get_job))
        .route("/api/jobs/status/:status", get(jobs::list_by_status))
        // Dead-letter queue
        .route("/api/dead-letter", get(jobs::list_dead_letter))
        // Metrics
        .route("/api/metrics", get(metrics::get_metrics))
        .with_state(pool)
}
