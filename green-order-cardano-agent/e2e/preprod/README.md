# Preprod Green Order E2E

These scripts create live preprod state for one Royalty V1 pool and one Aleph green account, then submit one
full-fill green intent through the local agent HTTP endpoint.

Copy `.env.example` to `.env` and fill:

- `BLOCKFROST_PROJECT_ID`
- `FUNDED_WALLET_SEED`
- `OPERATOR_KEY_HASH_HEX`, matching the running agent operator payment key hash
- optionally `ACCOUNT_HOT_PRIVATE_KEY_HEX`

To create local preprod wallets for the agent/batcher and reference-script deployment:

```bash
deno run --no-lock --allow-read --allow-write --allow-env 00-prepare-preprod-wallets.ts
```

The script writes ignored local secrets to `.state/preprod-wallets.json` and `.env.wallets`.

Prepare operator collateral/funding first. This script writes the operator key and minimum funding threshold into
`green-order-cardano-agent/resources/preprod.config.json`, so restart the agent after this step if it is already
running.

```bash
deno run --no-lock --allow-net --allow-read --allow-write --allow-env 07-prepare-operator-funding.ts
```

Then run the green-order agent with `green-order-cardano-agent/resources/preprod.config.json` and wait for health
before creating pool/account state and submitting the smoke order.

```bash
deno run --no-lock --allow-net --allow-read --allow-write --allow-env 01-create-royalty-v1-pool.ts
deno run --no-lock --allow-net --allow-read --allow-write --allow-env 02-create-entitlements.ts
deno run --no-lock --allow-net --allow-read --allow-write --allow-env 03-create-aleph-account-and-bind.ts
deno run --no-lock --allow-net --allow-read --allow-write --allow-env 08-create-separator-funding.ts
deno run --no-lock --allow-net --allow-read --allow-write --allow-env 04-submit-green-order-smoke.ts
```

The scripts persist local state under `.state/`. Removing `.state/` resets only the local harness; it does not
undo preprod transactions.
