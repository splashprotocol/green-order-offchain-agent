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
source "$repo_root/demo/lib/env.sh"
source "$repo_root/demo/lib/agent.sh"

CARDANO_NETWORK=preprod demo_refuse_unsafe_network
if CARDANO_NETWORK=mainnet demo_refuse_unsafe_network 2>/dev/null; then
  echo "expected mainnet to be rejected" >&2
  exit 1
fi
if CARDANO_NETWORK=preview demo_refuse_unsafe_network 2>/dev/null; then
  echo "expected preview to be rejected" >&2
  exit 1
fi

unset DEMO_NODE_SOCKET_PATH CARDANO_NODE_SOCKET_PATH NODE_SOCKET_PATH
if demo_require_node_socket_path </dev/null 2>"$tmp_dir/missing-socket.err"; then
  echo "expected missing node socket path to be rejected" >&2
  exit 1
fi
grep -q "Set CARDANO_NODE_SOCKET_PATH" "$tmp_dir/missing-socket.err"

DEMO_NODE_SOCKET_PATH=relative/node.socket
if demo_require_node_socket_path 2>"$tmp_dir/relative-socket.err"; then
  echo "expected relative node socket path to be rejected" >&2
  exit 1
fi
grep -q "must be absolute" "$tmp_dir/relative-socket.err"
unset DEMO_NODE_SOCKET_PATH

DEMO_STATE_ROOT="$tmp_dir/runs"
demo_parse_args --fresh
demo_init_run

[[ -n "${DEMO_RUN_ID:-}" ]]
[[ -d "$DEMO_RUN_DIR/wallets" ]]
[[ -d "$DEMO_RUN_DIR/config" ]]
[[ -d "$DEMO_RUN_DIR/logs" ]]
[[ "$DEMO_BASE_AGENT_CONFIG_FILE" == "$DEMO_RUN_DIR/config/preprod-base-agent.json" ]]
[[ "$DEMO_AGENT_CONFIG_FILE" == "$DEMO_RUN_DIR/config/preprod-partial-agent.json" ]]
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

export DEMO_CHAIN_SYNC_LOOKBACK_SECONDS=777
rm -rf "$DEMO_RUN_DIR/agent-chain-sync" "$DEMO_RUN_DIR/agent-chain-sync.green-account-stores.json"
mkdir -p "$DEMO_RUN_DIR/agent-chain-sync"
touch "$DEMO_RUN_DIR/agent-chain-sync.green-account-stores.json"
lookback="$(demo_partial_chain_sync_lookback_seconds)"
[[ "$lookback" == "777" ]]
[[ ! -e "$DEMO_RUN_DIR/agent-chain-sync" ]]
[[ ! -e "$DEMO_RUN_DIR/agent-chain-sync.green-account-stores.json" ]]
