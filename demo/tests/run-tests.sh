#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."

bash demo/tests/test_cli_and_state.sh
bash demo/tests/test_wallet_generation.sh
bash demo/tests/test_report_only.sh
