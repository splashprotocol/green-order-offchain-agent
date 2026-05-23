#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

REPO_ROOT="$repo_root"
source "$repo_root/demo/lib/run.sh"
source "$repo_root/demo/lib/report.sh"
source "$repo_root/demo/lib/verify.sh"

DEMO_STATE_ROOT="$tmp_dir/runs"
demo_parse_args --fresh
demo_init_run

cat >"$DEMO_STATE_FILE" <<JSON
{
  "runId": "$DEMO_RUN_ID",
  "network": "preprod",
  "wallets": {
    "fundingAddress": "addr_test1demo",
    "batcherAddress": "addr_test1batcher",
    "deploymentAddress": "addr_test1deployment"
  },
  "transactions": {
    "partialFillExecutionTx": "03a42197c2d628d981af30de948717d861d413dab5dfb80a2ba49388f3550f63"
  },
  "partial": {
    "intentDigest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "residualLeaving": "6787028",
    "residualExpectedOutput": "5090271"
  },
  "verification": {
    "partialFill": "PASS",
    "continuation": "PASS"
  }
}
JSON

demo_generate_report "REPORT ONLY"
DEMO_SKIP_REPORT_VALIDATION=1 demo_validate_report_only_inputs

[[ -f "$DEMO_REPORT_FILE" ]]
grep -q "REPORT ONLY" "$DEMO_REPORT_FILE"
grep -q "$DEMO_RUN_ID" "$DEMO_REPORT_FILE"
grep -q "03a42197c2d628d981af30de948717d861d413dab5dfb80a2ba49388f3550f63" "$DEMO_REPORT_FILE"
