#!/usr/bin/env bash

demo_generate_report() {
  local result="$1"
  mkdir -p "$(dirname "$DEMO_REPORT_FILE")"
  local commit
  commit="$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)"
  local e2e_state="$DEMO_RUN_DIR/preprod-green-order-e2e.json"

  {
    echo "# Catalyst Demo Report"
    echo
    echo "- Result: $result"
    echo "- Run ID: $DEMO_RUN_ID"
    echo "- Commit: $commit"
    echo "- Network: preprod"
    echo "- Generated at: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "- State file: $DEMO_STATE_FILE"
    echo "- Agent config: ${DEMO_AGENT_CONFIG_FILE:-n/a}"
    echo "- Agent log: ${DEMO_AGENT_LOG_FILE:-n/a}"
    echo
    echo "## Wallets"
    if [[ -f "${DEMO_WALLET_ENV_FILE:-}" ]]; then
      echo "- Funding address: $(grep '^BATCHER_ADDRESS=' "$DEMO_WALLET_ENV_FILE" | cut -d= -f2-)"
      echo "- Deployment address: $(grep '^DEPLOYMENT_ADDRESS=' "$DEMO_WALLET_ENV_FILE" | cut -d= -f2-)"
    else
      jq -r '.wallets // {} | to_entries[]? | "- \(.key): \(.value)"' "$DEMO_STATE_FILE" 2>/dev/null || true
    fi
    echo
    echo "## Transactions"
    if [[ -f "$e2e_state" ]]; then
      echo "- Full-fill execution tx: $(jq -r '.smoke.executionTxHash // "not-run"' "$e2e_state")"
      echo "- Partial-fill execution tx: $(jq -r '.partialSmoke.executionTxHash // "not-run"' "$e2e_state")"
      echo "- Partial intent digest: $(jq -r '.partialSmoke.submittedIntentDigest // "unknown"' "$e2e_state")"
      echo "- Partial received amount: $(jq -r '.partialSmoke.receivedAmount // "unknown"' "$e2e_state")"
      echo "- Store path: $(jq -r '.partialSmoke.storePath // "unknown"' "$e2e_state")"
    else
      jq -r '.transactions // {} | to_entries[]? | "- \(.key): \(.value)"' "$DEMO_STATE_FILE" 2>/dev/null || true
    fi
    echo
    echo "## Verification"
    if [[ -f "$DEMO_CHECKPOINTS_FILE" ]]; then
      jq -r 'to_entries[] | "- \(.key): \(.value.status)"' "$DEMO_CHECKPOINTS_FILE"
    else
      jq -r '.verification // {} | to_entries[]? | "- \(.key): \(.value)"' "$DEMO_STATE_FILE" 2>/dev/null || true
    fi
    echo
    echo "## Failure Log Excerpt"
    if [[ -f "${DEMO_AGENT_LOG_FILE:-}" ]]; then
      tail -n 80 "$DEMO_AGENT_LOG_FILE"
    else
      echo "No agent log captured for this mode."
    fi
  } >"$DEMO_REPORT_FILE"
}
