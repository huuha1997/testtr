# Local Setup

## Prerequisites
- Docker Desktop
- Rust stable
- Node.js 22+
- pnpm hoặc npm

## Services local
- postgres
- redis
- minio
- orchestrator
- mcp-gateway
- design-worker
- spec-worker
- codegen-worker
- ci-worker
- deploy-worker

## Chạy local bằng Docker
- `docker compose -f infra/compose/docker-compose.yml up --build`
- API: `http://localhost:8080`
- MCP Gateway: `http://localhost:8090`
- Redis (host): `redis://localhost:6380`

## Chạy Slint Console
- `cargo run -p console`
- Mặc định console gọi vào `http://localhost:8080`
- Các thao tác có sẵn:
  - Create run
  - Refresh run
  - Xem steps/timeline
  - Start/Stop SSE realtime (`/api/runs/{run_id}/events`)
  - Select mockup/stack
  - Approve/Reject deploy
  - Xem connections/metrics

## Environment Variables
- `DATABASE_URL`
- `REDIS_URL`
- `RUST_LOG`
- `SSE_HEARTBEAT_SECONDS`
- `ENCRYPTION_KEY`
- `OAUTH_<PROVIDER>_CLIENT_ID`
- `OAUTH_<PROVIDER>_CLIENT_SECRET`
- `OAUTH_<PROVIDER>_AUTH_URL`
- `OAUTH_<PROVIDER>_TOKEN_URL`
- `OAUTH_<PROVIDER>_REVOKE_URL`
- `OAUTH_<PROVIDER>_REDIRECT_URL`
- `OAUTH_<PROVIDER>_SCOPES`

## SSE Convention
- Endpoint: `GET /api/runs/{run_id}/events`
- Event types:
  - `state_changed`
  - `step_log`
  - `artifact_ready`
  - `gate_result`
  - `run_failed`
  - `run_completed`

## API Hiện Có
- `POST /api/runs`
- `GET /api/runs/{run_id}`
- `POST /api/runs/{run_id}/select-mockup`
- `POST /api/runs/{run_id}/select-stack`
- `GET /api/runs/{run_id}/steps`
- `GET /api/runs/{run_id}/timeline`
- `GET /api/runs/{run_id}/events`
- `POST /api/runs/{run_id}/approve-deploy`
- `POST /api/runs/{run_id}/reject-deploy`
- `GET /api/metrics/summary`
- `GET /api/connections`
- `PUT /api/connections/{provider}`
- `DELETE /api/connections/{provider}`
- `POST /api/connections/{provider}/oauth/start`
- `POST /api/connections/{provider}/oauth/callback`
- `POST /api/connections/{provider}/refresh`
- `POST /api/connections/{provider}/revoke`
- `POST /mcp/design/generate`
- `POST /mcp/spec/extract`
- `POST /mcp/codegen/run`
- `POST /mcp/repo/create-pr`
- `POST /mcp/deploy/vercel`

## Smoke Checklist
- API health trả về `ok`
- SSE stream nhận được heartbeat
- Redis/Postgres kết nối thành công
