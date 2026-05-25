# Catalyst Demo Script Reference

This file is the operational script reference for reviewers and auditors. It
explains what each script does, how to run the demo, what files are produced,
and what evidence to inspect after a run.

The intended reviewer flow is:

```bash
./demo/catalyst-demo.sh
```

The script creates new wallets for every fresh run. The reviewer only needs to
send preprod tADA to the funding address printed by the script.

## Prerequisites

Install and expose these tools on `PATH`:

- `bash`
- `cargo`
- `deno`
- `jq`
- `curl`

The full demo also requires a running Cardano preprod node with node-to-client
socket access. Provide the socket path before running, or enter it when the
script prompts:

```bash
export CARDANO_NODE_SOCKET_PATH="/absolute/path/to/node.socket"
```

The path must be absolute and must point to an existing Unix socket. The script
uses this socket for chain-sync, mempool monitoring, and transaction
submission.

The demo is preprod-only. The wrapper refuses `CARDANO_NETWORK=mainnet` or
`NETWORK=mainnet`.

By default Koios preprod is used:

```bash
https://preprod.koios.rest/api/v1
```

Override only when needed:

```bash
export KOIOS_PREPROD_URL="https://preprod.koios.rest/api/v1"
```

## Main Commands

### Fresh Full Demo

```bash
./demo/catalyst-demo.sh
```

What happens:

1. A new run id is created.
2. The preprod node socket path is read from `CARDANO_NODE_SOCKET_PATH` or
   requested interactively.
3. New per-run wallets are generated.
4. The script prints one funding address.
5. The reviewer sends preprod tADA to that address.
6. Funding boxes are prepared from that tADA.
7. The Aleph account, entitlement, separator funding, and pool are created.
8. The agent starts with a generated partial-fill-enabled config.
9. A full-fill green-order smoke intent is submitted and verified.
10. A partial-fill green-order smoke intent is submitted and verified.
11. A report is written under the run directory.

Expected final output includes:

```text
Result: PASS
Full-fill execution tx: <64 hex chars>
Partial-fill execution tx: <64 hex chars>
Report: demo/.state/runs/<run-id>/demo-report.md
```

### Resume an Interrupted Run

```bash
./demo/catalyst-demo.sh --run-id <run-id>
```

Use this if the terminal was interrupted, a network request failed, or a
checkpoint was not written after an on-chain transaction landed.

Resume behavior:

- completed checkpointed phases are reused;
- existing generated wallets are reused;
- existing full-fill and partial-fill txs are reused when already recorded;
- if `.partialSmoke.executionTxHash` exists, the wrapper records the missing
  `partial_fill` checkpoint and proceeds to final verification/reporting;
- no new wallets are created for the same `--run-id`.

### Partial-Only Demo

```bash
./demo/catalyst-demo.sh --partial-only
```

This skips the full-fill smoke phase and runs the partial-fill proof path only.
It is useful when testing partial-fill behavior in isolation.

### Report-Only Audit Mode

```bash
./demo/catalyst-demo.sh --report-only --run-id <run-id>
```

This mode does not start the agent and does not submit transactions. It
validates recorded transaction hashes against Koios and regenerates:

```text
demo/.state/runs/<run-id>/demo-report.md
```

Use report-only mode when an auditor wants to inspect a completed run without
spending tADA or mutating chain state.

### Help

```bash
./demo/catalyst-demo.sh --help
```

Prints supported flags and a short flow description.

## Generated Run Directory

Every normal run writes to:

```text
demo/.state/runs/<run-id>/
```

Important files:

| Path | Purpose |
| --- | --- |
| `wallets/preprod-wallets.json` | Generated wallet metadata for this run. |
| `wallets/.env.wallets` | Generated env file with wallet seeds and addresses. |
| `config/preprod-base-agent.json` | Run-local base agent config updated with generated operator and provided node socket. |
| `config/preprod-partial-agent.json` | Agent config generated for this run. |
| `preprod-green-order-e2e.json` | Main E2E state used by numbered TypeScript scripts. |
| `state.json` | Wrapper-level run metadata. |
| `checkpoints.json` | Completed phase markers used for resume. |
| `logs/wallets.log` | Wallet creation and funding wait log. |
| `logs/bootstrap.log` | Account, entitlement, funding, and pool setup log. |
| `logs/agent.log` | Runtime agent log. |
| `logs/full-fill.log` | Full-fill smoke submission log. |
| `logs/partial-fill.log` | Partial-fill smoke submission/recovery log. |
| `agent-chain-sync/` | Run-local agent chain-sync RocksDB database. Deleted before each agent start so the demo indexes from an empty local DB. |
| `agent-chain-sync.green-account-stores.json` | Persisted account store snapshots. Deleted before each agent start together with the RocksDB directory. |
| `demo-report.md` | Final auditor report. |

