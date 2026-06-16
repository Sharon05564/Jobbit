use axum::{extract::State, Json};
use sqlx::PgPool;

use crate::{error::AppError, services::metrics_service};

/// GET /api/metrics
///
/// Returns a snapshot of job queue health: counts by status and average
/// processing time. Useful for dashboards and alerting.
pub async fn get_metrics(
    State(pool): State<PgPool>,
) -> Result<Json<metrics_service::Metrics>, AppError> {
    let metrics = metrics_service::get_metrics(&pool).await?;
    Ok(Json(metrics))
}
