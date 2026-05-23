#!/usr/bin/env bash

demo_verify_execution_results() {
  echo "verify: checking smoke outputs and agent logs"
  local state="$DEMO_RUN_DIR/preprod-green-order-e2e.json"
  [[ -f "$state" ]] || {
    echo "missing E2E state file: $state" >&2
    return 1
  }
  if [[ "${DEMO_PARTIAL_ONLY:-0}" != "1" ]]; then
    jq -e '.smoke.executionTxHash | strings | length == 64' "$state" >/dev/null
  fi
  jq -e '.partialSmoke.executionTxHash | strings | length == 64' "$state" >/dev/null
  jq -e '.partialSmoke.submittedIntentDigest | strings | length == 64' "$state" >/dev/null
  jq -e '.partialSmoke.storePath | strings | length > 0' "$state" >/dev/null

  if [[ "${DEMO_AGENT_STARTED_THIS_ATTEMPT:-0}" == "1" ]] &&
    grep -E 'panic|Unsupported|invalid proof|RootMismatch|task abort' "$DEMO_AGENT_LOG_FILE" >/dev/null 2>&1; then
    echo "agent log contains unexpected crash pattern; log: $DEMO_AGENT_LOG_FILE" >&2
    grep -En 'panic|Unsupported|invalid proof|RootMismatch|task abort' "$DEMO_AGENT_LOG_FILE" >&2 || true
    return 1
  fi

  demo_checkpoint_done "verified"
}

demo_validate_report_only_inputs() {
  [[ -f "$DEMO_STATE_FILE" ]] || {
    echo "missing report-only state file: $DEMO_STATE_FILE" >&2
    return 1
  }
  if [[ "${DEMO_SKIP_REPORT_VALIDATION:-0}" == "1" ]]; then
    return 0
  fi

  local e2e_state="$DEMO_RUN_DIR/preprod-green-order-e2e.json"
  local txs=()
  if [[ -f "$e2e_state" ]]; then
    local full partial
    full="$(jq -r '.smoke.executionTxHash // empty' "$e2e_state")"
    partial="$(jq -r '.partialSmoke.executionTxHash // empty' "$e2e_state")"
    [[ -n "$full" ]] && txs+=("$full")
    [[ -n "$partial" ]] && txs+=("$partial")
  else
    while IFS= read -r tx; do
      [[ -n "$tx" ]] && txs+=("$tx")
    done < <(jq -r '.transactions // {} | to_entries[].value | select(type == "string" and test("^[0-9a-fA-F]{64}$"))' "$DEMO_STATE_FILE")
  fi

  if [[ "${#txs[@]}" -eq 0 ]]; then
    echo "report-only validation found no tx hashes to verify" >&2
    return 1
  fi

  local tx
  for tx in "${txs[@]}"; do
    demo_validate_tx_confirmed "$tx"
  done
}

demo_validate_tx_confirmed() {
  local tx="$1"
  local response
  response="$(
    curl -sS -X POST "$KOIOS_PREPROD_URL/tx_info" \
      -H "content-type: application/json" \
      -d "{\"_tx_hashes\":[\"$tx\"]}"
  )"
  if ! jq -e '.[0].tx_hash | strings | length == 64' >/dev/null <<<"$response"; then
    echo "report-only validation could not confirm tx $tx" >&2
    return 1
  fi
}