Generated wallet files contain testnet private material. They must not be
committed or shared as reusable wallets.

## Success Evidence

A passing run should have:

```bash
jq '.smoke.executionTxHash' demo/.state/runs/<run-id>/preprod-green-order-e2e.json
jq '.partialSmoke.executionTxHash' demo/.state/runs/<run-id>/preprod-green-order-e2e.json
jq '.partialSmoke.submittedIntentDigest' demo/.state/runs/<run-id>/preprod-green-order-e2e.json
jq '.partialSmoke.receivedAmount' demo/.state/runs/<run-id>/preprod-green-order-e2e.json
jq '.partial_fill, .verified' demo/.state/runs/<run-id>/checkpoints.json
```

The agent log must not contain:

```text
panic
Unsupported
invalid proof
RootMismatch
task abort
```

The wrapper checks this automatically in `demo/lib/verify.sh`.

## Demo Wrapper Scripts

### `demo/catalyst-demo.sh`

Main entry point for judges and auditors.

Use:

```bash
./demo/catalyst-demo.sh
./demo/catalyst-demo.sh --run-id <run-id>
./demo/catalyst-demo.sh --partial-only
./demo/catalyst-demo.sh --report-only --run-id <run-id>
```

Responsibilities:

- parse command-line flags;
- load shared environment;
- initialize or resume the run directory;
- call wallet, funding, bootstrap, agent, smoke, verify, and report phases;
- stop the agent on exit;
- print the final summary.

Important behavior:

- fresh mode creates new wallets;
- resume mode reuses the selected run directory;
- report-only mode does not submit transactions;
- if a partial-fill tx is already recorded, resume mode skips agent startup and
  completes verification/reporting from recorded state.

### `demo/lib/env.sh`

Environment and safety checks.

Responsibilities:

- set `REPO_ROOT`;
- set `DEMO_E2E_DIR`;
- default `KOIOS_PREPROD_URL`;
- require `bash`, `cargo`, `deno`, `jq`, and `curl`;
- reject mainnet.

This script runs before any chain mutation.

### `demo/lib/run.sh`

Run directory and checkpoint manager.

Responsibilities:

- parse common flags;
- create `DEMO_RUN_ID`;
- derive all run-local paths;
- create `demo/.state/runs/<run-id>/`;
- write wrapper state to `state.json`;
- maintain `checkpoints.json`.

Auditor notes:

- checkpoints make resume idempotent for completed phases;
- `--fresh` cannot be combined with `--run-id`;
- `--report-only` requires `--run-id`.

### `demo/lib/wallets.sh`

Wallet creation and funding wait.

Responsibilities:

- call `00-prepare-preprod-wallets.ts`;
- create new per-run wallets for fresh runs;
- reuse wallet files on resume;
- print the funding address;
- wait until enough tADA is detected.

Manual reviewer action:

```text
Send preprod tADA to the printed funding address.
```

The script currently asks for at least `120 tADA`. Sending more is fine.

### `demo/lib/funding.sh`

Funding preparation phase wrapper.

Responsibilities:

- mark funding-box preparation checkpoints;
- delegate actual UTxO preparation to the numbered E2E scripts;
- keep funding outputs run-local and derived from the freshly funded wallet.

No predefined wallet is required for reviewers.

### `demo/lib/bootstrap.sh`

On-chain setup orchestration.

Responsibilities:

- prepare operator funding;
- create the Aleph batch-witness entitlement;
- create the Aleph account;
- bind the account through the agent;
- create the Royalty V1 pool;
- create separator funding.

Primary scripts called:

- `07-prepare-operator-funding.ts`
- `02-create-entitlements.ts`
- `03-create-aleph-account-and-bind.ts`
- `01-create-royalty-v1-pool.ts`
- `08-create-separator-funding.ts`

Output:

```text
logs/bootstrap.log
preprod-green-order-e2e.json
```

### `demo/lib/agent.sh`

Agent lifecycle and readiness checks.

Responsibilities:

- generate the partial-fill agent config through
  `09-create-partial-agent-config.ts`;
- set run-local chain-sync DB paths;
- build `green-order-cardano-agent`;
- start the agent;
- wait for `/health`;
- wait until the agent observes and binds the account;
- verify the pool output is live;
- stop the agent on script exit.

Important outputs:

```text
config/preprod-partial-agent.json
logs/agent-config.log
logs/agent.log
agent-chain-sync/
agent-chain-sync.green-account-stores.json
```

### `demo/lib/smoke.sh`

Full-fill and partial-fill smoke phases.

