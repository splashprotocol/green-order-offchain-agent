# Preprod Green Order E2E Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add repeatable preprod E2E scripts that create a Royalty V1 pool, create and bind one Aleph green account, verify the required Aleph batch-witness entitlement, then submit and verify a full-fill green order against the pool.

**Architecture:** Keep preprod E2E outside normal CI because it needs funded keys, Blockfrost/Maestro access, a running synced green-order agent, and live preprod transactions. Build a small Deno/Lucid harness under `green-order-cardano-agent/e2e/preprod` with shared config/state helpers and three user-facing setup scripts: create pool, create the Aleph batch-witness entitlement record, then create and bind the Aleph account using that entitlement. Add one smoke runner that uses those setup artifacts to submit an HTTP intent and verify the account and pool UTxOs changed as expected.

**Tech Stack:** Deno, TypeScript, `@lucid-evolution/lucid`, `@anastasia-labs/cardano-multiplatform-lib-nodejs`, existing `green-order-cardano-agent/resources/preprod.config.json`, existing `green-order-cardano-agent/resources/preprod.deployment.json`, agent HTTP endpoints `/accounts/bind` and `/intents`.

---

## Constraints And Assumptions

- Do not modify Aleph on-chain source. The scripts must build datums/redeemers compatible with the deployed preprod validators already listed in `green-order-cardano-agent/resources/preprod.deployment.json`.
- The existing `splash-testing-cardano/src/lucid.ts` is mainnet-only, so create a preprod helper instead of extending that file in place.
- The green-order agent must already be running on preprod with `greenOrders.intentSource.httpListenAddr` enabled, usually `127.0.0.1:9031`.
- The scripts should persist generated refs and asset IDs into ignored local state, so re-running script 2 or 3 does not accidentally create a second account for the same test run.
- “Entitlements” for the first smoke means the Aleph account allowlist entry that authorizes the deployed `alephBatchWitness` script. This must be created as local setup state before the Aleph account is created, then embedded into the account datum before `/accounts/bind` is called.
- The first smoke path should be full-fill only: ADA leaves the Aleph account, a native token arrives from a Royalty V1 pool, `greenOrders.allowPartial = false`, and `execution.o2oAllowed = false`.
- The preprod deployment file must contain real reference-script UTxOs for `alephAccount`, `alephBatchWitness`, and the selected Royalty V1 validator. Placeholder all-zero references are not usable for live execution.

## Target File Layout

Create:

- `green-order-cardano-agent/e2e/preprod/README.md`
- `green-order-cardano-agent/e2e/preprod/.env.example`
- `green-order-cardano-agent/e2e/preprod/deno.json`
- `green-order-cardano-agent/e2e/preprod/01-create-royalty-v1-pool.ts`
- `green-order-cardano-agent/e2e/preprod/02-create-entitlements.ts`
- `green-order-cardano-agent/e2e/preprod/03-create-aleph-account-and-bind.ts`
- `green-order-cardano-agent/e2e/preprod/04-submit-green-order-smoke.ts`
- `green-order-cardano-agent/e2e/preprod/run-preprod-e2e.sh`
- `green-order-cardano-agent/e2e/preprod/src/config.ts`
- `green-order-cardano-agent/e2e/preprod/src/state.ts`
- `green-order-cardano-agent/e2e/preprod/src/lucid.ts`
- `green-order-cardano-agent/e2e/preprod/src/agent.ts`
- `green-order-cardano-agent/e2e/preprod/src/aleph.ts`
- `green-order-cardano-agent/e2e/preprod/src/royalty_pool_v1.ts`
- `green-order-cardano-agent/e2e/preprod/src/intent.ts`
- `green-order-cardano-agent/e2e/preprod/src/wait.ts`
- `green-order-cardano-agent/e2e/preprod/tests/state.test.ts`
- `green-order-cardano-agent/e2e/preprod/tests/wire.test.ts`

Modify:

- `.gitignore`
- `README.md`

---

### Task 1: Add Preprod E2E Config, State, And Ignore Rules

**Files:**
- Create: `green-order-cardano-agent/e2e/preprod/.env.example`
- Create: `green-order-cardano-agent/e2e/preprod/deno.json`
- Create: `green-order-cardano-agent/e2e/preprod/src/config.ts`
- Create: `green-order-cardano-agent/e2e/preprod/src/state.ts`
- Test: `green-order-cardano-agent/e2e/preprod/tests/state.test.ts`
- Modify: `.gitignore`

**Step 1: Write failing state tests**

