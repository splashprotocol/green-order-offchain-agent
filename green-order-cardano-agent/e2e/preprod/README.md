# Preprod Green Order E2E

These scripts create live preprod state for one Royalty V1 pool and one Aleph green account, then submit one
full-fill green intent through the local agent HTTP endpoint using the local TypeScript SDK. The default
full-fill smoke order swaps 1 ADA for at least 900,000 generated test-token units.

Copy `.env.example` to `.env` and fill:

- `BLOCKFROST_PROJECT_ID`, optional but recommended for preprod provider stability
- `FUNDED_WALLET_SEED`
- `CARDANO_NODE_SOCKET_PATH`
- `OPERATOR_KEY_HASH_HEX`, matching the running agent operator payment key hash
- optionally `ACCOUNT_HOT_PRIVATE_KEY_HEX`

`CARDANO_NODE_SOCKET_PATH` must point to a running Cardano preprod node socket when starting the Rust agent.
`06-deploy-aleph-reference-scripts.ts` also requires `ALEPH_BLUEPRINT_PATH` if Aleph reference scripts need to
be deployed.

If `BLOCKFROST_PROJECT_ID` is not set in the environment, `.env`, `.env.wallets`, or `WALLET_ENV_PATH`, the
`run-preprod-e2e.sh` wrapper asks for a preprod Blockfrost project id at startup. Press Enter to use Koios
instead. The prompted value is exported only for that process and is not written to committed files.

The wrapper generates a fresh run-local HMAC secret for every local-agent run, stores it only in ignored
`.state/preprod-hmac.env`, writes it into the generated agent config as `greenOrdersHmacAuth`, and passes it
to the TypeScript SDK. The key id is the fixed non-secret label `preprod-e2e`. Do not hardcode HMAC secrets in
scripts or committed config.
The runner also submits one SDK monitoring request with an intentionally wrong HMAC secret and expects
`403 Forbidden`; successful runs print `bad_hmac_check=passed`.

To create local preprod wallets for the agent/batcher and reference-script deployment:

```bash
deno run --no-lock --allow-read --allow-write --allow-env 00-prepare-preprod-wallets.ts
```

The script writes ignored local secrets to `.state/preprod-wallets.json` and `.env.wallets`. The batcher
wallet request defaults to 400 tADA because the live E2E funds operator collateral, four operator funding
UTxOs, pool/account creation, and smoke transactions.

The `run-preprod-e2e.sh` wrapper also runs wallet preparation automatically if `FUNDED_WALLET_SEED` and
`BATCHER_ADDRESS` are not present in the environment, `.env`, `.env.wallets`, or `WALLET_ENV_PATH`. On a fresh
auditor machine it prints the generated batcher address and waits until funding is observed, then continues
the same run.

With `FORCE_E2E_STATE=1`, the wrapper generates a fresh run-local wallet env at `.state/preprod-wallets.env`
and ignores any previous `.env.wallets` batcher for the run. Set `E2E_REUSE_WALLET_ENV=1` only when you
explicitly want a forced run to reuse an existing funded wallet env.

Prepare operator collateral/funding first. For the Catalyst demo wrapper, this script writes to a run-local
copy of the agent config. If you run it directly, set `AGENT_CONFIG_PATH` to an explicit writable config path
instead of mutating the committed template.

```bash
deno run --no-lock --allow-net --allow-read --allow-write --allow-env 07-prepare-operator-funding.ts
```

The `run-preprod-e2e.sh` wrapper starts a run-local Green Order agent by default. It copies the committed
preprod config to `.state/`, updates the operator key and node socket there, creates a partial-fill runtime
config, starts `target/debug/green-order-cardano-agent`, waits for `AGENT_HEALTH_URL`, and stops the process
on exit. Set `START_GREEN_ORDER_AGENT=0` to use an externally managed agent instead.

If running individual scripts manually, start the green-order agent with
`green-order-cardano-agent/resources/preprod.config.json` or a writable copy and wait for health before
creating pool/account state and submitting the smoke order.

```bash
deno run --no-lock --allow-net --allow-read --allow-write --allow-env 01-create-royalty-v1-pool.ts
deno run --no-lock --allow-net --allow-read --allow-write --allow-env 02-create-entitlements.ts
deno run --no-lock --allow-net --allow-read --allow-write --allow-env 03-create-aleph-account-and-bind.ts
deno run --no-lock --allow-net --allow-read --allow-write --allow-env 08-create-separator-funding.ts
deno run --no-lock --allow-net --allow-read --allow-write --allow-env 04-submit-green-order-smoke.ts
```

The `run-preprod-e2e.sh` wrapper builds `sdk/typescript` before the first script that calls the agent, so
`03-create-aleph-account-and-bind.ts`, `04-submit-green-order-smoke.ts`, and
`10-submit-green-order-partial-smoke.ts` exercise the SDK client for `bindAccount` and `submitIntent`.

To demonstrate all read-only SDK endpoints available on `AGENT_URL` after an account is bound, run:

```bash
npm --prefix ../../../sdk/typescript run build
deno run --no-lock --allow-net --allow-read --allow-env 11-query-agent-via-sdk.ts
```

The query script calls `getMonitoringReadiness`, `getMonitoringSummary`, `getAccount`, and `getAccountStatus`.
The SDK `getReadiness` method targets `/health` on the client base URL, while this preprod harness keeps
health checks on `AGENT_HEALTH_URL`.

The scripts persist local state under `.state/`. Removing `.state/` resets only the local harness; it does not
undo preprod transactions.