Full-fill behavior:

- runs `04-submit-green-order-smoke.ts`;
- submits a normal green-order intent;
- waits for execution;
- records `.smoke.executionTxHash`.

Partial-fill behavior:

- builds `partial_preflight`;
- runs `10-submit-green-order-partial-smoke.ts`;
- submits a strict partial-fill intent;
- waits for old account and pool outputs to be spent;
- finds successor account and pool outputs;
- verifies account datum and pool reserve changes;
- verifies residual continuation store;
- records `.partialSmoke`.

Resume behavior:

- if `partial_fill` checkpoint exists, it reuses the result;
- if `.partialSmoke.executionTxHash` exists but checkpoint is missing, it marks
  the checkpoint and reuses the recorded tx.

### `demo/lib/verify.sh`

Post-run verification.

Checks:

- full-fill execution tx exists unless partial-only mode was selected;
- partial-fill execution tx exists;
- partial intent digest exists;
- partial store path exists;
- agent logs do not contain crash/proof/root-mismatch patterns.

Report-only validation:

- reads recorded tx hashes;
- checks that Koios returns confirmed tx info.

### `demo/lib/report.sh`

Report generator.

Writes:

```text
demo/.state/runs/<run-id>/demo-report.md
```

Report includes:

- run id;
- network;
- git commit;
- generated wallet addresses;
- full-fill tx;
- partial-fill tx;
- partial intent digest;
- received amount;
- store path;
- checkpoint status;
- recent agent log excerpt.

### `demo/tests/run-tests.sh`

Local wrapper tests.

Use:

```bash
demo/tests/run-tests.sh
```

Runs:

- `test_cli_and_state.sh`;
- `test_wallet_generation.sh`;
- `test_report_only.sh`.

These tests do not replace the live preprod demo. They validate wrapper CLI,
state, and report behavior.

## Numbered Preprod E2E Scripts

These scripts live under:

```text
green-order-cardano-agent/e2e/preprod/
```

The wrapper calls them with run-local environment variables. Auditors normally
run `demo/catalyst-demo.sh`, not these scripts directly.

### `00-prepare-preprod-wallets.ts`

Creates wallet state for a run.

Inputs:

- output paths provided by the wrapper;
- optional force flag from the wrapper for fresh runs.

Outputs:

- `wallets/preprod-wallets.json`;
- `wallets/.env.wallets`;
- funding address printed by the wrapper.

Purpose:

- avoid predefined reviewer wallets;
- create fresh wallet material for every fresh run.

### `01-create-royalty-v1-pool.ts`

Creates the Royalty V1 pool used by the green-order smoke tests.

Inputs:

- funded wallet state;
- deployment reference scripts;
- pool parameters from config/env.

Outputs:

- pool output ref;
- pool NFT id;
- LQ asset id;
- test token asset id;
- pool details in `preprod-green-order-e2e.json`.

### `02-create-entitlements.ts`

Creates or records the Aleph batch-witness entitlement.

Inputs:

- deployment data;
- current E2E state.

Outputs:

- entitlement entry in `preprod-green-order-e2e.json`.

Purpose:

- allow the demo account to use the batch witness required by the agent.

### `03-create-aleph-account-and-bind.ts`

Creates the Aleph account and binds it to the agent.

Inputs:

- entitlement state;
- funded wallet;
- agent URL;
- optional account hot private key env.

Outputs:

- account id;
- account output ref;
- account private/public key data in run-local state;
- account binding state.

Purpose:

- create the account that signs green-order intents;
- make the agent aware of the account id and output ref.

### `04-submit-green-order-smoke.ts`

Submits the normal full-fill green-order smoke intent.

Inputs:

- current account output;
- current pool output;
- agent URL;
- green-order parameters. By default it swaps 1 ADA for at least 900,000 units
  of the generated test token, which is intentionally close to the 1:1
  bootstrap pool price so the liquidity book can form a real recipe.

Outputs:

- `.smoke.submittedIntentDigest`;
- `.smoke.executionTxHash`.

Purpose:

- prove the agent can execute a normal green-order transaction.

### `05-check-preprod-wallets.ts`

Diagnostic wallet checker.

Purpose:

- inspect generated wallet balances and addresses;
- useful when a funding wait appears stuck.

This is not part of the normal judge flow.

### `06-deploy-aleph-reference-scripts.ts`

Deployment helper for Aleph reference scripts.

Purpose:

- deploy or refresh script references when preparing the environment.

This is infrastructure tooling, not a normal judge-flow step.

### `07-prepare-operator-funding.ts`

Prepares operator/batcher funding boxes.

Inputs:

- funded wallet;
- operator config;
- required amounts.

