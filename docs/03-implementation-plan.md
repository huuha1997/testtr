# Implementation Plan

## Current Status
- Phase 1 hoàn thành.
- Phase 2 hoàn thành: OAuth start/callback, token encryption at-rest, refresh/revoke, token masking logs + audit tập trung.
- Phase 3 hoàn thành: run transitions, run_steps, queue crate, design/spec/codegen/ci/deploy workers, idempotency lock.
- Phase 4 hoàn thành phần skeleton `mcp-gateway` endpoints.
- Phase 5 hoàn thành theo hướng backend API (không Slint): timeline/logs SSE + approve/reject deploy.
- Phase 6 hoàn thành: CI gates, self-heal loop có giới hạn, metrics/audit endpoints.

## Phase 1 - Foundation
- Tạo workspace Rust + cấu trúc thư mục services/workers/crates.
- Dựng local stack bằng Docker Compose.
- Thiết lập config loader, tracing, error model chuẩn.
- Tạo endpoint health và readiness.

## Phase 2 - Auth & Connections
- Workspace auth + RBAC.
- Kết nối Banana, Stitch, Claude, GitHub, Vercel.
- Token broker: encrypt, refresh, revoke, scope validation.

## Phase 3 - Orchestrator & Queue
- State machine cho vòng đời run.
- Redis Streams wrapper + retry + DLQ + idempotency lock.
- Lưu artifacts và step statuses.

## Phase 4 - MCP Adapters
- Adapter thống nhất input/output cho Banana, Stitch, Claude.
- Adapter GitHub PR + Vercel deploy.
- Chuẩn hóa mã lỗi và retry policy theo provider.

## Phase 5 - Console APIs (non-Slint)
- Chat/mockup/stack flow qua API hiện có.
- Timeline + logs realtime qua SSE.
- Approve/reject deploy qua API.

## Phase 6 - CI, Self-Heal, Hardening
- Chạy lint/type/build/e2e/visual/a11y gates.
- Self-heal loop có giới hạn.
- Dashboard metrics/logs/traces và audit.

## MVP Definition of Done
- Chạy end-to-end từ brief tới production URL.
- Có PR + preview URL trước production.
- Có audit trace đầy đủ mỗi run.

## Next Execution Queue
- Phase 4: nối adapter thật tới Banana/Stitch/Claude/GitHub/Vercel.
