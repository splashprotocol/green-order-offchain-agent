#!/usr/bin/env bash

demo_bootstrap_env() {
  cd "$DEMO_E2E_DIR"
  export STATE_PATH="$DEMO_RUN_DIR/preprod-green-order-e2e.json"
  export FORCE_E2E_STATE=1
  export FORCE_SEPARATOR_FUNDING=1
  export WALLET_STATE_PATH="$DEMO_WALLET_STATE_FILE"
  export WALLET_ENV_PATH="$DEMO_WALLET_ENV_FILE"
  export CARDANO_NODE_SOCKET_PATH="${CARDANO_NODE_SOCKET_PATH:-${DEMO_NODE_SOCKET_PATH:-}}"
  export AGENT_CONFIG_PATH="$DEMO_BASE_AGENT_CONFIG_FILE"
}

demo_prepare_account_onchain() {
  if demo_checkpoint_is_done "account_created"; then
    echo "bootstrap: reusing existing account setup"
    return 0
  fi
  if jq -e '(.pendingAccount.outputRef.txHash and .pendingAccount.outputRef.outputIndex != null) or (.account.outputRef.txHash and .account.outputRef.outputIndex != null)' "$DEMO_RUN_DIR/preprod-green-order-e2e.json" >/dev/null 2>&1; then
    echo "bootstrap: reusing account already present in state"
    demo_checkpoint_done "account_created"
    return 0
  fi

  echo "bootstrap: creating account prerequisites using generated wallets"
  if [[ -f "$DEMO_BASE_AGENT_CONFIG_FILE" ]]; then
    local existing_socket
    existing_socket="$(jq -r '.node.path // empty' "$DEMO_BASE_AGENT_CONFIG_FILE" 2>/dev/null || true)"
    if [[ -n "$existing_socket" && "$existing_socket" != "$CARDANO_NODE_SOCKET_PATH" ]]; then
      echo "bootstrap: refreshing run-local agent config for current node socket"
      cp "$REPO_ROOT/green-order-cardano-agent/resources/preprod.config.json" "$DEMO_BASE_AGENT_CONFIG_FILE"
    fi
  else
    cp "$REPO_ROOT/green-order-cardano-agent/resources/preprod.config.json" "$DEMO_BASE_AGENT_CONFIG_FILE"
  fi
  (
    demo_bootstrap_env
    deno run --no-lock --allow-net --allow-read --allow-write --allow-env 07-prepare-operator-funding.ts
    deno run --no-lock --allow-net --allow-read --allow-write --allow-env 02-create-entitlements.ts --force
    deno run --no-lock --allow-net --allow-read --allow-write --allow-env 03-create-aleph-account-and-bind.ts --create-only
  ) 2>&1 | tee "$DEMO_LOG_DIR/bootstrap.log"

  demo_checkpoint_done "account_created"
}

demo_bind_account_onchain() {
  local binding_ref
  binding_ref="$(demo_current_account_binding_ref)"

  echo "bootstrap: binding account to agent"
  (
    demo_bootstrap_env
    deno run --no-lock --allow-net --allow-read --allow-write --allow-env 03-create-aleph-account-and-bind.ts --bind-only
  ) 2>&1 | tee -a "$DEMO_LOG_DIR/bootstrap.log"

  demo_checkpoint_done "account_bound"
  demo_checkpoint_set_value "account_bound_ref" "$binding_ref"
}

demo_current_account_binding_ref() {
  local state="$DEMO_RUN_DIR/preprod-green-order-e2e.json"
  [[ -f "$state" ]] || return 0
  local tx_hash output_index
  tx_hash="$(jq -r '.pendingAccount.outputRef.txHash // .account.outputRef.txHash // empty' "$state")"
  output_index="$(jq -r '.pendingAccount.outputRef.outputIndex // .account.outputRef.outputIndex // empty' "$state")"
  [[ -n "$tx_hash" && -n "$output_index" ]] || return 0
  printf '%s#%s\n' "$tx_hash" "$output_index"
}

demo_prepare_pool_onchain() {
  if demo_checkpoint_is_done "bootstrap"; then
    echo "bootstrap: reusing existing pool setup"
    return 0
  fi

  echo "bootstrap: creating pool and separator funding"
  (
    demo_bootstrap_env
    account_tx_hash="$(jq -r '.account.outputRef.txHash // empty' "$STATE_PATH")"
    if [[ -z "$account_tx_hash" ]]; then
      echo "pool setup requires bound account output ref in $STATE_PATH" >&2
      exit 1
    fi
    export POOL_MIN_TX_HASH="$account_tx_hash"
    separator_log="$DEMO_LOG_DIR/separator-funding.log"
    max_pool_attempts="${FORCE_E2E_POOL_MAX_ATTEMPTS:-8}"
    for ((attempt = 1; attempt <= max_pool_attempts; attempt++)); do
      if ! jq -e '.pool.outputRef.txHash and .pool.outputRef.outputIndex != null' "$STATE_PATH" >/dev/null 2>&1; then
        deno run --no-lock --allow-net --allow-read --allow-write --allow-env 01-create-royalty-v1-pool.ts --force
      else
        echo "pool output already present in state; reusing it"
      fi
      if deno run --no-lock --allow-net --allow-read --allow-write --allow-env 08-create-separator-funding.ts >"$separator_log" 2>&1; then
        cat "$separator_log"
        break
      fi

      cat "$separator_log"
      if ! grep -q "must sort before pool ref" "$separator_log"; then
        exit 1
      fi
      if [[ "$attempt" == "$max_pool_attempts" ]]; then
        echo "failed to create pool sorting after account in ${max_pool_attempts} attempts" >&2
        exit 1
      fi
      echo "pool tx hash sorted before account; retrying fresh pool (${attempt}/${max_pool_attempts})"
      deno run --no-lock --allow-net --allow-read --allow-write --allow-env 01-create-royalty-v1-pool.ts --force
    done
  ) 2>&1 | tee "$DEMO_LOG_DIR/bootstrap.log"

  demo_checkpoint_done "bootstrap"
}