Outputs:

- UTxOs available for agent/operator transaction building.

Purpose:

- derive all operational funds from the reviewer-funded wallet;
- avoid requiring reviewers to create batcher/operator boxes manually.

### `08-create-separator-funding.ts`

Creates separator funding used by the demo execution path.

Inputs:

- funded wallet;
- current E2E state.

Outputs:

- separator/funding outputs recorded in state or logs.

Purpose:

- prepare the UTxOs needed by the execution transaction layout.

### `09-create-partial-agent-config.ts`

Generates the per-run agent config for partial-fill testing.

Inputs:

- deployment config;
- operator config;
- run-local DB path;
- chain-sync lookback/start settings.

Outputs:

```text
config/preprod-partial-agent.json
```

Purpose:

- enable partial-fill behavior;
- isolate chain-sync DB and account-store persistence per run;
- start the agent from an empty run-local RocksDB directory and empty account-store persistence file.

### `09-deploy-royalty-v1-reference-script.ts`

Deployment helper for Royalty V1 reference scripts.

Purpose:

- prepare reference-script UTxOs for environments where they are missing.

This is infrastructure tooling, not a normal judge-flow step.

### `10-preflight-green-order-partial-smoke.ts`

Computes or checks expected partial-fill math.

Purpose:

- validate the partial-fill quote and residual amounts before submission;
- useful for debugging partial-fill parameters.

### `10-submit-green-order-partial-smoke.ts`

Submits and verifies the partial-fill smoke intent.

Normal use through wrapper:

```bash
./demo/catalyst-demo.sh
```

Direct diagnostic verification of a known tx:

```bash
cd green-order-cardano-agent/e2e/preprod
STATE_PATH=/absolute/path/to/preprod-green-order-e2e.json \
AGENT_CONFIG_PATH=/absolute/path/to/preprod-partial-agent.json \
deno run --no-lock --allow-net --allow-read --allow-write --allow-env --allow-run \
  10-submit-green-order-partial-smoke.ts --verify-execution-tx=<tx-hash>
```

Normal mode responsibilities:

- build the signed partial-fill intent;
- submit it to the agent;
- wait for old account and pool outputs to be spent;
- find successor account and pool outputs;
- verify account datum advanced;
- verify pool reserves changed by the actual partial fill;
- verify the residual continuation store is persisted by the agent;
- write `.partialSmoke` into state.

Verification mode responsibilities:

- read spent old account/pool outputs from Koios;
- verify the specified execution tx produced the expected successor account and
  pool outputs;
- update state with the verified partial-fill execution tx.

### `run-preprod-e2e.sh`

Legacy/low-level preprod E2E runner.

Purpose:

- run the numbered scripts without the judge-friendly wrapper.

Auditors should prefer `demo/catalyst-demo.sh`.

### `run-preprod-partial-e2e.sh`

Legacy/low-level partial-fill E2E runner.

Purpose:

- run partial-fill testing directly without the full Catalyst wrapper.

Auditors should prefer `demo/catalyst-demo.sh --partial-only`.

## Troubleshooting

### Funding Wait Does Not Continue

Check the printed funding address and wallet log:

```bash
tail -n 80 demo/.state/runs/<run-id>/logs/wallets.log
```

The run can be resumed:

```bash
./demo/catalyst-demo.sh --run-id <run-id>
```

### Agent Does Not Become Healthy

Inspect:

```bash
tail -n 120 demo/.state/runs/<run-id>/logs/agent.log
cat demo/.state/runs/<run-id>/config/preprod-partial-agent.json
```

Then resume:

```bash
./demo/catalyst-demo.sh --run-id <run-id>
```

### Transaction Landed But Script Was Interrupted

Resume the run:

```bash
./demo/catalyst-demo.sh --run-id <run-id>
```

If the partial execution tx was already recorded in
`preprod-green-order-e2e.json`, the wrapper reuses it and completes the missing
checkpoint.

### Auditor Wants Read-Only Evidence

Use:

```bash
./demo/catalyst-demo.sh --report-only --run-id <run-id>
```

This validates recorded tx hashes through Koios and regenerates the report.

## Recommended Reviewer Checklist

1. Run `./demo/catalyst-demo.sh`.
2. Send preprod tADA to the printed address.
3. Wait for `Result: PASS`.
4. Open `demo/.state/runs/<run-id>/demo-report.md`.
5. Confirm full-fill and partial-fill tx hashes are present.
6. Confirm `.partialSmoke.receivedAmount` is present.
7. Confirm `checkpoints.json` contains `partial_fill` and `verified`.
8. Run report-only mode for the same run id.
9. Confirm report-only mode completes without starting the agent or submitting
   transactions.
