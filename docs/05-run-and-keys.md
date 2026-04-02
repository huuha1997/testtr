# Runbook: Chạy Dự Án + Cấu Hình Keys

## 1) Mục tiêu file này
- Cách chạy project local.
- Cách lấy và set các key bắt buộc:
  - `BANANA_*`
  - `STITCH_*`
  - `CLAUDE_*`
  - `GITHUB_API_TOKEN`
  - `VERCEL_API_TOKEN`
  - env GitHub deploy (`DEPLOY_GITHUB_*`)

## 2) Prerequisites
- Docker Desktop
- Rust stable
- `cargo`
- `curl`

## 3) Tạo file `.env` tại root project
Tạo file `.env` ở thư mục gốc repo và điền theo mẫu:

```bash
# ===== Core =====
RUST_LOG=info
MCP_INTERNAL_API_KEY=replace-with-strong-random-key

# ===== Provider upstream endpoints =====
# Dùng endpoint thật của provider bạn có account.
BANANA_API_URL=https://<banana-provider-endpoint>
STITCH_API_URL=https://<stitch-provider-endpoint>
CLAUDE_API_URL=https://<claude-provider-endpoint>

# ===== Provider API tokens =====
BANANA_API_TOKEN=<banana-api-token>
STITCH_API_TOKEN=<stitch-api-token>
CLAUDE_API_TOKEN=<claude-api-token>
BANANA_API_KEY_HEADER=x-goog-api-key
STITCH_API_KEY_HEADER=X-Goog-Api-Key
CLAUDE_API_KEY_HEADER=x-api-key
BANANA_API_EXTRA_HEADERS_JSON=
STITCH_API_EXTRA_HEADERS_JSON=
CLAUDE_API_EXTRA_HEADERS_JSON={"anthropic-version":"2023-06-01"}
CLAUDE_MODEL=claude-3-5-sonnet-latest
CLAUDE_MAX_TOKENS=1024
STITCH_MCP_TOOL_NAME=extract_spec

# ===== GitHub + Vercel =====
GITHUB_API_TOKEN=<github-personal-access-token>
VERCEL_API_TOKEN=<vercel-access-token>

# ===== Deploy worker GitHub PR defaults =====
DEPLOY_GITHUB_OWNER=<github-org-or-user>
DEPLOY_GITHUB_REPO=<repo-name>
DEPLOY_GITHUB_BASE_BRANCH=main
DEPLOY_GITHUB_HEAD_PREFIX=preview/run-

# Optional
DEPLOY_VERCEL_TEAM_ID=
DEPLOY_VERCEL_SLUG=
CI_DEPLOY_VERCEL_PROJECT_NAME=agentic-preview
DEPLOY_VERCEL_PROJECT_NAME=agentic-preview
```

Lưu ý:
- `MCP_INTERNAL_API_KEY` phải giống nhau giữa `mcp-gateway` và các workers.
- Không commit `.env`.

## 4) Cách lấy các keys

### 4.1 Banana / Stitch / Claude (`*_API_TOKEN`)
- Đăng nhập dashboard provider tương ứng.
- Vào phần API Keys / Developer / Tokens.
- Tạo token mới với scope tối thiểu cần thiết (principle of least privilege).
- Copy token vào biến:
  - `BANANA_API_TOKEN`
  - `STITCH_API_TOKEN`
  - `CLAUDE_API_TOKEN`
- Lấy endpoint API tương ứng và set:
  - `BANANA_API_URL`
  - `STITCH_API_URL`
  - `CLAUDE_API_URL`

### 4.2 GitHub (`GITHUB_API_TOKEN`)
- Vào GitHub Settings -> Developer settings -> Personal access tokens.
- Khuyến nghị dùng Fine-grained token, scope tối thiểu cho repo đích:
  - Pull requests: Read and write
  - Contents: Read (hoặc Read/Write nếu flow của bạn cần)
  - Metadata: Read
- Set vào `GITHUB_API_TOKEN`.

### 4.3 Vercel (`VERCEL_API_TOKEN`)
- Vào Vercel Dashboard -> Account Settings -> Tokens.
- Tạo token mới.
- Set vào `VERCEL_API_TOKEN`.

## 5) Env GitHub deploy (quan trọng)
Các biến này quyết định PR mà `deploy-worker` tạo:
- `DEPLOY_GITHUB_OWNER`: org/user trên GitHub.
- `DEPLOY_GITHUB_REPO`: tên repo.
- `DEPLOY_GITHUB_BASE_BRANCH`: nhánh đích PR (thường `main`).
- `DEPLOY_GITHUB_HEAD_PREFIX`: prefix branch nguồn (ví dụ `preview/run-`).

Ví dụ branch tạo ra: `preview/run-<run_id_8_chars>`.

## 6) Chạy hệ thống local

### 6.1 Chạy backend stack
```bash
docker compose -f infra/compose/docker-compose.yml up --build
```

### 6.2 Health check
```bash
curl -sS http://localhost:8080/healthz
curl -sS http://localhost:8090/healthz
```

## 7) Chạy console
```bash
cargo run -p console
```

## 8) Chạy smoke e2e
Repo có sẵn script:
```bash
./scripts/e2e-smoke.sh
```

Có thể override:
```bash
API_BASE=http://localhost:8080 TIMEOUT_SECONDS=240 ./scripts/e2e-smoke.sh
```

## 9) Checklist nhanh khi lỗi
- `mcp-gateway` báo `invalid internal api key`:
  - kiểm tra `MCP_INTERNAL_API_KEY` có đồng nhất ở gateway + workers.
- `mcp-gateway` báo thiếu token:
  - kiểm tra `BANANA_API_TOKEN`, `STITCH_API_TOKEN`, `CLAUDE_API_TOKEN`, `GITHUB_API_TOKEN`, `VERCEL_API_TOKEN`.
- Tạo PR fail:
  - kiểm tra `DEPLOY_GITHUB_OWNER/REPO/BASE_BRANCH` đúng repo thật.
- Vercel deploy fail:
  - kiểm tra `VERCEL_API_TOKEN` hợp lệ và quyền team/project.
