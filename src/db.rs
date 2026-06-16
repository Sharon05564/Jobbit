use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use crate::config::Config;

/// Create and return a PostgreSQL connection pool.
///
/// SQLx manages a pool of reusable connections so we don't open a new
/// database connection on every request.
pub async fn create_pool(config: &Config) -> PgPool {
    PgPoolOptions::new()
        .max_connections(config.db_pool_size)
        .connect(&config.database_url)
        .await
        .expect("Failed to connect to PostgreSQL. Is the database running?")
}

/// Run all pending SQLx migrations from the ./migrations directory.
///
/// This is called once at startup so the schema is always up to date
/// before the server or worker begins accepting work.
pub async fn run_migrations(pool: &PgPool) {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .expect("Failed to run database migrations");
}
