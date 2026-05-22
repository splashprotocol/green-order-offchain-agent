#!/usr/bin/env bash

demo_start_agent() {
  if [[ -n "${DEMO_AGENT_PID:-}" ]] && ps -p "$DEMO_AGENT_PID" >/dev/null 2>&1; then
    return 0
  fi

  echo "agent: creating partial-fill preprod config"
  (
    cd "$DEMO_E2E_DIR"
    export PARTIAL_AGENT_DB_PATH="$DEMO_RUN_DIR/agent-chain-sync"
    deno run --no-lock --allow-net --allow-read --allow-write --allow-env \
      09-create-partial-agent-config.ts --out "$DEMO_AGENT_CONFIG_FILE"
  ) 2>&1 | tee "$DEMO_LOG_DIR/agent-config.log"

  echo "agent: starting"
  (
    cd "$REPO_ROOT"
    ./target/debug/green-order-cardano-agent --config-path "$DEMO_AGENT_CONFIG_FILE"
  ) >"$DEMO_AGENT_LOG_FILE" 2>&1 &
  DEMO_AGENT_PID=$!
  export DEMO_AGENT_PID
  echo "$DEMO_AGENT_PID" >"$DEMO_RUN_DIR/agent.pid"
}

demo_stop_agent() {
  local pid="${DEMO_AGENT_PID:-}"
  if [[ -z "$pid" && -f "${DEMO_RUN_DIR:-}/agent.pid" ]]; then
    pid="$(cat "$DEMO_RUN_DIR/agent.pid")"
  fi
  if [[ -n "$pid" ]] && ps -p "$pid" >/dev/null 2>&1; then
    echo "agent: stopping pid $pid"
    kill "$pid" >/dev/null 2>&1 || true
    wait "$pid" 2>/dev/null || true
  fi
}

demo_wait_for_indexer_ready() {
  echo "agent: waiting for health endpoint"
  local deadline=$((SECONDS + ${DEMO_AGENT_READY_TIMEOUT_SECONDS:-240}))
  while (( SECONDS < deadline )); do
    if curl -sS "${AGENT_HEALTH_URL:-http://127.0.0.1:9024/health}" >/dev/null 2>&1; then
      echo "agent: health endpoint responded"
      demo_checkpoint_done "agent_ready"
      return 0
    fi
    sleep 5
  done
  echo "agent did not become healthy; log: $DEMO_AGENT_LOG_FILE" >&2
  tail -n 80 "$DEMO_AGENT_LOG_FILE" >&2 || true
  return 1
}
