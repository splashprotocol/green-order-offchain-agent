#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

wallet_state="$tmp_dir/wallets/preprod-wallets.json"
wallet_env="$tmp_dir/wallets/.env.wallets"

(
  cd "$repo_root/green-order-cardano-agent/e2e/preprod"
  WALLET_STATE_PATH="$wallet_state" \
  WALLET_ENV_PATH="$wallet_env" \
  FORCE_NEW_WALLETS=1 \
    deno run --no-lock --allow-read --allow-write --allow-env 00-prepare-preprod-wallets.ts
)

[[ -f "$wallet_state" ]]
[[ -f "$wallet_env" ]]
grep -q "FUNDED_WALLET_SEED=" "$wallet_env"
grep -q "DEPLOYMENT_WALLET_SEED=" "$wallet_env"

first_address="$(grep '^BATCHER_ADDRESS=' "$wallet_env" | cut -d= -f2-)"

(
  cd "$repo_root/green-order-cardano-agent/e2e/preprod"
  WALLET_STATE_PATH="$wallet_state" \
  WALLET_ENV_PATH="$wallet_env" \
    deno run --no-lock --allow-read --allow-write --allow-env 00-prepare-preprod-wallets.ts
)

second_address="$(grep '^BATCHER_ADDRESS=' "$wallet_env" | cut -d= -f2-)"
[[ "$first_address" == "$second_address" ]]