Create `tests/state.test.ts` with tests for:

- New state starts with empty `pool`, `account`, and `entitlements`.
- Saving then loading preserves `pool.outputRef`, `account.accountId`, `account.outputRef`, and `entitlements`.
- Loader rejects a state file with malformed hex for tx hashes or account ids.

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno test --allow-read --allow-write tests/state.test.ts
```

Expected: FAIL because helpers do not exist.

**Step 2: Implement `.env.example`**

Include only non-secret defaults and secret names:

```dotenv
NETWORK=Preprod
BLOCKFROST_PREPROD_URL=https://cardano-preprod.blockfrost.io/api/v0
BLOCKFROST_PROJECT_ID=
FUNDED_WALLET_SEED=
AGENT_URL=http://127.0.0.1:9031
AGENT_HEALTH_URL=http://127.0.0.1:9024/health
DEPLOYMENT_PATH=../../resources/preprod.deployment.json
AGENT_CONFIG_PATH=../../resources/preprod.config.json
STATE_PATH=.state/preprod-green-order-e2e.json
OPERATOR_KEY_HASH_HEX=
ACCOUNT_HOT_PRIVATE_KEY_HEX=
```

**Step 3: Implement config loader**

`src/config.ts` must:

- Read env vars with `Deno.env.get`.
- Require `BLOCKFROST_PROJECT_ID` and `FUNDED_WALLET_SEED`.
- Require `OPERATOR_KEY_HASH_HEX`, matching the 28-byte payment key hash of the running agent operator credential.
- Require `ACCOUNT_HOT_PRIVATE_KEY_HEX` or generate one exactly once in `03-create-aleph-account-and-bind.ts` and persist it to ignored local state before submitting the account creation transaction. Script 4 must verify the private key derives `state.account.mainKeyHex` before signing the smoke intent.
- Default paths relative to the e2e directory.
- Load and parse `preprod.deployment.json`.
- Fail fast if `alephAccount`, `alephBatchWitness`, or `royaltyPool` are missing.
- Fail fast if `alephAccount.referenceUtxo`, `alephBatchWitness.referenceUtxo`, or the selected Royalty V1 validator reference UTxO is absent or has an all-zero transaction hash.
- Fail fast if the configured runtime operator key hash cannot be matched to the HTTP intent `operatorKeyHash` that the script will submit. The smoke must not use a random or placeholder operator key hash because the executor validates it against the running agent operator credential.

If the deployment has placeholder Aleph references, stop before any chain transaction and instruct the operator to deploy/reference the Aleph scripts and update `green-order-cardano-agent/resources/preprod.deployment.json`. The E2E scripts must not try to work around missing reference scripts.

**Step 4: Implement state helper**

`src/state.ts` must define:

```ts
export type OutputRef = { txHash: string; outputIndex: number };
export type PreprodE2eState = {
  pool?: {
    outputRef: OutputRef;
    validator: "royaltyPool" | "royaltyPoolLedgerFixed";
    poolNft: string;
    assetLq: string;
    assetX: string;
    assetY: string;
    initialLovelace: string;
    initialTokenAmount: string;
    depositedLq: string;
  };
  account?: {
    accountId: string;
    outputRef: OutputRef;
    hotPrivateKeyHex: string;
    mainKeyHex: string;
    coldKeyHash: string;
    initialStoreRoot: string;
    batchWitnessHash: string;
  };
  pendingAccount?: {
    accountId: string;
    outputRef?: OutputRef;
    hotPrivateKeyHex: string;
    mainKeyHex: string;
    coldKeyHash: string;
    initialStoreRoot: string;
    batchWitnessHash: string;
  };
  entitlements?: Array<{ kind: "alephBatchWitnessAllowlist"; hash: string; accountOutputRef?: OutputRef }>;
  smoke?: {
    submittedIntentDigest?: string;
    executionTxHash?: string;
  };
};
```

Validate lengths for `txHash` and `accountId` as 64 hex chars. Validate `pool.poolNft`, `pool.assetLq`, `pool.assetX`, `pool.assetY`, `account.hotPrivateKeyHex`, `pendingAccount.hotPrivateKeyHex`, `account.mainKeyHex`, `pendingAccount.mainKeyHex`, and entitlement hashes as hex strings with the lengths required by the corresponding Cardano type.

**Step 5: Update `.gitignore`**

Ignore:

```gitignore
green-order-cardano-agent/e2e/preprod/.env
green-order-cardano-agent/e2e/preprod/.state/
```

**Step 6: Verify and commit**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno fmt
deno test --allow-read --allow-write tests/state.test.ts
```

