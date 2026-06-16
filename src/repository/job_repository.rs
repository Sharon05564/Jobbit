//! All SQL queries for the jobs table live here.
//!
//! Keeping raw SQL in one place makes it easy to audit queries, add indexes,
//! and swap the database in the future without touching business logic.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{Job, JobStatus};

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

pub async fn find_all(pool: &PgPool) -> Result<Vec<Job>, sqlx::Error> {
    sqlx::query_as!(
        Job,
        r#"
        SELECT
            id, job_type, payload, status AS "status: JobStatus",
            attempts, max_attempts, run_at, locked_at, locked_by,
            last_error, created_at, updated_at, completed_at
        FROM jobs
        ORDER BY created_at DESC
        "#
    )
    .fetch_all(pool)
    .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Job>, sqlx::Error> {
    sqlx::query_as!(
        Job,
        r#"
        SELECT
            id, job_type, payload, status AS "status: JobStatus",
            attempts, max_attempts, run_at, locked_at, locked_by,
            last_error, created_at, updated_at, completed_at
        FROM jobs
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
}

pub async fn find_by_status(pool: &PgPool, status: &JobStatus) -> Result<Vec<Job>, sqlx::Error> {
    // Cast the enum column to text for comparison — avoids the need for
    // a registered pgtype binding while keeping the query simple.
    let status_str = status.to_string();
    sqlx::query_as!(
        Job,
        r#"
        SELECT
            id, job_type, payload, status AS "status: JobStatus",
            attempts, max_attempts, run_at, locked_at, locked_by,
            last_error, created_at, updated_at, completed_at
        FROM jobs
        WHERE status::text = $1
        ORDER BY created_at DESC
        "#,
        status_str
    )
    .fetch_all(pool)
    .await
}

// ---------------------------------------------------------------------------
// Writes
// ---------------------------------------------------------------------------

pub async fn create(
    pool: &PgPool,
    job_type: &str,
    payload: &serde_json::Value,
    max_attempts: i32,
    run_at: DateTime<Utc>,
) -> Result<Job, sqlx::Error> {
    sqlx::query_as!(
        Job,
        r#"
        INSERT INTO jobs (job_type, payload, max_attempts, run_at)
        VALUES ($1, $2, $3, $4)
        RETURNING
            id, job_type, payload, status AS "status: JobStatus",
            attempts, max_attempts, run_at, locked_at, locked_by,
            last_error, created_at, updated_at, completed_at
        "#,
        job_type,
        payload,
        max_attempts,
        run_at
    )
    .fetch_one(pool)
    .await
}

// ---------------------------------------------------------------------------
// Worker queries
// ---------------------------------------------------------------------------

/// Atomically claim a single queued job that is ready to run.
///
/// The SELECT ... FOR UPDATE SKIP LOCKED pattern is the standard way to build
/// a job queue in PostgreSQL. Multiple workers can call this at the same time
/// without blocking each other — each one gets a different row.
pub async fn claim_next_job(pool: &PgPool, worker_id: &str) -> Result<Option<Job>, sqlx::Error> {
    sqlx::query_as!(
        Job,
        r#"
        UPDATE jobs
        SET
            status    = 'running',
            locked_at = NOW(),
            locked_by = $1
        WHERE id = (
            SELECT id FROM jobs
            WHERE status = 'queued'
              AND run_at <= NOW()
            ORDER BY run_at ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        RETURNING
            id, job_type, payload, status AS "status: JobStatus",
            attempts, max_attempts, run_at, locked_at, locked_by,
            last_error, created_at, updated_at, completed_at
        "#,
        worker_id
    )
    .fetch_optional(pool)
    .await
}

/// Mark a job as successfully completed.
pub async fn mark_completed(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        UPDATE jobs
        SET
            status       = 'completed',
            completed_at = NOW(),
            locked_at    = NULL,
            locked_by    = NULL
        WHERE id = $1
        "#,
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Mark a job as failed and increment the attempt counter.
/// The caller decides whether to retry or dead-letter based on the counts.
pub async fn mark_failed(
    pool: &PgPool,
    id: Uuid,
    error_message: &str,
    retry_at: Option<DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    if let Some(next_run) = retry_at {
        // Schedule a retry: go back to queued with a future run_at
        sqlx::query!(
            r#"
            UPDATE jobs
            SET
                status     = 'queued',
                attempts   = attempts + 1,
                last_error = $2,
                run_at     = $3,
                locked_at  = NULL,
                locked_by  = NULL
            WHERE id = $1
            "#,
            id,
            error_message,
            next_run
        )
        .execute(pool)
        .await?;
    } else {
        // No more retries: move to dead-letter queue
        sqlx::query!(
            r#"
            UPDATE jobs
            SET
                status     = 'dead_lettered',
                attempts   = attempts + 1,
                last_error = $2,
                locked_at  = NULL,
                locked_by  = NULL
            WHERE id = $1
            "#,
            id,
            error_message
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Metrics helpers
// ---------------------------------------------------------------------------

/// Count of jobs grouped by status, used for the metrics endpoint.
pub struct StatusCounts {
    pub total: i64,
    pub queued: i64,
    pub running: i64,
    pub completed: i64,
    pub failed: i64,
    pub dead_lettered: i64,
}

pub async fn count_by_status(pool: &PgPool) -> Result<StatusCounts, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        SELECT
            COUNT(*)                                          AS total,
            COUNT(*) FILTER (WHERE status = 'queued')        AS queued,
            COUNT(*) FILTER (WHERE status = 'running')       AS running,
            COUNT(*) FILTER (WHERE status = 'completed')     AS completed,
            COUNT(*) FILTER (WHERE status = 'failed')        AS failed,
            COUNT(*) FILTER (WHERE status = 'dead_lettered') AS dead_lettered
        FROM jobs
        "#
    )
    .fetch_one(pool)
    .await?;

    Ok(StatusCounts {
        total: row.total.unwrap_or(0),
        queued: row.queued.unwrap_or(0),
        running: row.running.unwrap_or(0),
        completed: row.completed.unwrap_or(0),
        failed: row.failed.unwrap_or(0),
        dead_lettered: row.dead_lettered.unwrap_or(0),
    })
}

/// Average processing time in seconds for completed jobs.
pub async fn avg_processing_seconds(pool: &PgPool) -> Result<Option<f64>, sqlx::Error> {
    // Cast to FLOAT8 so SQLx maps the result to f64 instead of NUMERIC
    let row = sqlx::query!(
        r#"
        SELECT AVG(EXTRACT(EPOCH FROM (completed_at - created_at)))::FLOAT8 AS avg_seconds
        FROM jobs
        WHERE status = 'completed'
          AND completed_at IS NOT NULL
        "#
    )
    .fetch_one(pool)
    .await?;

    Ok(row.avg_seconds)
}
