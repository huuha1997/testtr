# Architecture

## Stack
- Console UI: Rust + Slint
- Backend: Rust (Axum + Tokio + SQLx)
- Queue: Redis Streams + Consumer Groups
- DB: PostgreSQL
- Artifact Store: MinIO
- Observability: OpenTelemetry + Prometheus + Grafana + Loki
- Runtime local: Docker Compose

## Service Boundaries
- `apps/console`: Slint desktop UI cho chat, mockup picker, stack wizard, run timeline.
- `services/orchestrator`: workflow state machine, API, SSE.
- `services/mcp-gateway`: adapter Banana/Stitch/Claude/GitHub/Vercel.
- `workers/design`: gọi Banana tạo mockup.
- `workers/spec`: gọi Stitch tạo design spec.
- `workers/codegen`: gọi Claude Code tạo patch code.
- `workers/ci`: lint/type/build/test/visual/a11y.
- `workers/deploy`: tạo PR và deploy preview/prod.

## Queue Topology
- Streams: `q.design`, `q.spec`, `q.codegen`, `q.ci`, `q.deploy`
- Groups: `cg.design`, `cg.spec`, `cg.codegen`, `cg.ci`, `cg.deploy`
- DLQ: `q.design.dlq`, `q.spec.dlq`, `q.codegen.dlq`, `q.ci.dlq`, `q.deploy.dlq`

## Core Entities
- `workspaces`, `users`, `workspace_members`
- `connections`, `oauth_tokens`
- `projects`, `runs`, `run_steps`
- `artifacts`
- `pull_requests`, `deployments`
- `audit_logs`
