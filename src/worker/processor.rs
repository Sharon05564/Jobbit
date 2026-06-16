//! The main worker loop.
//!
//! The processor:
//! 1. Claims the next queued job using a SELECT FOR UPDATE SKIP LOCKED query
//! 2. Dispatches it to the right handler
//! 3. On success → marks the job completed
//! 4. On failure → increments the attempt counter
//!    - if attempts < max_attempts → reschedules with exponential backoff
//!    - if attempts >= max_attempts → moves to dead-letter queue
//! 5. Sleeps for poll_interval_ms and repeats
//!
//! A `CancellationToken` (from tokio) lets the caller shut the worker down
//! cleanly on SIGINT/SIGTERM without dropping mid-flight jobs.

use sqlx::PgPool;
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;

use crate::{
    repository::job_repository,
    worker::{backoff, handlers},
};

/// Run the worker until the cancellation token is triggered.
pub async fn run(
    pool: PgPool,
    worker_id: String,
    poll_interval_ms: u64,
    shutdown: CancellationToken,
) {
    tracing::info!(worker_id = %worker_id, poll_interval_ms = poll_interval_ms, "Worker started");

    loop {
        // Check for shutdown signal before each poll cycle
        if shutdown.is_cancelled() {
            tracing::info!(worker_id = %worker_id, "Shutdown signal received — worker stopping");
            break;
        }

        match process_one(&pool, &worker_id).await {
            Ok(true) => {
                // A job was processed; immediately try to claim another one
                // instead of sleeping, so we drain the queue quickly.
            }
            Ok(false) => {
                // Queue was empty; sleep before polling again
                tokio::select! {
                    _ = sleep(Duration::from_millis(poll_interval_ms)) => {}
                    _ = shutdown.cancelled() => {
                        tracing::info!(worker_id = %worker_id, "Shutdown during sleep — worker stopping");
                        break;
                    }
                }
            }
            Err(e) => {
                // Database or unexpected error; log and back off briefly
                tracing::error!(error = %e, "Worker encountered an error");
                tokio::select! {
                    _ = sleep(Duration::from_secs(5)) => {}
                    _ = shutdown.cancelled() => { break; }
                }
            }
        }
    }

    tracing::info!(worker_id = %worker_id, "Worker stopped");
}

/// Try to claim and process one job. Returns Ok(true) if a job was processed,
/// Ok(false) if the queue was empty, or Err on a database failure.
async fn process_one(pool: &PgPool, worker_id: &str) -> Result<bool, sqlx::Error> {
    let Some(job) = job_repository::claim_next_job(pool, worker_id).await? else {
        return Ok(false);
    };

    tracing::info!(
        job_id   = %job.id,
        job_type = %job.job_type,
        attempt  = job.attempts + 1,
        max      = job.max_attempts,
        "Processing job"
    );

    match handlers::dispatch(&job.job_type, &job.payload).await {
        Ok(()) => {
            job_repository::mark_completed(pool, job.id).await?;
            tracing::info!(job_id = %job.id, "Job completed");
        }
        Err(error_message) => {
            handle_failure(pool, &job, &error_message).await?;
        }
    }

    Ok(true)
}

async fn handle_failure(
    pool: &PgPool,
    job: &crate::models::Job,
    error_message: &str,
) -> Result<(), sqlx::Error> {
    // attempts hasn't been incremented yet; the next attempt number is attempts + 1.
    let next_attempt = job.attempts + 1;

    if next_attempt < job.max_attempts {
        let retry_at = backoff::next_run_at(next_attempt);
        let delay = backoff::calculate_delay_seconds(next_attempt);

        tracing::warn!(
            job_id  = %job.id,
            attempt = next_attempt,
            max     = job.max_attempts,
            delay_s = delay,
            error   = %error_message,
            "Job failed — scheduling retry"
        );

        job_repository::mark_failed(pool, job.id, error_message, Some(retry_at)).await?;
    } else {
        tracing::error!(
            job_id  = %job.id,
            attempt = next_attempt,
            max     = job.max_attempts,
            error   = %error_message,
            "Job exhausted retries — dead-lettering"
        );

        job_repository::mark_failed(pool, job.id, error_message, None).await?;
    }

    Ok(())
}
