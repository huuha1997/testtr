FROM rust:1.88-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.toml
COPY crates crates
COPY services/mcp-gateway services/mcp-gateway
COPY services/orchestrator services/orchestrator
COPY workers/ci workers/ci
COPY workers/deploy workers/deploy
COPY workers/design workers/design
COPY workers/spec workers/spec
COPY workers/codegen workers/codegen

RUN cargo build -p mcp-gateway --release

FROM debian:bookworm-slim

RUN adduser --disabled-password --gecos '' appuser
USER appuser

WORKDIR /app
COPY --from=builder /app/target/release/mcp-gateway /app/mcp-gateway

ENV HTTP_ADDR=0.0.0.0:8090
EXPOSE 8090

CMD ["/app/mcp-gateway"]
