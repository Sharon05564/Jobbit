//! Business logic for job management.
//!
//! The service layer sits between HTTP routes and the repository.
//! Routes call service functions; service functions call repository functions.
//! This separation keeps route handlers thin and logic testable.

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    error::AppError,
    models::{CreateJobRequest, Job, JobStatus},
    repository::job_repository,
};

pub async fn create_job(pool: &PgPool, req: CreateJobRequest) -> Result<Job, AppError> {
    let max_attempts = req.max_attempts.unwrap_or(3).max(1);
    let run_at = req.run_at.unwrap_or_else(Utc::now);

    let job =
        job_repository::create(pool, &req.job_type, &req.payload, max_attempts, run_at).await?;

    tracing::info!(
        job_id = %job.id,
        job_type = %job.job_type,
        run_at = %job.run_at,
        "Job created"
    );

    Ok(job)
}

pub async fn list_jobs(pool: &PgPool) -> Result<Vec<Job>, AppError> {
    let jobs = job_repository::find_all(pool).await?;
    Ok(jobs)
}

pub async fn get_job(pool: &PgPool, id: Uuid) -> Result<Job, AppError> {
    let job = job_repository::find_by_id(pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(job)
}

pub async fn list_jobs_by_status(pool: &PgPool, status_str: &str) -> Result<Vec<Job>, AppError> {
    let status: JobStatus = status_str
        .parse()
        .map_err(|_| AppError::InvalidStatus(status_str.to_string()))?;

    let jobs = job_repository::find_by_status(pool, &status).await?;
    Ok(jobs)
}

pub async fn list_dead_letter_jobs(pool: &PgPool) -> Result<Vec<Job>, AppError> {
    let jobs = job_repository::find_by_status(pool, &JobStatus::DeadLettered).await?;
    Ok(jobs)
}