Expected: PASS.

Commit:

```bash
git add .gitignore green-order-cardano-agent/e2e/preprod
git commit -m "test: add preprod e2e state harness"
```

---

### Task 2: Add Preprod Lucid, Waiting, And Agent HTTP Helpers

**Files:**
- Create: `green-order-cardano-agent/e2e/preprod/src/lucid.ts`
- Create: `green-order-cardano-agent/e2e/preprod/src/wait.ts`
- Create: `green-order-cardano-agent/e2e/preprod/src/agent.ts`
- Test: `green-order-cardano-agent/e2e/preprod/tests/wire.test.ts`

**Step 1: Write failing wire tests**

Add tests for:

- `buildBindAccountPayload(accountId, outputRef)` returns the exact JSON required by `/accounts/bind`.
- `buildIntentPayload(...)` encodes ADA as `"00"` and native assets as `policyId || assetName`.
- `parseAgentAcceptedResponse` accepts `{ "status": "accepted", "reason": null }`.
- `parseAgentBoundResponse` accepts `{ "status": "bound", "reason": null }`.

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno test --allow-read tests/wire.test.ts
```

Expected: FAIL.

**Step 2: Implement Lucid helper**

`src/lucid.ts` must:

- Build `Lucid(new Blockfrost(config.blockfrostUrl, config.blockfrostProjectId), "Preprod")`.
- Select the funded wallet from `FUNDED_WALLET_SEED`.
- Export `submitAndWait(tx)` that signs, submits, and polls until confirmed.

**Step 3: Implement wait helper**

`src/wait.ts` must:

- Poll transaction confirmation via Lucid provider.
- Poll UTxO appearance at a script address.
- Poll agent health URL.
- Use bounded timeouts and print the last observed status on failure.

**Step 4: Implement agent helper**

`src/agent.ts` must:

- POST JSON to `/accounts/bind`.
- POST JSON to `/intents`.
- Convert non-2xx responses into errors that include HTTP status and `reason`.
- Expose pure payload builders used by tests.

**Step 5: Verify and commit**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno fmt
deno test --allow-read tests/wire.test.ts
```

Expected: PASS.

Commit:

```bash
git add green-order-cardano-agent/e2e/preprod
git commit -m "test: add preprod e2e http helpers"
```

---

### Task 3: Script 1, Create Royalty V1 Pool

**Files:**
- Create: `green-order-cardano-agent/e2e/preprod/src/royalty_pool_v1.ts`
- Create: `green-order-cardano-agent/e2e/preprod/01-create-royalty-v1-pool.ts`

**Step 1: Add a dry-run mode first**

`01-create-royalty-v1-pool.ts --dry-run` must:

- Load config and state.
- Refuse to run if `state.pool` already exists unless `--force` is provided.
- Print the selected `royaltyPool` validator hash/reference UTxO from deployment.
- Reject selected validator reference UTxOs whose transaction hash is all zeros.
- Print the intended asset pair and initial reserves.

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno run --allow-read --allow-env 01-create-royalty-v1-pool.ts --dry-run
```

Expected: PASS with no network submission.

**Step 2: Implement asset setup**

For the first preprod smoke, use:

- `assetX = ADA`
- `assetY = freshly minted native test token`

Mint the test token in the same transaction if the Royalty V1 creation flow supports it. If not, split into an internal mint step and save the minted asset in state before building the pool transaction.

The pool setup must also mint and store:

- `poolNft`: the unique pool NFT consumed by `RoyaltyPoolConfig`.
- `assetLq`: the liquidity token asset class.
- `depositedLq`: the LQ token amount placed at the pool output, so the parser can compute `MAX_LQ_CAP - liquidity_neg`.

**Step 3: Implement Royalty V1 datum/output builder**

Use the exact datum schema consumed by `bloom-offchain-cardano/src/pools/royalty_v1.rs`:

- Version: Royalty V1 or V1 ledger-fixed, matching `preprod.deployment.json`.
- Asset classes: ADA and test token.
- Pool NFT and LQ asset classes matching the minted assets.
- LQ token amount present at the pool output.
- Pure reserves plus treasury/royalty fields.
- Royalty pub key set from the funded wallet payment key.
- Fees small enough for a deterministic smoke swap.

Do not hand-roll CBOR strings. Use Lucid/CML Plutus data constructors.

Before live submission, add a local parser round-trip check:

- Build the exact transaction output that will be submitted.
- Decode it with the same shape expected by `RoyaltyPoolConfig`.
- Verify the fields required by `RoyaltyV1PoolOnly::try_from_ledger`: pool NFT, `asset_x`, `asset_y`, `asset_lq`, non-zero pure reserves, sufficient lovelace, LQ amount, fee fields, royalty pub key, and selected validator hash.
- Fail the dry run if the round-trip does not produce the expected pair and reserves.

**Step 4: Submit and save state**

The script must:

- Submit the transaction.
- Wait for confirmation.
- Find the pool output at the Royalty V1 script address.
- Save `state.pool.outputRef`, `validator`, `poolNft`, `assetLq`, `assetX`, `assetY`, `depositedLq`, and initial reserve values.

**Step 5: Verify manually on preprod**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno run --allow-net --allow-read --allow-write --allow-env 01-create-royalty-v1-pool.ts
```

