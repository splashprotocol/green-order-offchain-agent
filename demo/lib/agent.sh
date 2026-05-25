#!/usr/bin/env bash

demo_start_agent() {
  if [[ -n "${DEMO_AGENT_PID:-}" ]] && ps -p "$DEMO_AGENT_PID" >/dev/null 2>&1; then
    return 0
  fi

  echo "agent: creating partial-fill preprod config"
  local lookback_seconds
  lookback_seconds="$(demo_partial_chain_sync_lookback_seconds)"
  (
    cd "$DEMO_E2E_DIR"
    export PARTIAL_AGENT_DB_PATH="$DEMO_RUN_DIR/agent-chain-sync"
    export PARTIAL_CHAIN_SYNC_LOOKBACK_SECONDS="$lookback_seconds"
    export CARDANO_NODE_SOCKET_PATH="${CARDANO_NODE_SOCKET_PATH:-${DEMO_NODE_SOCKET_PATH:-}}"
    deno run --no-lock --allow-net --allow-read --allow-write --allow-env \
      09-create-partial-agent-config.ts --out "$DEMO_AGENT_CONFIG_FILE"
  ) 2>&1 | tee "$DEMO_LOG_DIR/agent-config.log"

  echo "agent: building binary"
  (cd "$REPO_ROOT" && cargo build -q -p green-order-cardano-agent --bin green-order-cardano-agent)

  echo "agent: starting"
  DEMO_AGENT_STARTED_THIS_ATTEMPT=1
  export DEMO_AGENT_STARTED_THIS_ATTEMPT
  (
    cd "$REPO_ROOT"
    ./target/debug/green-order-cardano-agent \
      --config-path "$DEMO_AGENT_CONFIG_FILE" \
      --deployment-path green-order-cardano-agent/resources/preprod.deployment.json \
      --validation-rules-path green-order-cardano-agent/resources/validation-rules.json.template \
      --log4rs-path green-order-cardano-agent/resources/log4rs.local.yaml
  ) >"$DEMO_AGENT_LOG_FILE" 2>&1 &
  DEMO_AGENT_PID=$!
  export DEMO_AGENT_PID
  echo "$DEMO_AGENT_PID" >"$DEMO_RUN_DIR/agent.pid"
}

demo_partial_chain_sync_lookback_seconds() {
  rm -rf "$DEMO_RUN_DIR/agent-chain-sync" "$DEMO_RUN_DIR/agent-chain-sync.green-account-stores.json"
  printf '%s\n' "${PARTIAL_CHAIN_SYNC_LOOKBACK_SECONDS:-${DEMO_CHAIN_SYNC_LOOKBACK_SECONDS:-14400}}"
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

demo_wait_for_agent_health() {
  echo "agent: waiting for health endpoint"
  local deadline=$((SECONDS + ${DEMO_AGENT_READY_TIMEOUT_SECONDS:-240}))
  while (( SECONDS < deadline )); do
    if ! demo_agent_process_is_running; then
      echo "agent exited before health endpoint responded; log: $DEMO_AGENT_LOG_FILE" >&2
      tail -n 80 "$DEMO_AGENT_LOG_FILE" >&2 || true
      return 1
    fi
    if curl -sS "${AGENT_HEALTH_URL:-http://127.0.0.1:9024/health}" >/dev/null 2>&1; then
      echo "agent: health endpoint responded"
      demo_checkpoint_done "agent_ready"
      demo_checkpoint_set_value "agent_ready_pid" "$(cat "$DEMO_RUN_DIR/agent.pid" 2>/dev/null || true)"
      return 0
    fi
    sleep 5
  done
  echo "agent did not become healthy; log: $DEMO_AGENT_LOG_FILE" >&2
  tail -n 80 "$DEMO_AGENT_LOG_FILE" >&2 || true
  return 1
}

demo_wait_for_indexer_ready() {
  local state="$DEMO_RUN_DIR/preprod-green-order-e2e.json"
  [[ -f "$state" ]] || {
    echo "agent readiness requires E2E state file: $state" >&2
    return 1
  }

  local account_id tx_hash output_index
  account_id="$(jq -r '.account.accountId // empty' "$state")"
  tx_hash="$(jq -r '.account.outputRef.txHash // empty' "$state")"
  output_index="$(jq -r '.account.outputRef.outputIndex // empty' "$state")"
  if [[ -z "$account_id" || -z "$tx_hash" || -z "$output_index" ]]; then
    echo "agent readiness requires account id and output ref in $state" >&2
    return 1
  fi

  echo "agent: waiting for indexed account $account_id at $tx_hash#$output_index"
  local deadline=$((SECONDS + ${DEMO_INDEXER_READY_TIMEOUT_SECONDS:-900}))
  while (( SECONDS < deadline )); do
    if ! demo_agent_process_is_running; then
      echo "agent exited while waiting for account readiness; log: $DEMO_AGENT_LOG_FILE" >&2
      tail -n 80 "$DEMO_AGENT_LOG_FILE" >&2 || true
      return 1
    fi
    local status
    status="$(
      curl -sS --get "${AGENT_URL:-http://127.0.0.1:9031}/accounts/status" \
        --data-urlencode "accountId=$account_id" \
        --data-urlencode "txHash=$tx_hash" \
        --data-urlencode "outputIndex=$output_index" 2>/dev/null || true
    )"
    if [[ -n "$status" ]] &&
      demo_agent_node_is_healthy &&
      jq -e '.status.current == true' >/dev/null 2>&1 <<<"$status"; then
      echo "agent: account readiness confirmed"
      demo_checkpoint_done "indexer_ready"
      return 0
    fi
    sleep 5
  done

  echo "agent did not index account $account_id at $tx_hash#$output_index; log: $DEMO_AGENT_LOG_FILE" >&2
  tail -n 80 "$DEMO_AGENT_LOG_FILE" >&2 || true
  return 1
}

