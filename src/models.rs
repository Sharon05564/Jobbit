use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Type;
use uuid::Uuid;

/// The lifecycle states a job can be in.
/// SQLx maps this to the PostgreSQL "job_status" enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "job_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    DeadLettered,
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            JobStatus::Queued => "queued",
            JobStatus::Running => "running",
            JobStatus::Completed => "completed",
            JobStatus::Failed => "failed",
            JobStatus::DeadLettered => "dead_lettered",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for JobStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "queued" => Ok(JobStatus::Queued),
            "running" => Ok(JobStatus::Running),
            "completed" => Ok(JobStatus::Completed),
            "failed" => Ok(JobStatus::Failed),
            "dead_lettered" => Ok(JobStatus::DeadLettered),
            other => Err(format!("Unknown job status: '{}'", other)),
        }
    }
}

/// A single job record as stored in PostgreSQL.
/// The `#[sqlx(rename_all = "snake_case")]` tells SQLx which column name to
/// map each field to when running `query_as!`.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Job {
    pub id: Uuid,
    pub job_type: String,
    pub payload: serde_json::Value,
    pub status: JobStatus,
    pub attempts: i32,
    pub max_attempts: i32,
    pub run_at: DateTime<Utc>,
    pub locked_at: Option<DateTime<Utc>>,
    pub locked_by: Option<String>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Request / response shapes used by the HTTP API
// ---------------------------------------------------------------------------

/// Body accepted by POST /api/jobs
#[derive(Debug, Deserialize)]
pub struct CreateJobRequest {
    pub job_type: String,

    #[serde(default = "default_payload")]
    pub payload: serde_json::Value,

    /// Defaults to 3 if not provided
    pub max_attempts: Option<i32>,

    /// Defaults to now if not provided (supports scheduling jobs in the future)
    pub run_at: Option<DateTime<Utc>>,
}

fn default_payload() -> serde_json::Value {
    serde_json::json!({})
}

/// Lightweight summary returned in list responses
#[derive(Debug, Serialize)]
pub struct JobSummary {
    pub id: Uuid,
    pub job_type: String,
    pub status: JobStatus,
    pub attempts: i32,
    pub max_attempts: i32,
    pub run_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub last_error: Option<String>,
}

impl From<Job> for JobSummary {
    fn from(j: Job) -> Self {
        JobSummary {
            id: j.id,
            job_type: j.job_type,
            status: j.status,
            attempts: j.attempts,
            max_attempts: j.max_attempts,
            run_at: j.run_at,
            created_at: j.created_at,
            last_error: j.last_error,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests for model logic (no database needed)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_status_round_trips_through_string() {
        let statuses = [
            JobStatus::Queued,
            JobStatus::Running,
            JobStatus::Completed,
            JobStatus::Failed,
            JobStatus::DeadLettered,
        ];
        for status in &statuses {
            let s = status.to_string();
            let parsed: JobStatus = s.parse().expect("should parse back");
            assert_eq!(status, &parsed, "round-trip failed for {}", s);
        }
    }

    #[test]
    fn unknown_status_returns_err() {
        let result: Result<JobStatus, _> = "in_flight".parse();
        assert!(result.is_err());
    }

    #[test]
    fn create_job_request_defaults_payload_to_empty_object() {
        let json = r#"{ "job_type": "send_email" }"#;
        let req: CreateJobRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.payload, serde_json::json!({}));
        assert_eq!(req.max_attempts, None);
        assert_eq!(req.run_at, None);
    }
}
