# Catalyst Demo Script Auditor Guide

This document explains the purpose, inputs, outputs, and safety properties of
the Catalyst demo scripts.

## Purpose

The demo proves, on Cardano preprod, that the green-order offchain agent can:

- create the required setup from freshly generated wallets;
- execute a normal full-fill green order intent;
- execute a strict partial-fill green order intent;
- persist the remaining partial-fill intent in the Aleph account store;
- continue operating after processing the partial-fill execution transaction.

The judge-facing interface is intentionally one command:

```bash
./demo/catalyst-demo.sh
```

The only manual action expected from the user is sending preprod tADA to the
funding address printed by the script.

## Run Directory

Each normal run creates a new directory:

```text
demo/.state/runs/<run-id>/
```

Contents:

- `wallets/`: generated wallet state and `.env.wallets`;
- `config/`: generated agent config;
- `logs/`: wallet, bootstrap, agent, and smoke logs;
- `agent-chain-sync/`: run-local RocksDB chain-sync cache, reset before each agent start;
- `agent-chain-sync.green-account-stores.json`: run-local account-store persistence, reset before each agent start;
- `state.json`: demo wrapper state;
- `checkpoints.json`: phase completion markers;
- `preprod-green-order-e2e.json`: existing E2E state consumed by numbered scripts;
- `demo-report.md`: judge/auditor report.

Wallets are generated per run. Existing wallet files are never overwritten when
resuming with `--run-id`.

## Script Inventory

### `demo/catalyst-demo.sh`

Main entry point. It parses flags, initializes a run directory, coordinates all
library scripts, and prints the final summary.

Supported flags:

- `--fresh`: create a new run with new wallets;
- `--partial-only`: skip the full-fill smoke phase;
- `--report-only --run-id <id>`: regenerate a report without mutating state;
- `--run-id <id>`: resume an existing run;
- `--help`: print usage.

Safety behavior:

- refuses mainnet through `env.sh`;
- stops the agent through a shell trap;
- keeps generated files under the run directory;
- does not mutate state in `--report-only` mode except writing the report.

### `demo/lib/env.sh`

Validates the execution environment.

Checks:

- required tools: `bash`, `cargo`, `deno`, `jq`, `curl`;
- expected network is preprod;
- Koios preprod URL default is available.

Failure mode:

- exits before any transaction if required tools are missing or network is
  unsafe.

### `demo/lib/run.sh`

Owns run ids, run directories, and checkpoints.

Responsibilities:

- parse common flags;
- create `demo/.state/runs/<run-id>/`;
- expose paths used by all other scripts;
- create and update `checkpoints.json`;
- reject ambiguous combinations like `--fresh --run-id`.

Checkpointing lets the user fund late or resume after interruption without
repeating completed phases.

### `demo/lib/wallets.sh`

Creates or loads the run-local demo wallets.

Responsibilities:

- call `00-prepare-preprod-wallets.ts` with run-specific output paths;
- force fresh wallet generation for new runs;
- print the funding address;
- wait for enough preprod tADA at that address.

The wallet generator writes:

- `FUNDED_WALLET_SEED`;
- `BATCHER_ADDRESS`;
- `DEPLOYMENT_WALLET_SEED`;
- `DEPLOYMENT_ADDRESS`.

These files are generated state and must not be committed.

### `demo/lib/funding.sh`

Represents the funding-box preparation phase.

The current implementation delegates actual splitting to the existing preprod
setup scripts, which consume the funded wallet and create the operator, pool,
account, separator, collateral, and reserve boxes as needed.

### `demo/lib/bootstrap.sh`

Creates the on-chain setup used by the smoke tests.

It calls existing numbered preprod scripts with run-local state:

- `07-prepare-operator-funding.ts`;
- `02-create-entitlements.ts`;
- `03-create-aleph-account-and-bind.ts`;
- `01-create-royalty-v1-pool.ts`;
- `08-create-separator-funding.ts`.

The wrapper starts the agent before this phase because
`03-create-aleph-account-and-bind.ts` binds the newly created account through
the agent HTTP API.

Outputs are captured in `logs/bootstrap.log` and the E2E state file.

### `demo/lib/agent.sh`

Owns agent lifecycle.

Responsibilities:

- create a temporary partial-fill-enabled preprod agent config;
- place chain-sync DB under the run directory;
- start `green-order-cardano-agent`;
- wait for the health endpoint;
- after bootstrap, verify `/accounts/status` can see the account output;
- verify the pool output ref is live on Koios and the agent chain sync has
  reached tip before smoke submission;
- capture logs under `logs/agent.log`;
- stop the agent on exit.

### `demo/lib/smoke.sh`

Runs the intent smoke phases.

Full-fill phase:

- calls `04-submit-green-order-smoke.ts`;
- records execution tx in the E2E state.

Partial-fill phase:

- builds `partial_preflight`;
- calls `10-submit-green-order-partial-smoke.ts`;
- records partial execution tx, intent digest, received amount, and store path.

### `demo/lib/verify.sh`

Performs post-execution verification.

Checks:

- full-fill execution tx exists when full-fill ran;
- partial-fill execution tx exists;
- partial intent digest exists;
- persisted store path exists in state;
- agent logs do not contain unexpected crash patterns such as `panic`,
  `Unsupported`, invalid proof/root mismatch, or task abort.

The partial smoke script performs the deeper domain checks: old UTxOs spent,
successor UTxOs created, account/store root consistency, and residual state.

### `demo/lib/report.sh`

Generates `demo-report.md`.

Report includes:

- result mode (`PASS` or `REPORT ONLY`);
- run id;
- git commit;
- network;
- wallet addresses;
- full-fill and partial-fill tx hashes;
- partial intent digest and received amount;
- store path;
- checkpoint status;
- last agent log excerpt.

### `demo/tests/run-tests.sh`

Local non-spending test entry point.

Runs:

- `test_cli_and_state.sh`;
- `test_wallet_generation.sh`;
- `test_report_only.sh`.

These tests validate local wrapper behavior without submitting transactions.

## Expected Judge Flow

1. Run:

   ```bash
   ./demo/catalyst-demo.sh
   ```

2. Send preprod tADA to the printed funding address.
3. Wait for the script to continue.
4. Review final terminal summary and `demo-report.md`.

## Report-Only Flow

For audit reproduction without spending:

```bash
./demo/catalyst-demo.sh --report-only --run-id <run-id>
```

This mode is read-only except for regenerating `demo-report.md`. It validates
recorded transaction hashes against Koios preprod, and it must not start the
agent, submit transactions, or modify checkpoints.
