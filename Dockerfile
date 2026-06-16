# ---- Build stage ----
# Use the official Rust image to compile the project.
# Separating build from runtime keeps the final image small.
FROM rust:1.78-slim AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Copy manifests first so Docker can cache the dependency download layer.
# If only source files change, we don't re-download all crates.
COPY Cargo.toml Cargo.lock ./

# Create a dummy main so cargo can compile dependencies without our real source
RUN mkdir src && echo 'fn main() {}' > src/main.rs
RUN cargo build --release
RUN rm -f src/main.rs

# Now copy the real source and migrations
COPY src ./src
COPY migrations ./migrations

# Touch main.rs so Cargo knows to recompile it
RUN touch src/main.rs
RUN cargo build --release

# ---- Runtime stage ----
# Use a minimal Debian image — no Rust toolchain needed at runtime
FROM debian:bookworm-slim

WORKDIR /app

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

# Copy the compiled binary from the build stage
COPY --from=builder /app/target/release/jobbit /usr/local/bin/jobbit

# Copy migrations so the binary can run them at startup
COPY --from=builder /app/migrations ./migrations

EXPOSE 3000

# Default command — can be overridden to "worker" in docker-compose.yml
CMD ["jobbit", "api"]
