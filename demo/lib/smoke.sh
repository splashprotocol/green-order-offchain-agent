#!/usr/bin/env bash

demo_run_smoke_phases() {
  if [[ "${DEMO_PARTIAL_ONLY:-0}" != "1" ]]; then
    demo_run_full_fill_smoke
  fi
  demo_run_partial_fill_smoke
}

demo_run_full_fill_smoke() {
  if demo_checkpoint_is_done "full_fill"; then
    echo "smoke: reusing full-fill result"
    return 0
  fi
  echo "smoke: running full-fill intent"
  (
    cd "$DEMO_E2E_DIR"
    export STATE_PATH="$DEMO_RUN_DIR/preprod-green-order-e2e.json"
    export AGENT_CONFIG_PATH="$DEMO_AGENT_CONFIG_FILE"
    deno run --no-lock --allow-net --allow-read --allow-write --allow-env --allow-run \
      04-submit-green-order-smoke.ts
  ) 2>&1 | tee "$DEMO_LOG_DIR/full-fill.log"
  demo_checkpoint_done "full_fill"
}

demo_run_partial_fill_smoke() {
  if demo_checkpoint_is_done "partial_fill"; then
    echo "smoke: reusing partial-fill result"
    return 0
  fi
  echo "smoke: running partial-fill intent"
  (
    cd "$DEMO_E2E_DIR"
    export STATE_PATH="$DEMO_RUN_DIR/preprod-green-order-e2e.json"
    export AGENT_CONFIG_PATH="$DEMO_AGENT_CONFIG_FILE"
    export PARTIAL_E2E=1
    export PARTIAL_PREFLIGHT_BIN="$REPO_ROOT/target/debug/partial_preflight"
    cargo build -q -p green-order-cardano-agent --bin partial_preflight
    deno run --no-lock --allow-net --allow-read --allow-write --allow-env --allow-run \
      10-submit-green-order-partial-smoke.ts
  ) 2>&1 | tee "$DEMO_LOG_DIR/partial-fill.log"
  demo_checkpoint_done "partial_fill"
}

demo_print_tx_summary() {
  local state="$DEMO_RUN_DIR/preprod-green-order-e2e.json"
  if [[ -f "$state" ]]; then
    echo "Full-fill execution tx: $(jq -r '.smoke.executionTxHash // "not-run"' "$state")"
    echo "Partial-fill execution tx: $(jq -r '.partialSmoke.executionTxHash // "not-run"' "$state")"
    echo "Residual received amount: $(jq -r '.partialSmoke.receivedAmount // "unknown"' "$state")"
  fi
}
