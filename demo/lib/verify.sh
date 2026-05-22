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

  if grep -E 'panic|Unsupported|invalid proof|RootMismatch|task abort' "$DEMO_AGENT_LOG_FILE" >/dev/null 2>&1; then
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
}
