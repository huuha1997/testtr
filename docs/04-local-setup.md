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
- `VERCEL_API_TOKEN` (cho `mcp-gateway` gọi Vercel API thật)
- `VERCEL_API_BASE` (optional, default `https://api.vercel.com`)
- `VERCEL_HTTP_TIMEOUT_SECONDS` (optional, default `25`)
- `VERCEL_MAX_RETRIES` (optional, default `3`)
- `MCP_INTERNAL_API_KEY` (optional; bật auth giữa workers và `mcp-gateway`)
- `MCP_PROVIDER_HTTP_TIMEOUT_SECONDS` (optional, default `25`)
- `MCP_PROVIDER_MAX_RETRIES` (optional, default `3`)
- `BANANA_API_URL`, `BANANA_API_TOKEN` (design provider)
- `STITCH_API_URL`, `STITCH_API_TOKEN` (spec provider)
- `CLAUDE_API_URL`, `CLAUDE_API_TOKEN` (codegen provider)
- `BANANA_API_KEY_HEADER` (default `x-goog-api-key`)
- `STITCH_API_KEY_HEADER` (default `x-api-key`)
- `CLAUDE_API_KEY_HEADER` (default `x-api-key`)
- `BANANA_API_EXTRA_HEADERS_JSON` (optional JSON object)
- `STITCH_API_EXTRA_HEADERS_JSON` (optional JSON object)
- `CLAUDE_API_EXTRA_HEADERS_JSON` (optional JSON object; ví dụ `{"anthropic-version":"2023-06-01"}`)
- `GITHUB_API_BASE` (optional, default `https://api.github.com`)
- `GITHUB_API_TOKEN` (cho tạo PR thật ở gateway)
- `MCP_GATEWAY_URL` (cho `deploy-worker`, default `http://localhost:8090`)
- `DEPLOY_HTTP_TIMEOUT_SECONDS` (optional, default `20`)
- `DEPLOY_VERCEL_PROJECT_NAME` (optional, default `agentic-preview`)
- `DEPLOY_VERCEL_TEAM_ID` (optional)
- `DEPLOY_VERCEL_SLUG` (optional)
- `DEPLOY_GITHUB_OWNER`, `DEPLOY_GITHUB_REPO`
- `DEPLOY_GITHUB_BASE_BRANCH` (optional, default `main`)
- `DEPLOY_GITHUB_HEAD_PREFIX` (optional, default `preview/run-`)
- `CI_DEPLOY_VERCEL_PROJECT_NAME` (optional, default `agentic-preview`)

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

## MCP Gateway Providers
- `POST /mcp/design/generate`: proxy tới Banana API URL cấu hình.
- `POST /mcp/spec/extract`: proxy tới Stitch API URL cấu hình.
- `POST /mcp/codegen/run`: proxy tới Claude API URL cấu hình.
- `POST /mcp/repo/create-pr`: gọi GitHub create PR thật.
- Nếu đặt `MCP_INTERNAL_API_KEY`, mọi endpoint `/mcp/*` yêu cầu header `x-internal-api-key`.

## Vercel Deploy API (gateway)
- Endpoint: `POST /mcp/deploy/vercel`
- Request:
  - `team_id` (optional)
  - `slug` (optional)
  - `deployment` (required): object body gửi thẳng tới `POST /v13/deployments` của Vercel.
- Response:
  - `deployment_id`
  - `ready_state`
  - `deployment_url`
  - `inspector_url`
  - `attempt_count`
  - `raw`

## Deploy Worker -> Gateway
- `deploy-worker` gọi `POST {MCP_GATEWAY_URL}/mcp/deploy/vercel` ở bước deploy.
- `ci-worker` khi pass gates sẽ enqueue payload đã có sẵn `deployment` object.
- Nếu payload job thiếu `deployment`, `deploy-worker` vẫn có fallback static deployment tối thiểu để đảm bảo luồng end-to-end chạy được.
- `deploy-worker` sẽ gọi thêm `POST {MCP_GATEWAY_URL}/mcp/repo/create-pr` nếu có đủ config GitHub.
- `run_steps.preview_deploy.detail` sẽ chứa:
  - `deployment_id`
  - `ready_state`
  - `preview_url`
  - `inspector_url`

## Smoke Checklist
- API health trả về `ok`
- SSE stream nhận được heartbeat
- Redis/Postgres kết nối thành công
