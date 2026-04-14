# TestTR

> Self-hosted agentic design-to-code platform — from prompt to production in one pipeline.

![Rust](https://img.shields.io/badge/Rust-Tokio+Axum-orange?logo=rust)
![Next.js](https://img.shields.io/badge/Next.js-16-black?logo=next.js)
![Redis](https://img.shields.io/badge/Queue-Redis_Streams-red?logo=redis)
![PostgreSQL](https://img.shields.io/badge/DB-PostgreSQL-blue?logo=postgresql)

## Overview

End-to-end design-to-production automation system. Converts design prompts to frontend code through AI mockup generation, design spec creation, code generation, quality gates, and automated deployment with PR creation.

## Architecture

```
                    ┌──────────────┐
                    │  Console UI  │  Slint desktop app
                    │  (mockup     │  Chat, picker, timeline
                    │   picker)    │
                    └──────┬───────┘
                           │
┌──────────┐    ┌──────────▼───────────┐    ┌─────────────┐
│  Web UI  │───▶│    Orchestrator      │◀──▶│ MCP Gateway │
│ Next.js  │    │  State Machine + SSE │    │ Banana/Stitch│
└──────────┘    └──┬───┬───┬───┬───┬──┘    │ Claude/GH   │
                   │   │   │   │   │       └─────────────┘
              ┌────▼┐┌─▼──┐│┌──▼─┐┌▼────┐
              │Design││Spec│││Code││Deploy│
              │Worker││    │││Gen ││Worker│
              └─────┘└────┘│└────┘└─────┘
                       ┌───▼──┐
                       │  CI  │  Quality gates
                       │Worker│  lint/type/build/test
                       └──────┘
```

## Pipeline

```
Prompt → Mockups (3 variants) → User Selection → Spec → Code → Quality Gates → Deploy → PR
```

## Key Principles

- **Deterministic** — Generation contracts lock mockup + tech stack choices
- **User-driven** — Selection gates before code generation
- **Self-correcting** — Quality gates with auto-correction within thresholds
- **Auditable** — Complete audit trail for all state transitions
- **Secure** — Encrypted provider tokens with scope limiting

## Tech Stack

| Service | Technology |
|---------|-----------|
| Console | Slint (Rust desktop UI) |
| Web | Next.js 16 |
| Orchestrator | Rust, Axum, SQLx, PostgreSQL |
| MCP Gateway | Rust, Banana/Stitch/Claude/GitHub/Vercel |
| Queue | Redis Streams (consumer groups + DLQ) |
| Storage | MinIO |
| Observability | OpenTelemetry, Prometheus, Grafana, Loki |
| Workers | Design, Spec, Codegen, CI, Deploy |

## Project Structure

```
apps/
├── console/             # Slint desktop UI
└── web/                 # Next.js dashboard
services/
├── orchestrator/        # State machine + REST + SSE
└── mcp-gateway/         # MCP adapter for external services
workers/
├── design/              # Banana mockup generation (3 A/B/C)
├── spec/                # Stitch design spec generation
├── codegen/             # Claude Code integration
├── ci/                  # Quality gates (lint, type, build, test)
└── deploy/              # PR creation + preview/prod deploy
crates/
├── contracts/           # Shared type definitions
└── queue/               # Redis queue implementation
```

## Getting Started

```bash
# Prerequisites: Rust, Docker, Node.js
docker compose up -d    # PostgreSQL, Redis, MinIO, Grafana

# Build all services
cargo build --release

# Run orchestrator
cargo run -p orchestrator

# Run web dashboard
cd apps/web && npm install && npm run dev
```

## License

MIT
