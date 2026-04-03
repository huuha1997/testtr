#!/usr/bin/env bash
# run-local.sh — Chạy toàn bộ stack qua cargo (không cần docker-compose)
# Yêu cầu: PostgreSQL và Redis đang chạy sẵn (local hoặc docker container)
# Usage: ./scripts/run-local.sh [--release]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="$REPO_ROOT/.env"
CARGO_FLAGS=""

if [[ "${1:-}" == "--release" ]]; then
    CARGO_FLAGS="--release"
    echo "[run-local] Building in release mode"
fi

# ── Load .env ────────────────────────────────────────────────────────────────
if [[ -f "$ENV_FILE" ]]; then
    set -o allexport
    # shellcheck disable=SC1090
    source "$ENV_FILE"
    set +o allexport
    echo "[run-local] Loaded $ENV_FILE"
else
    echo "[run-local] WARNING: .env not found, proceeding with existing env vars"
fi

# ── Kiểm tra postgres và redis ───────────────────────────────────────────────
DB_URL="${DATABASE_URL:-postgres://postgres:postgres@localhost:5432/agentic}"
REDIS_URL="${REDIS_URL:-redis://localhost:6380}"

echo "[run-local] Checking PostgreSQL at $DB_URL ..."
if ! psql "$DB_URL" -c '\q' 2>/dev/null; then
    echo "[run-local] ERROR: Cannot connect to PostgreSQL ($DB_URL)"
    echo "           Hãy đảm bảo PostgreSQL đang chạy và DATABASE_URL đúng."
    exit 1
fi

REDIS_HOST=$(echo "$REDIS_URL" | sed -E 's|redis://([^:]+):.*|\1|')
REDIS_PORT=$(echo "$REDIS_URL" | sed -E 's|redis://[^:]+:([0-9]+).*|\1|')
echo "[run-local] Checking Redis at $REDIS_HOST:$REDIS_PORT ..."
_redis_ok=false
if command -v redis-cli &>/dev/null && redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" ping 2>/dev/null | grep -q PONG; then
    _redis_ok=true
elif docker ps --format '{{.Ports}}' 2>/dev/null | grep -q "${REDIS_PORT}->"; then
    _redis_ok=true
fi
if ! $_redis_ok; then
    echo "[run-local] ERROR: Cannot connect to Redis ($REDIS_URL)"
    echo "           Hãy đảm bảo Redis đang chạy và REDIS_URL đúng."
    exit 1
fi

# Migrations chạy tự động khi orchestrator start (sqlx::migrate! embedded)

# ── Kill processes cũ còn giữ ports ─────────────────────────────────────────
for port in 8080 8090; do
    pid=$(lsof -ti:$port 2>/dev/null || true)
    if [[ -n "$pid" ]]; then
        echo "[run-local] Killing stale process on port $port (PID $pid)"
        kill -9 $pid 2>/dev/null || true
    fi
done
pkill -f "target/debug/orchestrator"  2>/dev/null || true
pkill -f "target/debug/mcp-gateway"   2>/dev/null || true
pkill -f "target/debug/design-worker" 2>/dev/null || true
pkill -f "target/debug/spec-worker"   2>/dev/null || true
pkill -f "target/debug/codegen-worker" 2>/dev/null || true
pkill -f "target/debug/ci-worker"     2>/dev/null || true
pkill -f "target/debug/stitch-worker" 2>/dev/null || true
pkill -f "target/debug/deploy-worker" 2>/dev/null || true
sleep 1

# ── Build tất cả services ────────────────────────────────────────────────────
echo "[run-local] Building all packages (this may take a while)..."
(cd "$REPO_ROOT" && cargo build $CARGO_FLAGS \
    -p orchestrator \
    -p mcp-gateway \
    -p design-worker \
    -p stitch-worker \
    -p spec-worker \
    -p codegen-worker \
    -p ci-worker \
    -p deploy-worker)
echo "[run-local] Build done."

# ── Quản lý PIDs để cleanup khi thoát ───────────────────────────────────────
PIDS=()

cleanup() {
    echo ""
    echo "[run-local] Stopping all services..."
    for pid in "${PIDS[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null || true
        fi
    done
    wait 2>/dev/null || true
    echo "[run-local] All services stopped."
}
trap cleanup INT TERM EXIT

# ── Helper: start a service ───────────────────────────────────────────────────
LOG_DIR="$REPO_ROOT/logs"
mkdir -p "$LOG_DIR"

start_service() {
    local name="$1"
    local package="$2"
    local log="$LOG_DIR/${name}.log"
    shift 2
    # Remaining args are extra env vars, e.g. HTTP_ADDR=0.0.0.0:8090

    echo "[run-local] Starting $name → $log"
    (cd "$REPO_ROOT" && env "$@" cargo run $CARGO_FLAGS -p "$package" 2>&1) >> "$log" &
    local pid=$!
    PIDS+=("$pid")
    echo "[run-local] $name PID=$pid"
}

# ── Start services ────────────────────────────────────────────────────────────
# Orchestrator trước, workers sau
start_service "orchestrator"    "orchestrator"
sleep 2   # chờ orchestrator bind port

start_service "mcp-gateway"     "mcp-gateway"  HTTP_ADDR=0.0.0.0:8090
sleep 1

start_service "design-worker"   "design-worker"
start_service "stitch-worker"   "stitch-worker"
start_service "spec-worker"     "spec-worker"
start_service "codegen-worker"  "codegen-worker"
start_service "ci-worker"       "ci-worker"
start_service "deploy-worker"   "deploy-worker"

echo ""
echo "══════════════════════════════════════════════════"
echo " Agentic stack đang chạy:"
echo "   Orchestrator  → http://localhost:${HTTP_ADDR##*:}"
echo "   MCP Gateway   → http://localhost:8090"
echo " Logs: $LOG_DIR/"
echo " Nhấn Ctrl+C để dừng tất cả."
echo "══════════════════════════════════════════════════"

# ── Tail logs ra terminal ─────────────────────────────────────────────────────
tail -f \
    "$LOG_DIR/orchestrator.log" \
    "$LOG_DIR/mcp-gateway.log" \
    "$LOG_DIR/design-worker.log" \
    "$LOG_DIR/stitch-worker.log" \
    "$LOG_DIR/spec-worker.log" \
    "$LOG_DIR/codegen-worker.log" \
    "$LOG_DIR/ci-worker.log" \
    "$LOG_DIR/deploy-worker.log" \
    2>/dev/null &
PIDS+=($!)

# Chờ cho đến khi bị interrupt (monitor — exit nếu orchestrator chết)
while true; do
    sleep 5
    if ! kill -0 "${PIDS[0]}" 2>/dev/null; then
        echo "[run-local] Orchestrator exited — stopping all services."
        break
    fi
done
