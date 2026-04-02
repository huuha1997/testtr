#!/usr/bin/env bash
set -euo pipefail

API_BASE="${API_BASE:-http://localhost:8080}"
PROMPT="${PROMPT:-Build a clean landing page for a fintech startup}"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-180}"
POLL_INTERVAL_SECONDS="${POLL_INTERVAL_SECONDS:-3}"

echo "== create run =="
CREATE_RES="$(curl -sS -X POST "$API_BASE/api/runs" \
  -H 'content-type: application/json' \
  -d "{\"prompt\":\"$PROMPT\"}")"
RUN_ID="$(printf "%s" "$CREATE_RES" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')"
if [[ -z "${RUN_ID}" ]]; then
  echo "failed to extract run id from: $CREATE_RES"
  exit 1
fi
echo "run_id=$RUN_ID"

echo "== select mockup =="
curl -sS -X POST "$API_BASE/api/runs/$RUN_ID/select-mockup" \
  -H 'content-type: application/json' \
  -d '{"mockup_id":"A"}' >/dev/null

echo "== select stack =="
curl -sS -X POST "$API_BASE/api/runs/$RUN_ID/select-stack" \
  -H 'content-type: application/json' \
  -d '{"stack_id":"nextjs-tailwind"}' >/dev/null

echo "== poll status =="
START_TS="$(date +%s)"
while true; do
  RUN_RES="$(curl -sS "$API_BASE/api/runs/$RUN_ID")"
  STATUS="$(printf "%s" "$RUN_RES" | sed -n 's/.*"status":"\([^"]*\)".*/\1/p')"
  echo "status=$STATUS"
  if [[ "$STATUS" == "awaiting_approval" || "$STATUS" == "preview_deployed" ]]; then
    break
  fi
  NOW_TS="$(date +%s)"
  if (( NOW_TS - START_TS > TIMEOUT_SECONDS )); then
    echo "timeout waiting preview status"
    exit 1
  fi
  sleep "$POLL_INTERVAL_SECONDS"
done

echo "== approve deploy =="
curl -sS -X POST "$API_BASE/api/runs/$RUN_ID/approve-deploy" \
  -H 'content-type: application/json' >/dev/null

echo "== final run =="
curl -sS "$API_BASE/api/runs/$RUN_ID"
echo
echo "e2e smoke done"
