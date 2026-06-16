use std::env;

/// All runtime configuration loaded from environment variables.
/// Values come from the .env file (via dotenvy) or real environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    /// Full PostgreSQL connection string, e.g. postgres://user:pass@localhost/jobbit
    pub database_url: String,

    /// Host the HTTP server binds to
    pub host: String,

    /// Port the HTTP server listens on
    pub port: u16,

    /// Maximum number of database connections in the pool
    pub db_pool_size: u32,

    /// How often the worker polls for new jobs, in milliseconds
    pub worker_poll_interval_ms: u64,

    /// Unique name for this worker instance (used for job locking)
    pub worker_id: String,
}

impl Config {
    /// Load configuration from environment variables.
    /// Panics on startup if required variables are missing.
    pub fn from_env() -> Self {
        Config {
            database_url: required_var("DATABASE_URL"),
            host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: env::var("PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .expect("PORT must be a valid number"),
            db_pool_size: env::var("DB_POOL_SIZE")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .expect("DB_POOL_SIZE must be a valid number"),
            worker_poll_interval_ms: env::var("WORKER_POLL_INTERVAL_MS")
                .unwrap_or_else(|_| "2000".to_string())
                .parse()
                .expect("WORKER_POLL_INTERVAL_MS must be a valid number"),
            worker_id: env::var("WORKER_ID").unwrap_or_else(|_| {
                // Default to hostname + PID so multiple workers are distinguishable
                format!("worker-{}", std::process::id())
            }),
        }
    }
}

fn required_var(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("Required environment variable '{}' is not set", name))
}
