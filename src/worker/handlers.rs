//! Simulated job handlers — one per job_type.
//!
//! In a real system these would call external services (SMTP, S3, webhooks).
//! Here they sleep briefly to simulate work and occasionally fail so the
//! retry/dead-letter paths can be exercised.
//!
//! Returning Ok(()) means success; Err(String) is the error message stored
//! in last_error.

use serde_json::Value;
use tokio::time::{sleep, Duration};

/// Dispatch a job to the right handler based on its job_type.
///
/// Unknown job types succeed immediately with a warning log so they don't
/// clog the queue forever.
pub async fn dispatch(job_type: &str, payload: &Value) -> Result<(), String> {
    match job_type {
        "send_email" => handle_send_email(payload).await,
        "process_file" => handle_process_file(payload).await,
        "run_webhook" => handle_run_webhook(payload).await,
        unknown => {
            tracing::warn!(
                job_type = unknown,
                "Unknown job type — marking as completed"
            );
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Individual handlers
// ---------------------------------------------------------------------------

async fn handle_send_email(payload: &Value) -> Result<(), String> {
    let to = payload
        .get("to")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown@example.com");

    tracing::info!(to = to, "Simulating send_email");
    sleep(Duration::from_millis(200)).await;

    // Simulate a ~20% failure rate so retries get exercised
    if should_fail(payload, 0.2) {
        return Err("SMTP server returned 500 — temporary failure".to_string());
    }

    tracing::info!(to = to, "Email sent successfully");
    Ok(())
}

async fn handle_process_file(payload: &Value) -> Result<(), String> {
    let file = payload
        .get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown.txt");

    tracing::info!(file = file, "Simulating process_file");
    sleep(Duration::from_millis(400)).await;

    if should_fail(payload, 0.15) {
        return Err("File processing timed out".to_string());
    }

    tracing::info!(file = file, "File processed successfully");
    Ok(())
}

async fn handle_run_webhook(payload: &Value) -> Result<(), String> {
    let url = payload
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("https://example.com/webhook");

    tracing::info!(url = url, "Simulating run_webhook");
    sleep(Duration::from_millis(300)).await;

    if should_fail(payload, 0.1) {
        return Err("Webhook endpoint returned 503 — service unavailable".to_string());
    }

    tracing::info!(url = url, "Webhook delivered successfully");
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return true with probability `rate` (0.0–1.0) UNLESS the payload contains
/// `"force_fail": true` or `"force_success": true`, which let tests control
/// the outcome deterministically.
fn should_fail(payload: &Value, rate: f64) -> bool {
    if payload.get("force_fail").and_then(|v| v.as_bool()) == Some(true) {
        return true;
    }
    if payload.get("force_success").and_then(|v| v.as_bool()) == Some(true) {
        return false;
    }
    // Use a simple deterministic pseudo-random based on system time nanoseconds
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    (ns as f64 / u32::MAX as f64) < rate
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn force_success_always_succeeds() {
        let payload = json!({ "to": "test@example.com", "force_success": true });
        let result = dispatch("send_email", &payload).await;
        assert!(result.is_ok(), "expected Ok, got {:?}", result);
    }

    #[tokio::test]
    async fn force_fail_always_fails() {
        let payload = json!({ "to": "test@example.com", "force_fail": true });
        let result = dispatch("send_email", &payload).await;
        assert!(result.is_err(), "expected Err, got {:?}", result);
    }

    #[tokio::test]
    async fn unknown_job_type_succeeds() {
        let payload = json!({});
        let result = dispatch("totally_unknown_type", &payload).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn process_file_force_fail() {
        let payload = json!({ "file_path": "/tmp/test.csv", "force_fail": true });
        let result = dispatch("process_file", &payload).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn run_webhook_force_success() {
        let payload = json!({ "url": "https://example.com/hook", "force_success": true });
        let result = dispatch("run_webhook", &payload).await;
        assert!(result.is_ok());
    }
}
