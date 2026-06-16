//! Exponential backoff calculation for job retries.
//!
//! Each retry waits longer than the previous one, which gives transient
//! failures (network timeouts, temporary service outages) time to recover
//! before the job tries again.
//!
//! Formula: base_seconds * 2^attempt  (capped at max_delay_seconds)
//!
//! Example with base=10s:
//!   attempt 1 → 10 * 2^1 = 20 seconds
//!   attempt 2 → 10 * 2^2 = 40 seconds
//!   attempt 3 → 10 * 2^3 = 80 seconds

use chrono::{DateTime, Duration, Utc};

const BASE_DELAY_SECONDS: i64 = 10;
const MAX_DELAY_SECONDS: i64 = 3600; // cap at 1 hour

/// Compute the earliest `run_at` timestamp for the next retry.
///
/// `attempt` is the number of times the job has already been tried
/// (i.e. the current `attempts` value before incrementing).
pub fn next_run_at(attempt: i32) -> DateTime<Utc> {
    let delay = calculate_delay_seconds(attempt);
    Utc::now() + Duration::seconds(delay)
}

/// Return the delay in seconds for a given attempt number.
pub fn calculate_delay_seconds(attempt: i32) -> i64 {
    // 2^attempt overflows for large attempt numbers; cap the exponent at 6
    // so the delay saturates at 640 seconds well before MAX_DELAY_SECONDS.
    let exponent = attempt.min(6) as u32;
    let delay = BASE_DELAY_SECONDS * (2_i64.pow(exponent));
    delay.min(MAX_DELAY_SECONDS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delays_grow_exponentially() {
        let d0 = calculate_delay_seconds(0);
        let d1 = calculate_delay_seconds(1);
        let d2 = calculate_delay_seconds(2);

        assert_eq!(d0, 10);
        assert_eq!(d1, 20);
        assert_eq!(d2, 40);
    }

    #[test]
    fn delay_is_capped_at_max() {
        // The exponent is capped at 6 (10 * 2^6 = 640), which is below
        // MAX_DELAY_SECONDS (3600), so very large attempt numbers saturate at 640.
        let large_attempt_delay = calculate_delay_seconds(100);
        assert_eq!(large_attempt_delay, 640);
        assert!(large_attempt_delay <= MAX_DELAY_SECONDS);
    }

    #[test]
    fn next_run_at_is_in_the_future() {
        let before = Utc::now();
        let run_at = next_run_at(1);
        assert!(run_at > before);
    }
}