demo_wait_for_account_observed() {
  local state="$DEMO_RUN_DIR/preprod-green-order-e2e.json"
  [[ -f "$state" ]] || {
    echo "account observation requires E2E state file: $state" >&2
    return 1
  }

  local account_id tx_hash output_index
  account_id="$(jq -r '.pendingAccount.accountId // .account.accountId // empty' "$state")"
  tx_hash="$(jq -r '.pendingAccount.outputRef.txHash // .account.outputRef.txHash // empty' "$state")"
  output_index="$(jq -r '.pendingAccount.outputRef.outputIndex // .account.outputRef.outputIndex // empty' "$state")"
  if [[ -z "$account_id" || -z "$tx_hash" || -z "$output_index" ]]; then
    echo "account observation requires pending/current account id and output ref in $state" >&2
    return 1
  fi

  echo "agent: waiting for observed account $account_id at $tx_hash#$output_index"
  local deadline=$((SECONDS + ${DEMO_INDEXER_READY_TIMEOUT_SECONDS:-900}))
  while (( SECONDS < deadline )); do
    if ! demo_agent_process_is_running; then
      echo "agent exited while waiting for account observation; log: $DEMO_AGENT_LOG_FILE" >&2
      tail -n 80 "$DEMO_AGENT_LOG_FILE" >&2 || true
      return 1
    fi
    local status
    status="$(
      curl -sS --get "${AGENT_URL:-http://127.0.0.1:9031}/accounts/status" \
        --data-urlencode "accountId=$account_id" \
        --data-urlencode "txHash=$tx_hash" \
        --data-urlencode "outputIndex=$output_index" 2>/dev/null || true
    )"
    if [[ -n "$status" ]] &&
      jq -e '.status.current == true or .status.persisted == true or .status.predicted == true or .status.unbound == true' >/dev/null 2>&1 <<<"$status"; then
      echo "agent: account observation confirmed"
      demo_checkpoint_done "account_observed"
      return 0
    fi
    sleep 5
  done

  echo "agent did not observe account $account_id at $tx_hash#$output_index; log: $DEMO_AGENT_LOG_FILE" >&2
  tail -n 80 "$DEMO_AGENT_LOG_FILE" >&2 || true
  return 1
}

demo_account_status_json() {
  local state="$DEMO_RUN_DIR/preprod-green-order-e2e.json"
  local account_id tx_hash output_index
  account_id="$(jq -r '.pendingAccount.accountId // .account.accountId // empty' "$state")"
  tx_hash="$(jq -r '.pendingAccount.outputRef.txHash // .account.outputRef.txHash // empty' "$state")"
  output_index="$(jq -r '.pendingAccount.outputRef.outputIndex // .account.outputRef.outputIndex // empty' "$state")"
  [[ -n "$account_id" && -n "$tx_hash" && -n "$output_index" ]] || return 1
  curl -sS --get "${AGENT_URL:-http://127.0.0.1:9031}/accounts/status" \
    --data-urlencode "accountId=$account_id" \
    --data-urlencode "txHash=$tx_hash" \
    --data-urlencode "outputIndex=$output_index"
}

