#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

help_output="$(bash "$repo_root/demo/catalyst-demo.sh" --help)"
[[ "$help_output" == *"Catalyst Judge Demo"* ]]
[[ "$help_output" == *"--fresh"* ]]
[[ "$help_output" == *"--partial-only"* ]]
[[ "$help_output" == *"--report-only"* ]]
[[ "$help_output" == *"--run-id"* ]]

source "$repo_root/demo/lib/run.sh"

DEMO_STATE_ROOT="$tmp_dir/runs"
demo_parse_args --fresh
demo_init_run

[[ -n "${DEMO_RUN_ID:-}" ]]
[[ -d "$DEMO_RUN_DIR/wallets" ]]
[[ -d "$DEMO_RUN_DIR/config" ]]
[[ -d "$DEMO_RUN_DIR/logs" ]]
[[ -f "$DEMO_STATE_FILE" ]]
[[ -f "$DEMO_CHECKPOINTS_FILE" ]]
[[ "$DEMO_MODE" == "fresh" ]]

touch "$DEMO_RUN_DIR/wallets/existing.key"
if DEMO_STATE_ROOT="$tmp_dir/runs" demo_parse_args --fresh --run-id "$DEMO_RUN_ID" 2>/dev/null; then
  echo "expected --fresh --run-id to be rejected" >&2
  exit 1
fi

demo_parse_args --run-id "$DEMO_RUN_ID"
demo_init_run
[[ "$DEMO_MODE" == "reuse" ]]
[[ -f "$DEMO_RUN_DIR/wallets/existing.key" ]]
