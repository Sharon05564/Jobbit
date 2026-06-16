mod config;
mod db;
mod error;
mod models;
mod repository;
mod routes;
mod services;
mod worker;

use clap::{Parser, Subcommand};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{fmt, EnvFilter};

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "jobbit", about = "Durable background job queue")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start the HTTP API server
    Api,
    /// Start the background job worker
    Worker,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    // Load .env file if present (silently ignored if missing)
    let _ = dotenvy::dotenv();

    // Set up structured logging.
    // RUST_LOG controls which levels are emitted, e.g. RUST_LOG=jobbit=debug,info
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let config = config::Config::from_env();

    tracing::info!(command = ?std::env::args().nth(1), "Jobbit starting");

    let pool = db::create_pool(&config).await;
    db::run_migrations(&pool).await;

    tracing::info!("Database migrations applied");

    match cli.command {
        Command::Api => run_api_server(config, pool).await,
        Command::Worker => run_worker(config, pool).await,
    }
}

// ---------------------------------------------------------------------------
// API server
// ---------------------------------------------------------------------------

async fn run_api_server(config: config::Config, pool: sqlx::PgPool) {
    let router = routes::build_router(pool);

    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to bind to {}: {}", addr, e));

    tracing::info!(address = %addr, "API server listening");

    // Graceful shutdown: wait for Ctrl-C
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("Server error");

    tracing::info!("API server shut down");
}

// ---------------------------------------------------------------------------
// Worker
// ---------------------------------------------------------------------------

async fn run_worker(config: config::Config, pool: sqlx::PgPool) {
    let shutdown = CancellationToken::new();
    let shutdown_clone = shutdown.clone();

    // Spawn a task that waits for Ctrl-C and then triggers the token
    tokio::spawn(async move {
        shutdown_signal().await;
        tracing::info!("Shutdown signal received — stopping worker");
        shutdown_clone.cancel();
    });

    worker::processor::run(
        pool,
        config.worker_id,
        config.worker_poll_interval_ms,
        shutdown,
    )
    .await;
}

// ---------------------------------------------------------------------------
// OS signal handling
// ---------------------------------------------------------------------------

/// Returns a future that resolves when Ctrl-C (SIGINT) is received.
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl-C handler");
    tracing::info!("Ctrl-C received");
}
