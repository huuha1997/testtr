FROM rust:1.88-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.toml
COPY crates crates
COPY services/mcp-gateway services/mcp-gateway
COPY services/orchestrator services/orchestrator
COPY workers/ci workers/ci
COPY workers/codegen workers/codegen
COPY workers/deploy workers/deploy
COPY workers/design workers/design
COPY workers/spec workers/spec

RUN cargo build -p ci-worker --release

FROM debian:bookworm-slim

RUN adduser --disabled-password --gecos '' appuser
USER appuser

WORKDIR /app
COPY --from=builder /app/target/release/ci-worker /app/ci-worker

CMD ["/app/ci-worker"]
