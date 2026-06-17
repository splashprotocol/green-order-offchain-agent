# Testing And Verification

Repository: https://github.com/splashprotocol/green-order-offchain-agent

This document gives auditors the local and preprod verification paths for the milestone 5 close-out package.

## Local Checks

Run from the repository root:

```sh
cargo test -q -p green-order-cardano-agent account_index
cargo test -q -p green-order-cardano-agent account_store
cargo test -q -p green-order-cardano-agent http_intent_source
npm --prefix sdk/typescript test
bash demo/tests/run-tests.sh
```

These checks cover Rust account indexing, account-store planning, HTTP intent handling, SDK request/signature helpers, and the local demo wrapper behavior.

## Preprod Script Checks

Run Deno checks/tests from the preprod script directory:

```sh
cd green-order-cardano-agent/e2e/preprod
deno test --allow-read --allow-write --allow-env tests
deno check 00-prepare-preprod-wallets.ts 07-prepare-operator-funding.ts 11-query-agent-via-sdk.ts 12-query-agent-with-bad-hmac.ts src/config.ts
```

These checks cover wallet generation, config/state helpers, partial-agent config, partial preflight, SDK-agent wiring, and bad-HMAC behavior.

## Full Live Preprod Demo

The full live run requires:

- Cardano preprod node socket.
- Optional but recommended preprod Blockfrost key.
- Enough preprod tADA for generated wallets.
- Rust, Cargo, Deno, Node/npm, `jq`, and `curl`.

Run:

```sh
./demo/catalyst-demo.sh
```

The script prints a generated funding address. Send the requested preprod tADA to that address, then let the script continue. It creates the required setup, starts the agent, runs full-fill smoke tests and partial-fill smoke tests under the generated partial-enabled config, verifies the outputs, and writes `demo-report.md` under the run directory.

For report-only regeneration:

```sh
./demo/catalyst-demo.sh --report-only --run-id <run-id>
```

## Expected Result

The local checks should exit with status `0`. A successful live demo ends with a `PASS` result in the generated report and includes public preprod transaction hashes for the account, pool, full-fill, and partial-fill demo steps under the generated partial-enabled config.

Generated wallets, seeds, HMAC secrets, node sockets, and run state stay local under ignored state directories and must not be committed.