demo_account_needs_external_bind() {
  local status
  status="$(demo_account_status_json 2>/dev/null || true)"
  [[ -n "$status" ]] || return 0
  jq -e '.status.unbound == true and (.status.current != true) and (.status.predicted != true) and (.status.persisted != true)' >/dev/null 2>&1 <<<"$status"
}

demo_mark_post_bootstrap_log_position() {
  DEMO_POST_BOOTSTRAP_AGENT_LOG_LINE="$(demo_agent_log_line_count)"
  export DEMO_POST_BOOTSTRAP_AGENT_LOG_LINE
}

demo_wait_for_pool_ready() {
  local state="$DEMO_RUN_DIR/preprod-green-order-e2e.json"
  local tx_hash output_index pair_pattern
  tx_hash="$(jq -r '.pool.outputRef.txHash // empty' "$state")"
  output_index="$(jq -r '.pool.outputRef.outputIndex // empty' "$state")"
  pair_pattern="$(demo_pool_pair_log_pattern "$state")"
  if [[ -z "$tx_hash" || -z "$output_index" ]]; then
    echo "pool readiness requires pool output ref in $state" >&2
    return 1
  fi

  echo "agent: waiting for pool ref $tx_hash#$output_index to be live, indexed by agent, and node healthy"
  local deadline=$((SECONDS + ${DEMO_POOL_READY_TIMEOUT_SECONDS:-900}))
  while (( SECONDS < deadline )); do
    if ! demo_agent_process_is_running; then
      echo "agent exited while waiting for pool readiness; log: $DEMO_AGENT_LOG_FILE" >&2
      tail -n 80 "$DEMO_AGENT_LOG_FILE" >&2 || true
      return 1
    fi
    if demo_pool_ref_is_live "$tx_hash" "$output_index" &&
      demo_agent_node_is_healthy &&
      demo_agent_log_has_pool_indexed "$pair_pattern"; then
      echo "agent: pool readiness confirmed"
      demo_checkpoint_done "pool_ready"
      return 0
    fi
    sleep 5
  done

  echo "pool readiness timed out for $tx_hash#$output_index; log: $DEMO_AGENT_LOG_FILE" >&2
  tail -n 80 "$DEMO_AGENT_LOG_FILE" >&2 || true
  return 1
}

demo_pool_pair_log_pattern() {
  local state="$1"
  local asset_x asset_y policy name
  asset_x="$(jq -r '.pool.assetX // empty' "$state")"
  asset_y="$(jq -r '.pool.assetY // empty' "$state")"
  if [[ "$asset_x" == "00" && ${#asset_y} -gt 56 ]]; then
    policy="${asset_y:0:56}"
    name="${asset_y:56}"
    printf '(Native, %s.%s)' "$policy" "$name"
    return 0
  fi
  printf ''
}

demo_agent_log_has_pool_indexed() {
  local pair_pattern="$1"
  [[ -n "$pair_pattern" ]] || return 1
  [[ -f "$DEMO_AGENT_LOG_FILE" ]] || return 1
  grep -F "accepted liquidity-book event for pair $pair_pattern" "$DEMO_AGENT_LOG_FILE" >/dev/null 2>&1
}

demo_pool_ref_is_live() {
  local tx_hash="$1"
  local output_index="$2"
  local response
  response="$(
    curl -sS -X POST "$KOIOS_PREPROD_URL/utxo_info" \
      -H "content-type: application/json" \
      -d "{\"_utxo_refs\":[\"${tx_hash}#${output_index}\"]}" 2>/dev/null || true
  )"
  jq -e '.[0].is_spent != true' >/dev/null 2>&1 <<<"$response"
}

demo_agent_node_is_healthy() {
  demo_agent_process_is_running || return 1
  local health
  health="$(curl -sS "${AGENT_HEALTH_URL:-http://127.0.0.1:9024/health}" 2>/dev/null || true)"
  jq -e '.node.status.state == "Ok"' >/dev/null 2>&1 <<<"$health"
}

demo_agent_process_is_running() {
  local pid="${DEMO_AGENT_PID:-}"
  if [[ -z "$pid" && -f "${DEMO_RUN_DIR:-}/agent.pid" ]]; then
    pid="$(cat "$DEMO_RUN_DIR/agent.pid" 2>/dev/null || true)"
  fi
  [[ -n "$pid" ]] && ps -p "$pid" >/dev/null 2>&1
}

demo_agent_log_line_count() {
  if [[ -f "$DEMO_AGENT_LOG_FILE" ]]; then
    wc -l <"$DEMO_AGENT_LOG_FILE" | tr -d ' '
  else
    echo 0
  fi
}
