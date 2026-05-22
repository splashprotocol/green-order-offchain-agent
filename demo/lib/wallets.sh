#!/usr/bin/env bash

demo_prepare_wallets() {
  if demo_checkpoint_is_done "wallets"; then
    echo "wallets: reusing $DEMO_WALLET_STATE_FILE"
    return 0
  fi

  echo "wallets: generating fresh per-run preprod wallets"
  (
    cd "$DEMO_E2E_DIR"
    WALLET_STATE_PATH="$DEMO_WALLET_STATE_FILE" \
    WALLET_ENV_PATH="$DEMO_WALLET_ENV_FILE" \
    FORCE_NEW_WALLETS=1 \
      deno run --no-lock --allow-read --allow-write --allow-env 00-prepare-preprod-wallets.ts
  ) | tee "$DEMO_LOG_DIR/wallets.log"

  DEMO_FUNDING_ADDRESS="$(grep '^BATCHER_ADDRESS=' "$DEMO_WALLET_ENV_FILE" | cut -d= -f2-)"
  if [[ -z "$DEMO_FUNDING_ADDRESS" ]]; then
    echo "failed to read generated funding address from $DEMO_WALLET_ENV_FILE" >&2
    return 1
  fi

  demo_checkpoint_done "wallets"
  echo "Funding address: $DEMO_FUNDING_ADDRESS"
}

demo_wait_for_funding() {
  local min_ada="${DEMO_MIN_FUNDING_ADA:-120}"
  local address="${DEMO_FUNDING_ADDRESS:-}"
  if [[ -z "$address" && -f "$DEMO_WALLET_ENV_FILE" ]]; then
    address="$(grep '^BATCHER_ADDRESS=' "$DEMO_WALLET_ENV_FILE" | cut -d= -f2-)"
  fi
  if [[ -z "$address" ]]; then
    echo "funding address is unknown; wallet generation did not complete" >&2
    return 1
  fi

  echo
  echo "Send at least ${min_ada} tADA to:"
  echo "$address"
  echo

  if [[ "${DEMO_SKIP_FUNDING_WAIT:-0}" == "1" ]]; then
    echo "funding: skipped by DEMO_SKIP_FUNDING_WAIT=1"
    return 0
  fi

  local timeout="${DEMO_FUNDING_TIMEOUT_SECONDS:-1800}"
  local deadline=$((SECONDS + timeout))
  while (( SECONDS < deadline )); do
    local lovelace
    lovelace="$(demo_address_lovelace "$address" || true)"
    if [[ "$lovelace" =~ ^[0-9]+$ ]] && (( lovelace >= min_ada * 1000000 )); then
      echo "funding: detected $((lovelace / 1000000)) tADA"
      demo_checkpoint_done "funded"
      return 0
    fi
    echo "funding: waiting for tADA at $address"
    sleep "${DEMO_FUNDING_POLL_SECONDS:-20}"
  done

  echo "funding timeout; rerun with --run-id $DEMO_RUN_ID after sending tADA" >&2
  return 1
}

demo_address_lovelace() {
  local address="$1"
  curl -sS -X POST "$KOIOS_PREPROD_URL/address_info" \
    -H "content-type: application/json" \
    -d "{\"_addresses\":[\"$address\"]}" |
    jq -r '.[0].balance // "0"'
}
