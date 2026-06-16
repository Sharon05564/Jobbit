use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    error::AppError,
    models::{CreateJobRequest, Job, JobSummary},
    services::job_service,
};

// ---------------------------------------------------------------------------
// POST /api/jobs
// ---------------------------------------------------------------------------

/// Submit a new background job.
///
/// Returns the created job with HTTP 201 Created.
pub async fn create_job(
    State(pool): State<PgPool>,
    Json(req): Json<CreateJobRequest>,
) -> Result<(StatusCode, Json<Job>), AppError> {
    let job = job_service::create_job(&pool, req).await?;
    Ok((StatusCode::CREATED, Json(job)))
}

// ---------------------------------------------------------------------------
// GET /api/jobs
// ---------------------------------------------------------------------------

pub async fn list_jobs(State(pool): State<PgPool>) -> Result<Json<Vec<JobSummary>>, AppError> {
    let jobs = job_service::list_jobs(&pool).await?;
    let summaries: Vec<JobSummary> = jobs.into_iter().map(JobSummary::from).collect();
    Ok(Json(summaries))
}

// ---------------------------------------------------------------------------
// GET /api/jobs/:id
// ---------------------------------------------------------------------------

pub async fn get_job(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<Job>, AppError> {
    let job = job_service::get_job(&pool, id).await?;
    Ok(Json(job))
}

// ---------------------------------------------------------------------------
// GET /api/jobs/status/:status
// ---------------------------------------------------------------------------

pub async fn list_by_status(
    State(pool): State<PgPool>,
    Path(status): Path<String>,
) -> Result<Json<Vec<JobSummary>>, AppError> {
    let jobs = job_service::list_jobs_by_status(&pool, &status).await?;
    let summaries: Vec<JobSummary> = jobs.into_iter().map(JobSummary::from).collect();
    Ok(Json(summaries))
}

// ---------------------------------------------------------------------------
// GET /api/dead-letter
// ---------------------------------------------------------------------------

pub async fn list_dead_letter(State(pool): State<PgPool>) -> Result<Json<Vec<Job>>, AppError> {
    let jobs = job_service::list_dead_letter_jobs(&pool).await?;
    Ok(Json(jobs))
}
