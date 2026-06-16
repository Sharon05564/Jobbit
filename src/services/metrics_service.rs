use serde::Serialize;
use sqlx::PgPool;

use crate::{error::AppError, repository::job_repository};

/// Snapshot of job queue health, returned by GET /api/metrics.
#[derive(Debug, Serialize)]
pub struct Metrics {
    pub total_jobs: i64,
    pub queued_jobs: i64,
    pub running_jobs: i64,
    pub completed_jobs: i64,
    pub failed_jobs: i64,
    pub dead_lettered_jobs: i64,
    /// Average seconds from job creation to completion (None if no completed jobs)
    pub avg_processing_seconds: Option<f64>,
}

pub async fn get_metrics(pool: &PgPool) -> Result<Metrics, AppError> {
    let counts = job_repository::count_by_status(pool).await?;
    let avg = job_repository::avg_processing_seconds(pool).await?;

    Ok(Metrics {
        total_jobs: counts.total,
        queued_jobs: counts.queued,
        running_jobs: counts.running,
        completed_jobs: counts.completed,
        failed_jobs: counts.failed,
        dead_lettered_jobs: counts.dead_lettered,
        avg_processing_seconds: avg,
    })
}