Expected: state file contains `pool.outputRef` and the output exists on preprod.

Commit:

```bash
git add green-order-cardano-agent/e2e/preprod
git commit -m "test: add preprod royalty v1 pool setup script"
```

---

### Task 4: Script 2, Create Entitlement Record

**Files:**
- Create: `green-order-cardano-agent/e2e/preprod/02-create-entitlements.ts`
- Modify: `green-order-cardano-agent/e2e/preprod/src/state.ts`

**Step 1: Add a dry-run mode first**

`02-create-entitlements.ts --dry-run` must:

- Load deployment and state.
- Refuse to overwrite `state.entitlements` unless `--force` is provided.
- Reject `alephBatchWitness` reference UTxO whose transaction hash is all zeros.
- Print the entitlement hash that will be embedded into the Aleph account allowlist.

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno run --allow-read --allow-env 02-create-entitlements.ts --dry-run
```

Expected: PASS with no network submission.

**Step 2: Create the entitlement record**

For this smoke, the entitlement is fixed: the account datum must allow the deployed `alephBatchWitness` script hash. The script must:

- Load `deployment.alephBatchWitness.hash`.
- Validate it is a 28-byte script hash.
- Save `entitlements: [{ kind: "alephBatchWitnessAllowlist", hash }]`.
- Do not submit a transaction.

This script creates local setup state that Task 5 embeds into the account datum. It must run before account creation because the bound account must not be rewritten by setup scripts.

**Step 3: Verify manually**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno run --allow-read --allow-write --allow-env 02-create-entitlements.ts
```

Expected: state contains an `entitlements` array with the `alephBatchWitnessAllowlist` entry and no account UTxO was created or spent.

Commit:

```bash
git add green-order-cardano-agent/e2e/preprod
git commit -m "test: add preprod entitlement setup script"
```

---

### Task 5: Script 3, Create Aleph Account And Bind It

**Files:**
- Create: `green-order-cardano-agent/e2e/preprod/src/aleph.ts`
- Create: `green-order-cardano-agent/e2e/preprod/03-create-aleph-account-and-bind.ts`

**Step 1: Add a dry-run mode first**

`03-create-aleph-account-and-bind.ts --dry-run` must:

