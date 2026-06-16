# Jobbit

A durable background job queue built in Rust.

Jobbit lets applications submit background jobs through an HTTP API. A separate worker process pulls queued jobs from PostgreSQL, executes them, retries failures with exponential backoff, and moves permanently failed jobs to a dead-letter queue.

---

## Table of Contents

- [Architecture](#architecture)
- [Tech Stack](#tech-stack)
- [Project Structure](#project-structure)
- [Database Schema](#database-schema)
- [Job Lifecycle](#job-lifecycle)
- [Worker Behavior](#worker-behavior)
- [Retry & Backoff](#retry--backoff)
- [API Endpoints](#api-endpoints)
- [Sample curl Requests](#sample-curl-requests)
- [Setup & Running Locally](#setup--running-locally)
- [Running Tests](#running-tests)
- [Environment Variables](#environment-variables)
- [Future Improvements](#future-improvements)

---

## Architecture

```
┌──────────────┐        POST /api/jobs        ┌────────────────────┐
│  API Client  │ ────────────────────────────▶ │   Axum HTTP API    │
└──────────────┘                               │   (src/routes/)    │
                                               └────────┬───────────┘
                                                        │ INSERT job
                                                        ▼
                                               ┌────────────────────┐
                                               │    PostgreSQL DB    │
                                               │   (jobs table)     │
                                               └────────┬───────────┘
                                                        │ SELECT FOR UPDATE
                                                        ▼        SKIP LOCKED
                                               ┌────────────────────┐
                                               │  Background Worker  │
                                               │  (src/worker/)     │
                                               │                    │
                                               │  claim → execute   │
                                               │  → complete/retry  │
                                               │  → dead-letter     │
                                               └────────────────────┘
```

The API server and worker are compiled into the same binary and selected by a CLI subcommand:

```
cargo run -- api      # starts the HTTP server
cargo run -- worker   # starts the background worker
```

Both can run simultaneously (and in separate containers via Docker Compose) sharing the same PostgreSQL database.

---

## Tech Stack

| Layer | Technology |
|---|---|
| Language | Rust (edition 2021) |
| Web framework | Axum 0.7 |
| Async runtime | Tokio 1 |
| Database | PostgreSQL 16 |
| Database access | SQLx 0.7 (compile-time checked queries) |
| Serialization | Serde + serde_json |
| IDs | UUID v4 |
| Timestamps | Chrono |
| Config | dotenvy (.env loading) |
| Logging | tracing + tracing-subscriber |
| Error handling | thiserror + anyhow |
| CLI | clap 4 |
| Containers | Docker + Docker Compose |
| CI | GitHub Actions |

---

## Project Structure

```
jobbit/
├── src/
│   ├── main.rs                  # Entry point, CLI dispatch, server/worker startup
│   ├── config.rs                # Environment variable loading
│   ├── db.rs                    # Database pool creation and migration runner
│   ├── error.rs                 # AppError type with HTTP response conversion
│   ├── models.rs                # Job struct, JobStatus enum, request/response types
│   ├── routes/
│   │   ├── mod.rs               # Router construction
│   │   ├── health.rs            # GET /health
│   │   ├── jobs.rs              # Job CRUD endpoints
│   │   └── metrics.rs           # GET /api/metrics
│   ├── repository/
│   │   └── job_repository.rs    # All SQL queries (insert, select, update)
│   ├── services/
│   │   ├── job_service.rs       # Business logic for job management
│   │   └── metrics_service.rs   # Metrics aggregation
│   └── worker/
│       ├── mod.rs
│       ├── processor.rs         # Main polling loop with graceful shutdown
│       ├── handlers.rs          # Simulated job type handlers
│       └── backoff.rs           # Exponential backoff calculation
├── migrations/
│   └── 001_create_jobs_table.sql
├── Dockerfile
├── docker-compose.yml
├── .env.example
└── .github/workflows/ci.yml
```

---

## Database Schema

```sql
CREATE TYPE job_status AS ENUM (
    'queued', 'running', 'completed', 'failed', 'dead_lettered'
);

CREATE TABLE jobs (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_type     TEXT NOT NULL,
    payload      JSONB NOT NULL DEFAULT '{}',
    status       job_status NOT NULL DEFAULT 'queued',
    attempts     INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    run_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    locked_at    TIMESTAMPTZ,
    locked_by    TEXT,
    last_error   TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ
);
```

The `updated_at` column is maintained by a PostgreSQL trigger, so it always reflects the last modification time without extra application code.

---

## Job Lifecycle

```
          submit
            │
            ▼
         queued  ◀─────────────────────────────────┐
            │                                       │
    worker claims                              backoff retry
            │                                       │
            ▼                                       │
         running                              (attempts < max)
            │                                       │
     ┌──────┴──────┐                                │
  success        failure ───────────────────────────┘
     │               │
     ▼           (attempts >= max)
  completed           │
                      ▼
                dead_lettered
```

---

## Worker Behavior

1. Polls PostgreSQL every `WORKER_POLL_INTERVAL_MS` milliseconds (default 2 s).
2. Claims jobs using `SELECT ... FOR UPDATE SKIP LOCKED` — safe for multiple concurrent workers.
3. Dispatches to the matching handler based on `job_type`.
4. On **success**: sets `status = completed`, `completed_at = now()`.
5. On **failure**:
   - Increments `attempts`.
   - If `attempts < max_attempts`: sets `status = queued`, schedules `run_at` in the future using exponential backoff, stores the error in `last_error`.
   - If `attempts >= max_attempts`: sets `status = dead_lettered`.
6. Responds to Ctrl-C (SIGINT) with graceful shutdown — finishes the current job before stopping.

Supported job types:

| job_type | Simulates |
|---|---|
| `send_email` | Email delivery (~20% failure rate) |
| `process_file` | File processing (~15% failure rate) |
| `run_webhook` | HTTP webhook delivery (~10% failure rate) |

To force a deterministic outcome in tests, add `"force_success": true` or `"force_fail": true` to the job payload.

---

## Retry & Backoff

The backoff formula is: `delay = base_seconds × 2^attempt` (capped at 1 hour).

With the default `base_seconds = 10`:

| Attempt | Delay |
|---|---|
| 1 | 20 seconds |
| 2 | 40 seconds |
| 3 | 80 seconds |
| 4 | 160 seconds |
| 5 | 320 seconds |
| 6+ | 640 seconds (cap) |

---

## API Endpoints

| Method | Path | Description |
|---|---|---|
| GET | `/health` | Liveness check |
| POST | `/api/jobs` | Submit a new job |
| GET | `/api/jobs` | List all jobs (summary) |
| GET | `/api/jobs/:id` | Get one job by UUID |
| GET | `/api/jobs/status/:status` | Filter jobs by status |
| GET | `/api/dead-letter` | List dead-lettered jobs |
| GET | `/api/metrics` | Queue health snapshot |

---

## Sample curl Requests

### Health check

```bash
curl http://localhost:3000/health
```

### Submit a job

```bash
curl -s -X POST http://localhost:3000/api/jobs \
  -H "Content-Type: application/json" \
  -d '{
    "job_type": "send_email",
    "payload": { "to": "alice@example.com", "subject": "Welcome!" },
    "max_attempts": 3
  }' | jq
```

### Submit a guaranteed-success job (useful for demos)

```bash
curl -s -X POST http://localhost:3000/api/jobs \
  -H "Content-Type: application/json" \
  -d '{
    "job_type": "process_file",
    "payload": { "file_path": "/data/report.csv", "force_success": true }
  }' | jq
```

### Submit a job that will always fail (to demo dead-lettering)

```bash
curl -s -X POST http://localhost:3000/api/jobs \
  -H "Content-Type: application/json" \
  -d '{
    "job_type": "run_webhook",
    "payload": { "url": "https://example.com/hook", "force_fail": true },
    "max_attempts": 2
  }' | jq
```

### List all jobs

```bash
curl -s http://localhost:3000/api/jobs | jq
```

### Get a specific job

```bash
curl -s http://localhost:3000/api/jobs/<uuid> | jq
```

### Filter by status

```bash
curl -s http://localhost:3000/api/jobs/status/queued | jq
curl -s http://localhost:3000/api/jobs/status/completed | jq
curl -s http://localhost:3000/api/jobs/status/dead_lettered | jq
```

### Dead-letter queue

```bash
curl -s http://localhost:3000/api/dead-letter | jq
```

### Metrics

```bash
curl -s http://localhost:3000/api/metrics | jq
```

Example response:

```json
{
  "total_jobs": 42,
  "queued_jobs": 5,
  "running_jobs": 1,
  "completed_jobs": 30,
  "failed_jobs": 3,
  "dead_lettered_jobs": 3,
  "avg_processing_seconds": 0.312
}
```

---

## Setup & Running Locally

### Prerequisites

- [Rust](https://rustup.rs/) (stable, 1.75+)
- [Docker](https://www.docker.com/) + Docker Compose
- `sqlx-cli` for running migrations manually: `cargo install sqlx-cli --no-default-features --features postgres`

### Option A — Docker Compose (recommended)

```bash
# Clone the repo
git clone <your-repo-url> jobbit && cd jobbit

# Start PostgreSQL, API server, and worker together
docker compose up --build
```

The API is available at `http://localhost:3000`.

### Option B — Run locally without Docker

```bash
# 1. Start PostgreSQL (or point DATABASE_URL at an existing instance)
docker run -d \
  -e POSTGRES_USER=jobbit \
  -e POSTGRES_PASSWORD=jobbit \
  -e POSTGRES_DB=jobbit \
  -p 5432:5432 postgres:16-alpine

# 2. Create .env
cp .env.example .env

# 3. Run migrations
sqlx migrate run --source migrations

# 4. Start the API server (terminal 1)
cargo run -- api

# 5. Start the worker (terminal 2)
cargo run -- worker
```

---

## Running Tests

```bash
cargo test
```

Unit tests cover:

- `backoff.rs`: delay growth, capping behaviour, future timestamps
- `handlers.rs`: force_success/force_fail determinism, unknown job types
- `models.rs`: `JobStatus` round-trip serialization, `CreateJobRequest` defaults

---

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | *(required)* | PostgreSQL connection string |
| `HOST` | `0.0.0.0` | API server bind host |
| `PORT` | `3000` | API server port |
| `DB_POOL_SIZE` | `5` | Max DB connections in pool |
| `WORKER_POLL_INTERVAL_MS` | `2000` | Worker polling frequency (ms) |
| `WORKER_ID` | `worker-<PID>` | Unique worker identifier |
| `RUST_LOG` | `info` | Log level filter |

---

## Future Improvements

- **Job priorities** — add a `priority` column and sort the worker query by `(priority DESC, run_at ASC)`.
- **Job cancellation** — add a `DELETE /api/jobs/:id` endpoint that sets `status = cancelled`.
- **Scheduled/cron jobs** — a recurring job that re-enqueues itself on completion.
- **Real job handlers** — replace the simulated handlers with actual SMTP, S3, and HTTP client calls.
- **Concurrency** — spawn multiple Tokio tasks in the worker to process N jobs in parallel.
- **Prometheus metrics** — expose `/metrics` in the Prometheus text format for scraping.
- **Admin UI** — a minimal web dashboard to view and requeue jobs.
- **Integration tests** — spin up a test database and verify the full job lifecycle end-to-end.
