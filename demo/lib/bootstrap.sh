#!/usr/bin/env bash

demo_bootstrap_onchain() {
  if demo_checkpoint_is_done "bootstrap"; then
    echo "bootstrap: reusing existing on-chain setup"
    return 0
  fi

  echo "bootstrap: creating pool/account/bindings using generated wallets"
  (
    cd "$DEMO_E2E_DIR"
    export STATE_PATH="$DEMO_RUN_DIR/preprod-green-order-e2e.json"
    export FORCE_E2E_STATE=1
    export FORCE_SEPARATOR_FUNDING=1
    export WALLET_STATE_PATH="$DEMO_WALLET_STATE_FILE"
    export WALLET_ENV_PATH="$DEMO_WALLET_ENV_FILE"
    deno run --no-lock --allow-net --allow-read --allow-write --allow-env 07-prepare-operator-funding.ts
    deno run --no-lock --allow-net --allow-read --allow-write --allow-env 02-create-entitlements.ts --force
    deno run --no-lock --allow-net --allow-read --allow-write --allow-env 03-create-aleph-account-and-bind.ts --force
    deno run --no-lock --allow-net --allow-read --allow-write --allow-env 01-create-royalty-v1-pool.ts --force
    deno run --no-lock --allow-net --allow-read --allow-write --allow-env 08-create-separator-funding.ts
  ) 2>&1 | tee "$DEMO_LOG_DIR/bootstrap.log"

  demo_checkpoint_done "bootstrap"
}