- Load deployment and state.
- Require `state.entitlements` to contain exactly one `alephBatchWitnessAllowlist` record matching `deployment.alephBatchWitness.hash`.
- Refuse to create a new account if `state.account` already exists unless `--force` is provided.
- If `state.pendingAccount.outputRef` exists, resume from binding that output instead of creating another account.
- Print Aleph account script hash/reference UTxO.
- Reject `alephAccount` or `alephBatchWitness` reference UTxOs whose transaction hash is all zeros.
- Print generated `accountId`, initial empty MPF root, and planned account lovelace.

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno run --allow-read --allow-env 03-create-aleph-account-and-bind.ts --dry-run
```

Expected: PASS with no network submission.

**Step 2: Implement Aleph account datum builder**

Build `AlephAccountState` compatible with `bloom-offchain-cardano/src/orders/green.rs`:

- `magic`: exact bytes `01`, matching the deployed Aleph account constant, not the `green-test` parser fixture used in Rust unit tests.
- `allowlist`: include the 28-byte hash from `state.entitlements[0]`, and assert it equals `deployment.alephBatchWitness.hash`.
- `nonce`: `[0]`.
- `main_key`: 33-byte compressed secp256k1 public key derived from the account hot ECDSA private key.
- `co_key`: empty.
- `cold_key_hash`: 28-byte hash derived from the configured cold key/payment credential.
- `store_root`: MPF empty root from the Rust implementation.

Add a small Rust/TypeScript parity check if needed: the hex of the empty root must equal `green-order-cardano-agent/src/account_store.rs::ALEPH_MPF_EMPTY_ROOT`.
Add a known-vector signing test from the Aleph witness tests before live use: compute `blake2b_256(prefix || cbor(AlephIntention) || postfix)`, sign it with the hot secp256k1 key, and verify the compact 64-byte ECDSA signature against `main_key`.
If `ACCOUNT_HOT_PRIVATE_KEY_HEX` is provided, derive `main_key` from it. Otherwise generate a new secp256k1 private key exactly once. In both cases, save `state.pendingAccount` with `hotPrivateKeyHex`, `mainKeyHex`, `accountId`, `coldKeyHash`, `initialStoreRoot`, and `batchWitnessHash` before building or submitting the account transaction, and reuse that pending record for all retries.

**Step 3: Submit account creation transaction**

The script must:

- Pay enough ADA to the Aleph account script for the leaving amount, fee lovelace, min ADA, and execution buffer.
- Require `state.pendingAccount` and build the datum from it; do not generate a new key after this point.
- Attach the inline datum.
- Submit and wait for confirmation.
- Save the confirmed account output ref into `state.pendingAccount.outputRef`.
- Do not promote `pendingAccount` into `account` yet; binding has not succeeded.
- Update the entitlement record with `accountOutputRef` after the account output is known.

**Step 4: Wait for agent indexing**

Before binding, poll the stateful `/accounts/bind` request carefully:

- Submit the bind request with `state.pendingAccount.accountId` and `state.pendingAccount.outputRef`.
- If the response is `outputNotObserved`, wait and retry until timeout.
- If the response is `bound`, promote `state.pendingAccount` into `state.account`, persist it, remove `pendingAccount`, and stop polling.
- If the response is `alreadyBound`, treat it as success only when `state.account` already records the exact same `accountId` and output ref, or when `state.pendingAccount.outputRef` exists for the same request that may have succeeded before a crash; in the latter case, promote the pending record into `account`.
- Any other rejection is a hard failure.

Do not use a separate speculative probe because `/accounts/bind` mutates state on the first successful call.

**Step 5: Bind account once**

Call:

```http
POST /accounts/bind
{
  "accountId": "<64 hex chars>",
  "txHash": "<account creation tx hash>",
  "outputIndex": <account output index>
}
```

Expected response:

```json
{"status":"bound","reason":null}
```

This step is implemented by the polling loop above. Do not call `/accounts/bind` again after receiving `bound`.

**Step 6: Verify manually on preprod**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno run --allow-net --allow-read --allow-write --allow-env 03-create-aleph-account-and-bind.ts
```

Expected: state file contains `account`, and the agent accepts the bind.

Commit:

```bash
git add green-order-cardano-agent/e2e/preprod
git commit -m "test: add preprod aleph account binding script"
```

---

### Task 6: Submit Full-Fill Green Order Smoke

**Files:**
- Create: `green-order-cardano-agent/e2e/preprod/src/intent.ts`
- Create: `green-order-cardano-agent/e2e/preprod/04-submit-green-order-smoke.ts`

**Step 1: Implement pure intent builder tests first**

Add tests in `tests/wire.test.ts` for:

- Account id is 32 bytes hex.
- `originalIntentDigest` is computed from the Aleph intention Plutus data, not random input.
- Signature is 64 bytes.
- `targetNonceSlot = 0`, `targetNonceValue = 1` for the first smoke.

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno test --allow-read tests/wire.test.ts
```

Expected: FAIL until `src/intent.ts` exists.

**Step 2: Build and sign the intent**

The smoke intent must:

- Spend ADA from the bound Aleph account.
- Receive the pool test token.
- Set `feeLovelace` high enough for agent execution fee assumptions.
- Use `operatorKeyHash` from `OPERATOR_KEY_HASH_HEX`, matching the running green-order agent operator credential.
- Use `auth.type = "sig"` with empty `updateProof`.
- Sign exactly `blake2b_256(prefix || cbor(AlephIntention) || postfix)` with the account hot secp256k1 private key whose compressed public key is stored in the account datum. The submitted signature must be compact 64-byte ECDSA, and the submitted `prefix`/`postfix` must be the same bytes used to build the signed message.

**Step 3: Submit to `/intents`**

Call the existing endpoint:

```http
POST /intents
```

Expected:

```json
{"status":"accepted","reason":null}
```

**Step 4: Verify chain effects**

Poll preprod until:

- The old Aleph account output is spent.
- A new Aleph account output exists.
- The account nonce at index `0` is at least `1`.
- The account value lost the leaving ADA plus fee and gained the expected token.
- The Royalty V1 pool output ref changed.
- The pool reserves moved in the opposite direction.

Save `state.smoke.submittedIntentDigest` and, if discoverable, `state.smoke.executionTxHash`.

**Step 5: Verify manually on preprod**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno run --allow-net --allow-read --allow-write --allow-env 04-submit-green-order-smoke.ts
```

