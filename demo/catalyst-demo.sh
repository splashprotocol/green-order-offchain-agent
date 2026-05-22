#!/usr/bin/env bash
set -euo pipefail

DEMO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$DEMO_ROOT/.." && pwd)"

source "$DEMO_ROOT/lib/run.sh"
source "$DEMO_ROOT/lib/env.sh"
source "$DEMO_ROOT/lib/wallets.sh"
source "$DEMO_ROOT/lib/funding.sh"
source "$DEMO_ROOT/lib/bootstrap.sh"
source "$DEMO_ROOT/lib/agent.sh"
source "$DEMO_ROOT/lib/smoke.sh"
source "$DEMO_ROOT/lib/verify.sh"
source "$DEMO_ROOT/lib/report.sh"

demo_main() {
  demo_parse_args "$@"
  if [[ "${DEMO_HELP:-0}" == "1" ]]; then
    demo_print_help
    return 0
  fi

  demo_load_environment
  demo_init_run

  if [[ "${DEMO_REPORT_ONLY:-0}" == "1" ]]; then
    demo_validate_report_only_inputs
    demo_generate_report "REPORT ONLY"
    echo "REPORT ONLY: regenerated $DEMO_REPORT_FILE"
    return 0
  fi

  trap 'demo_stop_agent || true' EXIT

  demo_check_required_tools
  demo_refuse_unsafe_network
  demo_prepare_wallets
  demo_wait_for_funding
  demo_prepare_funding_boxes
  demo_bootstrap_onchain
  demo_start_agent
  demo_wait_for_indexer_ready
  demo_run_smoke_phases
  demo_verify_execution_results
  demo_generate_report "PASS"

  echo
  echo "Catalyst Demo Summary"
  echo "Run: $DEMO_RUN_ID"
  echo "Network: preprod"
  demo_print_tx_summary
  echo "Report: $DEMO_REPORT_FILE"
  echo "Result: PASS"
}

demo_print_help() {
  cat <<'EOF'
Catalyst Judge Demo

Usage:
  ./demo/catalyst-demo.sh [--fresh] [--partial-only]
  ./demo/catalyst-demo.sh --run-id <id>
  ./demo/catalyst-demo.sh --report-only --run-id <id>

What it does:
  - creates a fresh per-run wallet set by default
  - prints one funding address and waits for tADA
  - splits funding into demo setup boxes
  - creates pool/account/bindings
  - starts the preprod agent with a generated config
  - runs full-fill and partial-fill intent smoke tests
  - verifies residual continuation after partial fill
  - writes a judge/auditor report

Flags:
  --fresh          Create a new run with new wallets and state.
  --partial-only   Skip full-fill smoke and run partial-fill proof only.
  --report-only    Read-only report regeneration for an existing --run-id.
  --run-id <id>    Resume or inspect an existing run directory.
  --help           Show this help.

Generated files live under:
  demo/.state/runs/<run-id>/

The only manual action expected from a judge is sending preprod tADA to the
funding address printed by the script.
EOF
}

demo_main "$@"
