FROM rust:1.88-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.toml
COPY apps/console apps/console
COPY crates crates
COPY services/mcp-gateway services/mcp-gateway
COPY services/orchestrator services/orchestrator
COPY workers/ci workers/ci
COPY workers/codegen workers/codegen
COPY workers/deploy workers/deploy
COPY workers/design workers/design
COPY workers/spec workers/spec

RUN cargo build -p deploy-worker --release

FROM debian:bookworm-slim

RUN adduser --disabled-password --gecos '' appuser
USER appuser

WORKDIR /app
COPY --from=builder /app/target/release/deploy-worker /app/deploy-worker

CMD ["/app/deploy-worker"]