Expected: the script exits `0` only after it observes the execution effects above.

Commit:

```bash
git add green-order-cardano-agent/e2e/preprod
git commit -m "test: add preprod green order smoke script"
```

---

### Task 7: Add Sequential Runner And Documentation

**Files:**
- Create: `green-order-cardano-agent/e2e/preprod/run-preprod-e2e.sh`
- Create: `green-order-cardano-agent/e2e/preprod/README.md`
- Modify: `README.md`

**Step 1: Implement runner**

`run-preprod-e2e.sh` must run:

```bash
deno run --allow-net --allow-read --allow-write --allow-env 01-create-royalty-v1-pool.ts
deno run --allow-net --allow-read --allow-write --allow-env 02-create-entitlements.ts
deno run --allow-net --allow-read --allow-write --allow-env 03-create-aleph-account-and-bind.ts
deno run --allow-net --allow-read --allow-write --allow-env 04-submit-green-order-smoke.ts
```

**Step 2: Document manual preconditions**

`README.md` must explain:

- How to fund the preprod wallet.
- How to start the green-order agent with preprod config.
- How to wait for health/sync.
- How to run each script independently.
- How to reset only local E2E state without touching chain state.
- That the scripts submit real preprod transactions.

**Step 3: Add top-level pointer**

Add a short link from repository `README.md` to `green-order-cardano-agent/e2e/preprod/README.md`.

**Step 4: Verify docs and shell syntax**

Run:

```bash
bash -n green-order-cardano-agent/e2e/preprod/run-preprod-e2e.sh
cd green-order-cardano-agent/e2e/preprod
deno fmt
deno check 01-create-royalty-v1-pool.ts 02-create-entitlements.ts 03-create-aleph-account-and-bind.ts 04-submit-green-order-smoke.ts
deno test --allow-read --allow-write tests
```

Expected: all pass without submitting transactions.

Commit:

```bash
git add README.md green-order-cardano-agent/e2e/preprod
git commit -m "docs: document preprod green order e2e"
```

---

## Final Verification Checklist

Run local non-network verification:

```bash
cd green-order-cardano-agent/e2e/preprod
deno fmt --check
deno check 01-create-royalty-v1-pool.ts 02-create-entitlements.ts 03-create-aleph-account-and-bind.ts 04-submit-green-order-smoke.ts
deno test --allow-read --allow-write tests
cargo test -p green-order-cardano-agent http_intent_source::tests:: -- --nocapture
cargo test -p green-order-cardano-agent account_index::tests:: -- --nocapture
```

Run live preprod verification only with funded keys and a running synced agent:

```bash
cd green-order-cardano-agent/e2e/preprod
cp .env.example .env
# fill BLOCKFROST_PROJECT_ID, FUNDED_WALLET_SEED, operator key, and optionally ACCOUNT_HOT_PRIVATE_KEY_HEX
./run-preprod-e2e.sh
```

Expected final result:

- Script 1 creates and records one Royalty V1 pool.
- Script 2 creates the Aleph batch-witness allowlist entitlement record required by the smoke.
- Script 3 creates one Aleph account UTxO with that entitlement embedded and binds it through `/accounts/bind`.
- Script 4 submits one full-fill green intent through `/intents`.
- The live chain shows the account output and pool output both advanced after execution.

## Open Risks For The Implementer

- Royalty V1 datum construction is the highest-risk part. Verify against existing Spectrum data encoders before submitting any preprod transaction.
- Aleph signature prefix/postfix must match the on-chain validator’s exact signed-message rule. A malformed signature can still pass HTTP admission but fail during transaction validation.
- The Aleph batch-witness allowlist entitlement must be present before binding. Do not spend the bound account directly from setup scripts; recreate the account or add an agent-supported admin path if the entitlement is wrong.
- The first smoke should avoid partial fills until preprod proves the full-fill path, because partial path-auth requires MPF continuation proofs and more chain-state assertions.
