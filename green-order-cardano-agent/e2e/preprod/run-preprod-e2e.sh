#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

npm --prefix ../../../sdk/typescript run build

REPO_ROOT="$(cd ../../.. && pwd)"
START_GREEN_ORDER_AGENT="${START_GREEN_ORDER_AGENT:-1}"
STARTED_GREEN_ORDER_AGENT_PID=""
AGENT_RUN_CONFIG_PATH=""
AGENT_WRITABLE_BASE_CONFIG_PATH=""

find_free_loopback_port() {
  local port="$1"
  while lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1; do
    port=$((port + 1))
  done
  printf '%s\n' "$port"
}

derive_listen_addr_from_url() {
  local url="$1"
  printf '%s\n' "$url" | sed -E 's#^https?://([^/]+).*#\1#'
}

absolute_e2e_path() {
  local path="$1"
  if [[ "$path" == /* ]]; then
    printf '%s\n' "$path"
  else
    printf '%s\n' "$PWD/$path"
  fi
}

env_file_has_value() {
  local path="$1"
  local name="$2"
  [[ -f "$path" ]] && grep -Eq "^${name}=.+" "$path"
}

has_blockfrost_project_id() {
  if [[ -n "${BLOCKFROST_PROJECT_ID:-}" ]]; then
    return 0
  fi
  if env_file_has_value ".env" "BLOCKFROST_PROJECT_ID"; then
    return 0
  fi
  if env_file_has_value ".env.wallets" "BLOCKFROST_PROJECT_ID"; then
    return 0
  fi
  if [[ -n "${WALLET_ENV_PATH:-}" ]] && env_file_has_value "$WALLET_ENV_PATH" "BLOCKFROST_PROJECT_ID"; then
    return 0
  fi
  return 1
}

prepare_preprod_provider_env() {
  if has_blockfrost_project_id; then
    return 0
  fi

  if [[ -t 0 && -t 1 ]]; then
    local blockfrost_project_id
    printf "Enter Blockfrost preprod project id (press Enter to use Koios): "
    IFS= read -r blockfrost_project_id
    if [[ -n "$blockfrost_project_id" ]]; then
      export BLOCKFROST_PROJECT_ID="$blockfrost_project_id"
    else
      echo "BLOCKFROST_PROJECT_ID not set; using Koios provider"
    fi
  else
    echo "BLOCKFROST_PROJECT_ID not set and stdin is not interactive; using Koios provider"
  fi
}

has_preprod_wallet_env() {
  if [[ -n "${FUNDED_WALLET_SEED:-}" && -n "${BATCHER_ADDRESS:-}" ]]; then
    return 0
  fi
  if env_file_has_value ".env.wallets" "FUNDED_WALLET_SEED" && env_file_has_value ".env.wallets" "BATCHER_ADDRESS"; then
    return 0
  fi
  if env_file_has_value ".env" "FUNDED_WALLET_SEED" && env_file_has_value ".env" "BATCHER_ADDRESS"; then
    return 0
  fi
  if [[ -n "${WALLET_ENV_PATH:-}" ]] &&
    env_file_has_value "$WALLET_ENV_PATH" "FUNDED_WALLET_SEED" &&
    env_file_has_value "$WALLET_ENV_PATH" "BATCHER_ADDRESS"; then
    return 0
  fi
  return 1
}

prepare_preprod_wallet_env() {
  if has_preprod_wallet_env; then
    return 0
  fi

  echo "wallet env missing; generating local preprod wallet secrets"
  deno run --no-lock --allow-read --allow-write --allow-env 00-prepare-preprod-wallets.ts
  local batcher_address
  batcher_address="$(sed -n 's/^BATCHER_ADDRESS=//p' "${WALLET_ENV_PATH:-.env.wallets}" | head -n 1)"
  echo "Send at least ${E2E_BATCHER_REQUESTED_ADA:-400} tADA to:"
  echo "$batcher_address"
  deno run --no-lock --allow-net --allow-read --allow-env 00-wait-for-preprod-wallet-funding.ts
}

init_agent_endpoints() {
  if [[ -z "${AGENT_HEALTH_URL:-}" && -z "${AGENT_HEALTH_LISTEN_ADDR:-}" ]]; then
    local health_port
    health_port="$(find_free_loopback_port "${E2E_AGENT_HEALTH_PORT_START:-19024}")"
    AGENT_HEALTH_LISTEN_ADDR="127.0.0.1:$health_port"
    AGENT_HEALTH_URL="http://$AGENT_HEALTH_LISTEN_ADDR/health"
  elif [[ -z "${AGENT_HEALTH_LISTEN_ADDR:-}" ]]; then
    AGENT_HEALTH_LISTEN_ADDR="$(derive_listen_addr_from_url "$AGENT_HEALTH_URL")"
  elif [[ -z "${AGENT_HEALTH_URL:-}" ]]; then
    AGENT_HEALTH_URL="http://$AGENT_HEALTH_LISTEN_ADDR/health"
  fi

  if [[ -z "${AGENT_URL:-}" && -z "${AGENT_HTTP_LISTEN_ADDR:-}" ]]; then
    local http_port
    http_port="$(find_free_loopback_port "${E2E_AGENT_HTTP_PORT_START:-19031}")"
    while [[ "127.0.0.1:$http_port" == "$AGENT_HEALTH_LISTEN_ADDR" ]]; do
      http_port="$(find_free_loopback_port "$((http_port + 1))")"
    done
    AGENT_HTTP_LISTEN_ADDR="127.0.0.1:$http_port"
    AGENT_URL="http://$AGENT_HTTP_LISTEN_ADDR"
  elif [[ -z "${AGENT_HTTP_LISTEN_ADDR:-}" ]]; then
    AGENT_HTTP_LISTEN_ADDR="$(derive_listen_addr_from_url "$AGENT_URL")"
  elif [[ -z "${AGENT_URL:-}" ]]; then
    AGENT_URL="http://$AGENT_HTTP_LISTEN_ADDR"
  fi

  export AGENT_HEALTH_LISTEN_ADDR AGENT_HEALTH_URL AGENT_HTTP_LISTEN_ADDR AGENT_URL
}

health_endpoint_responds() {
  curl -sS "${AGENT_HEALTH_URL:-http://127.0.0.1:9024/health}" >/dev/null 2>&1
}

require_agent_endpoint_free() {
  if health_endpoint_responds; then
    echo "agent health endpoint is already in use before start: ${AGENT_HEALTH_URL}" >&2
    echo "Stop the existing agent process, or set START_GREEN_ORDER_AGENT=0 to use it." >&2
    return 1
  fi
}

wait_for_green_order_agent_health() {
  local log_path="${E2E_AGENT_LOG_PATH_ABS:-$(absolute_e2e_path "${E2E_AGENT_LOG_PATH:-.state/preprod-sdk-agent.log}")}"
  echo "agent: waiting for health endpoint"
  local deadline=$((SECONDS + ${E2E_AGENT_READY_TIMEOUT_SECONDS:-240}))
  while (( SECONDS < deadline )); do
    if [[ -n "$STARTED_GREEN_ORDER_AGENT_PID" ]] && ! kill -0 "$STARTED_GREEN_ORDER_AGENT_PID" >/dev/null 2>&1; then
      echo "agent exited before health endpoint responded" >&2
      tail -n 80 "$log_path" >&2 || true
      return 1
    fi
    if health_endpoint_responds; then
      echo "agent: health endpoint responded"
      return 0
    fi
    sleep 5
  done

  echo "agent did not become healthy; log: $log_path" >&2
  tail -n 80 "$log_path" >&2 || true
  return 1
}

stop_green_order_agent() {
  local pid="$STARTED_GREEN_ORDER_AGENT_PID"
  if [[ -z "$pid" ]]; then
    return 0
  fi
  if kill -0 "$pid" >/dev/null 2>&1; then
    echo "agent: stopping pid $pid"
    kill "$pid" >/dev/null 2>&1 || true
    local deadline=$((SECONDS + ${E2E_AGENT_STOP_TIMEOUT_SECONDS:-60}))
    while (( SECONDS < deadline )); do
      if ! kill -0 "$pid" >/dev/null 2>&1; then
        wait "$pid" 2>/dev/null || true
        STARTED_GREEN_ORDER_AGENT_PID=""
        return 0
      fi
      sleep 1
    done
    echo "agent: pid $pid did not stop after TERM; forcing shutdown"
    kill -KILL "$pid" >/dev/null 2>&1 || true
    wait "$pid" 2>/dev/null || true
  fi
  STARTED_GREEN_ORDER_AGENT_PID=""
}

prepare_green_order_agent_base_config() {
  if [[ "$START_GREEN_ORDER_AGENT" != "1" ]]; then
    return 0
  fi
  init_agent_endpoints
  mkdir -p .state
  if [[ -z "${AGENT_CONFIG_PATH:-}" ]]; then
    AGENT_WRITABLE_BASE_CONFIG_PATH="$(absolute_e2e_path "${E2E_AGENT_BASE_CONFIG_PATH:-.state/preprod-sdk-agent-base.json}")"
    cp "${BASE_AGENT_CONFIG_PATH:-../../resources/preprod.config.json}" "$AGENT_WRITABLE_BASE_CONFIG_PATH"
    export AGENT_CONFIG_PATH="$AGENT_WRITABLE_BASE_CONFIG_PATH"
  else
    AGENT_WRITABLE_BASE_CONFIG_PATH="$(absolute_e2e_path "$AGENT_CONFIG_PATH")"
    export AGENT_CONFIG_PATH="$AGENT_WRITABLE_BASE_CONFIG_PATH"
  fi
}

prepare_green_order_agent_config() {
  AGENT_RUN_CONFIG_PATH="$(absolute_e2e_path "${E2E_AGENT_CONFIG_PATH:-.state/preprod-sdk-agent.json}")"

  export BASE_AGENT_CONFIG_PATH="$AGENT_CONFIG_PATH"
  export PARTIAL_AGENT_DB_PATH="${PARTIAL_AGENT_DB_PATH:-$REPO_ROOT/green-order-cardano-agent/e2e/preprod/.state/preprod-sdk-agent-chain-sync}"
  export AGENT_CONFIG_PATH="$AGENT_RUN_CONFIG_PATH"

  deno run --no-lock --allow-net --allow-read --allow-write --allow-env \
    09-create-partial-agent-config.ts --out "$AGENT_RUN_CONFIG_PATH"
}

clean_forced_green_order_agent_state() {
  if [[ "${FORCE_E2E_STATE:-0}" != "1" ]]; then
    return 0
  fi

  local db_path
  db_path="$(deno eval --no-lock \
    'const config = JSON.parse(await Deno.readTextFile(Deno.args[0])); console.log(config.chainSync?.dbPath ?? "");' \
    "$AGENT_RUN_CONFIG_PATH")"
  if [[ -z "$db_path" || "$db_path" == "/" ]]; then
    echo "refusing to clean empty or root agent chain-sync db path: '$db_path'" >&2
    return 1
  fi

  local state_root
  state_root="$(absolute_e2e_path ".state")"
  if [[ "$db_path" != "$state_root"/* && "${ALLOW_FORCE_CLEAN_EXTERNAL_AGENT_DB:-0}" != "1" ]]; then
    echo "refusing to clean agent chain-sync db outside $state_root: $db_path" >&2
    echo "Set ALLOW_FORCE_CLEAN_EXTERNAL_AGENT_DB=1 only for an explicit disposable path." >&2
    return 1
  fi

  echo "agent: cleaning forced chain-sync state $db_path"
  rm -rf "$db_path" "$db_path.green-account-stores.json"
}

start_green_order_agent() {
  if [[ "$START_GREEN_ORDER_AGENT" != "1" ]]; then
    echo "agent: using externally managed endpoint ${AGENT_URL:-http://127.0.0.1:9031}"
    return 0
  fi

  init_agent_endpoints
  require_agent_endpoint_free
  prepare_green_order_agent_config
  clean_forced_green_order_agent_state

  echo "agent: building binary"
  (cd "$REPO_ROOT" && cargo build -q -p green-order-cardano-agent --bin green-order-cardano-agent)

  local log_path
  log_path="$(absolute_e2e_path "${E2E_AGENT_LOG_PATH:-.state/preprod-sdk-agent.log}")"
  E2E_AGENT_LOG_PATH_ABS="$log_path"
  export E2E_AGENT_LOG_PATH_ABS
  echo "agent: starting"
  (
    cd "$REPO_ROOT"
    exec ./target/debug/green-order-cardano-agent \
      --config-path "$AGENT_RUN_CONFIG_PATH" \
      --deployment-path green-order-cardano-agent/resources/preprod.deployment.json \
      --validation-rules-path green-order-cardano-agent/resources/validation-rules.json.template \
      --log4rs-path green-order-cardano-agent/resources/log4rs.local.yaml
  ) >"$log_path" 2>&1 &
  STARTED_GREEN_ORDER_AGENT_PID=$!
  export STARTED_GREEN_ORDER_AGENT_PID
  echo "$STARTED_GREEN_ORDER_AGENT_PID" >"$(absolute_e2e_path ".state/preprod-sdk-agent.pid")"
  trap stop_green_order_agent EXIT
  wait_for_green_order_agent_health
}

force_args=()
if [[ "${FORCE_E2E_STATE:-0}" == "1" ]]; then
  force_args=(--force)
  export FORCE_SEPARATOR_FUNDING=1
fi

prepare_green_order_agent_base_config
prepare_preprod_provider_env
prepare_preprod_wallet_env
deno run --no-lock --allow-net --allow-read --allow-write --allow-env 07-prepare-operator-funding.ts
start_green_order_agent

if [[ "${FORCE_E2E_STATE:-0}" == "1" ]]; then
  deno run --no-lock --allow-net --allow-read --allow-write --allow-env 02-create-entitlements.ts "${force_args[@]}"
  deno run --no-lock --allow-net --allow-read --allow-write --allow-env 03-create-aleph-account-and-bind.ts "${force_args[@]}"

  separator_log="$(mktemp)"
  max_pool_attempts="${FORCE_E2E_POOL_MAX_ATTEMPTS:-8}"
  for ((attempt = 1; attempt <= max_pool_attempts; attempt++)); do
    deno run --no-lock --allow-net --allow-read --allow-write --allow-env 01-create-royalty-v1-pool.ts "${force_args[@]}"
    if deno run --no-lock --allow-net --allow-read --allow-write --allow-env 08-create-separator-funding.ts >"$separator_log" 2>&1; then
      cat "$separator_log"
      rm -f "$separator_log"
      separator_log=""
      break
    fi

    cat "$separator_log"
    if ! grep -q "must sort before pool ref" "$separator_log"; then
      rm -f "$separator_log"
      separator_log=""
      exit 1
    fi
    if [[ "$attempt" == "$max_pool_attempts" ]]; then
      rm -f "$separator_log"
      separator_log=""
      echo "failed to create pool sorting after account in ${max_pool_attempts} attempts" >&2
      exit 1
    fi
    echo "pool tx hash sorted before account; retrying fresh pool (${attempt}/${max_pool_attempts})"
  done
  if [[ -n "${separator_log:-}" ]]; then
    rm -f "$separator_log"
  fi
else
  deno run --no-lock --allow-net --allow-read --allow-write --allow-env 01-create-royalty-v1-pool.ts
  deno run --no-lock --allow-net --allow-read --allow-write --allow-env 02-create-entitlements.ts
  deno run --no-lock --allow-net --allow-read --allow-write --allow-env 03-create-aleph-account-and-bind.ts
  deno run --no-lock --allow-net --allow-read --allow-write --allow-env 08-create-separator-funding.ts
fi

smoke_script="${E2E_SMOKE_SCRIPT:-04-submit-green-order-smoke.ts}"
deno run --no-lock --allow-net --allow-read --allow-write --allow-env --allow-run "${smoke_script}"

if [[ "${E2E_QUERY_SDK_AFTER_SMOKE:-1}" == "1" ]]; then
  sdk_query_script="${E2E_SDK_QUERY_SCRIPT:-11-query-agent-via-sdk.ts}"
  deno run --no-lock --allow-net --allow-read --allow-write --allow-env "${sdk_query_script}"
fi
