# Preprod Partial Fill Intent Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a deterministic preprod E2E smoke that submits a green order intent which is only partially filled, then proves the Aleph account successor carries the remaining intent in the MPF store.

**Architecture:** Reuse the existing preprod setup scripts, but run them with partial-specific env overrides and a temporary agent config where `greenOrders.allowPartial = true`. Add a new `10-submit-green-order-partial-smoke.ts` script that mirrors the successful full-fill smoke, but asserts account store root mutation, partial received amount, and persisted pending continuation state. Keep Aleph validators unchanged.

**Tech Stack:** Deno TypeScript E2E scripts, green-order-cardano-agent HTTP intent API, Koios preprod API, Aleph account datum CBOR parsing via Lucid `Data`, Rust green order agent runtime config.

---

### Task 1: Add Partial Fill Config Surface

**Files:**
- Modify: `green-order-cardano-agent/e2e/preprod/src/config.ts`
- Test: `green-order-cardano-agent/e2e/preprod/tests/config.test.ts`

**Step 1: Write failing config tests**

Create `green-order-cardano-agent/e2e/preprod/tests/config.test.ts` with tests for the new env-backed partial smoke fields. Import from `../src/config.ts`:

```ts
Deno.test("loadConfig reads partial smoke env overrides", async () => {
  const previous = snapshotEnv([
    "PARTIAL_LEAVING_LOVELACE",
    "PARTIAL_EXPECTED_TOKEN_AMOUNT",
    "PARTIAL_FEE_LOVELACE",
    "PARTIAL_EXECUTION_TIMEOUT_MS",
  ]);
  try {
    Deno.env.set("PARTIAL_LEAVING_LOVELACE", "20000000");
    Deno.env.set("PARTIAL_EXPECTED_TOKEN_AMOUNT", "10000");
    Deno.env.set("PARTIAL_FEE_LOVELACE", "2000000");
    Deno.env.set("PARTIAL_EXECUTION_TIMEOUT_MS", "900000");

    const config = await loadConfig();

    assertEquals(config.partialLeavingLovelace, 20_000_000n);
    assertEquals(config.partialExpectedTokenAmount, 10_000n);
    assertEquals(config.partialFeeLovelace, 2_000_000n);
    assertEquals(config.partialExecutionTimeoutMs, 900_000);
  } finally {
    restoreEnv(previous);
  }
});
```

Use local env snapshot helpers in the test file so the test does not leak env to other tests.

**Step 2: Run test and verify it fails**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno test --no-lock --allow-read --allow-env tests/config.test.ts
```

Expected: FAIL because `partialLeavingLovelace`, `partialExpectedTokenAmount`, `partialFeeLovelace`, and `partialExecutionTimeoutMs` do not exist yet.

**Step 3: Implement minimal config fields**

Add fields to `PreprodE2eConfig`:

```ts
partialLeavingLovelace: bigint;
partialExpectedTokenAmount: bigint;
partialFeeLovelace: bigint;
partialExecutionTimeoutMs: number;
```

Add values in `loadConfig()`:

```ts
partialLeavingLovelace: bigintEnv("PARTIAL_LEAVING_LOVELACE", 20_000_000n),
partialExpectedTokenAmount: bigintEnv("PARTIAL_EXPECTED_TOKEN_AMOUNT", 10_000n),
partialFeeLovelace: bigintEnv("PARTIAL_FEE_LOVELACE", 2_000_000n),
partialExecutionTimeoutMs: numberEnv("PARTIAL_EXECUTION_TIMEOUT_MS", 900_000),
```

Add a `numberEnv(name, fallback)` helper that accepts only decimal integers and returns a safe JS number.

**Step 4: Verify config tests pass**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno test --no-lock --allow-read --allow-env tests/config.test.ts
```

Expected: PASS.

**Step 5: Commit**

```bash
git add green-order-cardano-agent/e2e/preprod/src/config.ts green-order-cardano-agent/e2e/preprod/tests/config.test.ts
git commit -m "test: add partial fill e2e config"
```

### Task 2: Parse Store Root From Aleph Account Datum

**Files:**
- Modify: `green-order-cardano-agent/e2e/preprod/04-submit-green-order-smoke.ts`
- Create: `green-order-cardano-agent/e2e/preprod/src/account_datum.ts`
- Test: `green-order-cardano-agent/e2e/preprod/tests/account_datum.test.ts`

**Step 1: Extract parser test coverage**

Add `tests/account_datum.test.ts` using known current-account datum CBOR from the existing E2E state fixture if available. If no stable fixture exists, build a Lucid `Constr(0, [...])` datum in the test for current ABI. Import `parseAccountDatum` from `../src/account_datum.ts`:

```ts
const datum = Data.to(new Constr(0, [
  0n,
  "00".repeat(32),
  [1n, 0n, 0n, 0n],
  ["11".repeat(33)],
  ["22".repeat(33)],
  "33".repeat(32),
]));

const parsed = parseAccountDatum(datum, "current");
assertEquals(parsed.nonce0, 1n);
assertEquals(parsed.mainKeyHex, "11".repeat(33));
assertEquals(parsed.storeRootHex, "33".repeat(32));
```

Also keep legacy ABI coverage if the old parser currently supports it.

**Step 2: Run test and verify it fails**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno test --no-lock --allow-read --allow-env tests/account_datum.test.ts
```

Expected: FAIL because `src/account_datum.ts` does not exist.

**Step 3: Implement shared parser**

Move `parseAccountDatum` out of `04-submit-green-order-smoke.ts` into `src/account_datum.ts`.

Expose:

```ts
export type ParsedAccountDatum = {
  nonce0: bigint;
  mainKeyHex: string;
  storeRootHex?: string;
};

export function parseAccountDatum(
  datumCbor: string,
  alephAbi: "current" | "legacy",
): ParsedAccountDatum;
```

For current ABI, read `storeRootHex` from field `5`. For legacy ABI, leave `storeRootHex` undefined unless the legacy shape has a confirmed store-root field.

Update `04-submit-green-order-smoke.ts` to import the parser and remove its local copy.

**Step 4: Verify smoke still checks**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno check --no-lock 04-submit-green-order-smoke.ts src/account_datum.ts tests/account_datum.test.ts
deno test --no-lock --allow-read --allow-env tests/account_datum.test.ts
```

Expected: PASS.

**Step 5: Commit**

```bash
git add green-order-cardano-agent/e2e/preprod/04-submit-green-order-smoke.ts green-order-cardano-agent/e2e/preprod/src/account_datum.ts green-order-cardano-agent/e2e/preprod/tests/account_datum.test.ts
git commit -m "refactor: share aleph account datum parsing"
```

### Task 3: Add Partial Smoke Script

**Files:**
- Create: `green-order-cardano-agent/e2e/preprod/10-preflight-green-order-partial-smoke.ts`
- Optionally create: Rust helper under `green-order-cardano-agent/tests/`
- Create: `green-order-cardano-agent/e2e/preprod/10-submit-green-order-partial-smoke.ts`
- Modify: `green-order-cardano-agent/e2e/preprod/src/state.ts`
- Modify: `green-order-cardano-agent/e2e/preprod/tests/state.test.ts`

**Step 1: Add engine preflight fixture**

Add `10-preflight-green-order-partial-smoke.ts` as the stable runner-facing preflight wrapper. It may call a Rust helper internally if importing the matcher from TypeScript would require duplicating too much logic, but the Deno script must always exist because the runner calls it.

The helper input must include:

- pool ADA reserve
- pool output-token reserve
- `partialLeavingLovelace`
- `partialExpectedTokenAmount`
- `partialFeeLovelace`
- operator key hash

The helper output must be JSON:

```json
{
  "consumedLeaving": "123",
  "receivedOutput": "45",
  "remainingLeaving": "19877",
  "remainingExpectedOutput": "9955",
  "remainingFee": "1988"
}
```

The helper must fail unless it proves strict partial execution.

For `--select-defaults --shell-env`, the helper should search a small bounded table of fixture candidates and print only shell assignments for the first candidate that proves strict partial execution. Example candidate search dimensions:

- `ACCOUNT_INITIAL_LOVELACE`: `50_000_000`, `75_000_000`
- `POOL_INITIAL_LOVELACE`: `40_000_000`, `80_000_000`
- `POOL_INITIAL_TOKEN_AMOUNT`: `100_000`, `1_000_000`, `5_000_000`
- `PARTIAL_LEAVING_LOVELACE`: `5_000_000`, `10_000_000`, `20_000_000`
- `PARTIAL_EXPECTED_TOKEN_AMOUNT`: values at or below the helper-computed first-step `receivedOutput`, so the matcher does not classify the fragment as unsatisfied
- `PARTIAL_FEE_LOVELACE`: `2_000_000`

The committed default values are whatever this helper prints and proves; do not manually document unproven numeric defaults in the runner.

**Step 2: Extend state types**

Add `partialSmoke` to the persisted E2E state:

```ts
partialSmoke?: {
  submittedIntentDigest: string;
  executionTxHash: string;
  oldStoreRootHex?: string;
  newStoreRootHex?: string;
  receivedAmount: string;
  storePath: string;
};
```

Write/extend `tests/state.test.ts` to assert this field round-trips through `saveState` and `loadState`. Extend `validateState` to require `partialSmoke.storePath` to be a non-empty string when `partialSmoke` is present.

**Step 3: Run state test and verify it fails if needed**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno test --no-lock --allow-read --allow-write --allow-env tests/state.test.ts
```

Expected: PASS if the state type is compile-only, or FAIL until the serializer accepts the field.

**Step 4: Create partial smoke script by copying full-fill shape**

Create `10-submit-green-order-partial-smoke.ts` from `04-submit-green-order-smoke.ts` and make these deliberate differences:

- Use `config.partialLeavingLovelace`, `config.partialExpectedTokenAmount`, and `config.partialFeeLovelace`.
- Before submitting, run a deterministic engine preflight against the current pool and intent fixture:
  - Add a Rust helper/test in `green-order-cardano-agent` or `bloom-offchain-cardano` that builds the exact GreenOrder plus Royalty V1 pool state used by this E2E fixture and runs the same maker/taker matching path as the agent.
  - The helper must output JSON with `consumedLeaving`, `receivedOutput`, `remainingLeaving`, `remainingExpectedOutput`, `remainingFee`, and `newStoreRoot`.
  - The helper must fail unless `0 < consumedLeaving < partialLeavingLovelace`, `receivedOutput > 0`, and `remainingLeaving > 0`.
  - Do not use `receivedOutput < partialExpectedTokenAmount` as the trigger for execution. GreenOrder's matcher exposes `expected_arriving_amount` as `min_marginal_output`, so setting expected output above the first-step output can make the fragment unsatisfied instead of partially executable.
  - The TS smoke script should either call this helper or reimplement only the helper-proven arithmetic with the same fixture constants, then compare live results to the helper output.
- Keep `auth.type = "sig"` with empty `updateProof`; the agent must create the MPF insert proof during partial execution.
- Keep `operatorKeyHash` sourced from `loadConfig()`, which already derives the green-order agent batcher from the active agent config.
- Use `config.partialExecutionTimeoutMs` for all `waitFor` calls so Koios/indexer lag does not create false failures.
- Parse old and new Aleph account datum with `parseAccountDatum`.
- Assert current ABI for this test. If `state.account.alephAbi === "legacy"`, fail early with a clear message because the MPF root assertion depends on current ABI field `5`.

**Step 5: Add partial-specific on-chain assertions**

The script must assert:

```ts
if (!oldParsed.storeRootHex || !newParsed.storeRootHex) {
  throw new Error("partial fill requires current Aleph ABI with store root");
}
if (newParsed.storeRootHex === oldParsed.storeRootHex) {
  throw new Error("partial fill did not mutate Aleph account MPF store root");
}
if (newParsed.nonce0 < oldParsed.nonce0 + 1n) {
  throw new Error("partial fill did not advance Sig nonce");
}
const receivedDelta =
  (newAccount.assets[state.pool.assetY] ?? 0n) -
  (oldAccount.assets[state.pool.assetY] ?? 0n);
if (receivedDelta <= 0n) {
  throw new Error("partial fill did not deliver any output asset");
}
if (receivedDelta !== preflight.receivedOutput) {
  throw new Error(`partial fill output ${receivedDelta} does not match engine preflight ${preflight.receivedOutput}`);
}
```

Also keep pool assertions:

- pool lovelace reserve increases
- pool token reserve decreases
- old account output spent
- old pool output spent
- successor account and pool outputs are from the same execution tx

**Step 6: Assert persisted pending MPF leaf**

After the successor account is observed, wait for the agent to persist the account store snapshot at:

```ts
const agentConfig = JSON.parse(await Deno.readTextFile(config.agentConfigPath));
const storePath = `${agentConfig.chainSync.dbPath}.green-account-stores.json`;
```

Then parse the JSON shape already produced by `green-order-cardano-agent/src/account_index.rs`:

```json
{
  "stores": [
    {
      "account_id": [208, 154],
      "output_ref": "txhash#0",
      "root": [0, 1],
      "leaves": []
    }
  ]
}
```

Add helpers in the script to:

- Convert byte arrays to lowercase hex.
- Find the store whose `output_ref` equals the successor account ref.
- Assert `hex(store.root) === newParsed.storeRootHex`.
- Find one pending leaf for the submitted intent key/digest. Inspect actual `StoredIntentLeafSnapshot` JSON field names during implementation and make the helper fail loudly if the snapshot shape is unknown.
- Assert that leaf status is pending, not completed.
- Assert the leaf remaining intent has:
  - `leaving_amount === preflight.remainingLeaving`.
  - `expected_arriving_amount === preflight.remainingExpectedOutput`.
  - `fee_lovelace === preflight.remainingFee`.

For this first partial smoke, derive `consumedLeaving` from the persisted remaining intent:

```ts
const remainingLeaving = BigInt(leaf.intent.leaving_amount);
if (remainingLeaving !== preflight.remainingLeaving) {
  throw new Error(`persisted remaining leaving ${remainingLeaving} does not match preflight ${preflight.remainingLeaving}`);
}
const consumedLeaving = config.partialLeavingLovelace - remainingLeaving;
if (consumedLeaving !== preflight.consumedLeaving) {
  throw new Error(`persisted consumed leaving ${consumedLeaving} does not match preflight ${preflight.consumedLeaving}`);
}
if (consumedLeaving <= 0n || consumedLeaving >= config.partialLeavingLovelace) {
  throw new Error("persisted continuation is not a strict partial fill");
}
if (BigInt(leaf.intent.expected_arriving_amount) !== preflight.remainingExpectedOutput) {
  throw new Error("persisted remaining expected output does not match preflight");
}
if (BigInt(leaf.intent.fee_lovelace) !== preflight.remainingFee) {
  throw new Error("persisted remaining fee does not match preflight");
}
```

Then use `consumedLeaving` to verify the remaining expected output and prorated remaining fee against the engine preflight. This avoids inferring consumed input from the account ADA delta, which includes execution fees and minimum ADA effects.

Do not accept a root-only assertion as success; root mutation alone is not enough.

**Step 7: Persist partial smoke state**

After success, call `saveState` with updated account/pool refs and:

```ts
partialSmoke: {
  submittedIntentDigest: digestHex,
  executionTxHash: newAccount.txHash,
  oldStoreRootHex: oldParsed.storeRootHex,
  newStoreRootHex: newParsed.storeRootHex,
  receivedAmount: receivedDelta.toString(),
  storePath,
}
```

**Step 8: Deno check**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno check --no-lock 10-submit-green-order-partial-smoke.ts
deno test --no-lock --allow-read --allow-write --allow-env tests/state.test.ts tests/account_datum.test.ts tests/config.test.ts
```

Expected: PASS.

If the preflight helper is implemented in Rust, also run:

```bash
cargo test -p green-order-cardano-agent partial_green_order_preflight
```

Expected: PASS and the fixture asserts strict partial execution.

**Step 9: Commit**

```bash
git add green-order-cardano-agent/e2e/preprod/10-submit-green-order-partial-smoke.ts green-order-cardano-agent/e2e/preprod/10-preflight-green-order-partial-smoke.ts green-order-cardano-agent/e2e/preprod/src/state.ts green-order-cardano-agent/e2e/preprod/tests/state.test.ts green-order-cardano-agent/tests
git commit -m "test: add partial fill green order smoke"
```

### Task 4: Add Partial E2E Runner

**Files:**
- Modify: `green-order-cardano-agent/e2e/preprod/07-prepare-operator-funding.ts`
- Test: `green-order-cardano-agent/e2e/preprod/tests/operator_funding_config.test.ts`
- Create: `green-order-cardano-agent/e2e/preprod/run-preprod-partial-e2e.sh`
- Optionally modify: `green-order-cardano-agent/e2e/preprod/run-preprod-e2e.sh`

**Step 1: Stop operator funding from mutating default config in partial runs**

`07-prepare-operator-funding.ts` currently writes operator key, node, network, and funding settings to `../../resources/preprod.config.json`. For partial E2E this is not acceptable because the runner uses a temp config and default preprod config must not be mutated.

Modify the script so:

- `const agentConfigPath = Deno.env.get("AGENT_CONFIG_PATH")?.trim() ?? "../../resources/preprod.config.json";`
- when `PARTIAL_E2E=1`, it refuses to write if `agentConfigPath` is `../../resources/preprod.config.json` or ends with `/resources/preprod.config.json`
- it writes the generated operator key and funding defaults to `agentConfigPath`
- it logs the exact config path updated

Add `tests/operator_funding_config.test.ts` with a small extracted helper from `07-prepare-operator-funding.ts`, for example:

```ts
assertThrows(
  () => assertWritableAgentConfig("../../resources/preprod.config.json", { partialE2e: true }),
  Error,
  "must not update default preprod config during partial E2E",
);
assertEquals(resolveAgentConfigPath("/tmp/partial.json"), "/tmp/partial.json");
```

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno test --no-lock --allow-read --allow-write --allow-env tests/operator_funding_config.test.ts
```

Expected: PASS.

**Step 2: Create dedicated partial runner**

Create `run-preprod-partial-e2e.sh` that follows the setup flow from `run-preprod-e2e.sh`, but ends with `10-submit-green-order-partial-smoke.ts`.

Set deterministic partial env defaults inside the runner unless already provided. Do not hardcode suspect liquidity/intent ratios in the shell script. Instead, call the engine preflight helper in `--select-defaults` mode before setup; the helper must derive defaults that it has proven produce strict partial execution for the current Royalty V1 matcher.

```bash
export FORCE_E2E_STATE="${FORCE_E2E_STATE:-1}"
export PARTIAL_E2E=1
eval "$(
  deno run --no-lock --allow-read --allow-env 10-preflight-green-order-partial-smoke.ts --select-defaults --shell-env
)"
export PARTIAL_EXECUTION_TIMEOUT_MS="${PARTIAL_EXECUTION_TIMEOUT_MS:-900000}"
```

The selected defaults must include `ACCOUNT_INITIAL_LOVELACE`, `POOL_INITIAL_LOVELACE`, `POOL_INITIAL_TOKEN_AMOUNT`, `PARTIAL_LEAVING_LOVELACE`, `PARTIAL_EXPECTED_TOKEN_AMOUNT`, and `PARTIAL_FEE_LOVELACE`. If the caller explicitly sets any of these env vars, the preflight helper must validate the caller-provided complete fixture and fail before setup unless it proves strict partial fill: `0 < consumedLeaving < PARTIAL_LEAVING_LOVELACE` and `receivedOutput > 0`; no manual post-submit tuning is acceptable.

**Step 3: Require partial-enabled agent config**

Do not mutate `green-order-cardano-agent/resources/preprod.config.json`.

The runner should require `AGENT_CONFIG_PATH` to point to a temporary config where:

```json
"greenOrders": {
  "allowPartial": true
}
```

If `AGENT_CONFIG_PATH` is unset, points at `../../resources/preprod.config.json`, or points at JSON where `.greenOrders.allowPartial !== true`, fail before running any Deno setup script:

```bash
if [[ -z "${AGENT_CONFIG_PATH:-}" ]]; then
  echo "Set AGENT_CONFIG_PATH to a temp preprod config with greenOrders.allowPartial=true" >&2
  exit 1
fi
if [[ "${AGENT_CONFIG_PATH}" == "../../resources/preprod.config.json" || "${AGENT_CONFIG_PATH}" == *"/resources/preprod.config.json" ]]; then
  echo "Partial E2E must not use the default preprod config; create a temp partial config first" >&2
  exit 1
fi
if [[ "$(jq -r '.greenOrders.allowPartial // false' "${AGENT_CONFIG_PATH}")" != "true" ]]; then
  echo "${AGENT_CONFIG_PATH} must set greenOrders.allowPartial=true" >&2
  exit 1
fi
```

This avoids accidentally changing production preprod defaults.

**Step 4: Reuse setup logic**

Either:

- keep a small duplicated setup sequence in the new runner, or
- add `E2E_SMOKE_SCRIPT="${E2E_SMOKE_SCRIPT:-04-submit-green-order-smoke.ts}"` to `run-preprod-e2e.sh` and let the partial runner call it with `E2E_SMOKE_SCRIPT=10-submit-green-order-partial-smoke.ts`.

Prefer the second option if the diff stays small.

**Step 5: Shell-check by execution to first Deno command**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
bash -n run-preprod-partial-e2e.sh
```

Expected: no syntax errors.

**Step 6: Commit**

```bash
git add green-order-cardano-agent/e2e/preprod/07-prepare-operator-funding.ts green-order-cardano-agent/e2e/preprod/tests/operator_funding_config.test.ts green-order-cardano-agent/e2e/preprod/run-preprod-partial-e2e.sh green-order-cardano-agent/e2e/preprod/run-preprod-e2e.sh
git commit -m "test: add partial fill preprod runner"
```

### Task 5: Add Temp Config Helper For Live Runs

**Files:**
- Create: `green-order-cardano-agent/e2e/preprod/09-create-partial-agent-config.ts`
- Test: `green-order-cardano-agent/e2e/preprod/tests/partial_agent_config.test.ts`

**Step 1: Write helper behavior test**

Test that the helper:

- reads `../../resources/preprod.config.json`
- writes a temp JSON path passed by `--out`
- sets `greenOrders.allowPartial = true`
- changes chain-sync DB path to a partial-specific absolute path under `green-order-cardano-agent/e2e/preprod/.state/`
- preserves `operatorKey`, `bindings`, validator hashes, and ports

**Step 2: Implement helper**

The helper should support:

```bash
deno run --no-lock --allow-read --allow-write --allow-env 09-create-partial-agent-config.ts \
  --out /tmp/green-order-preprod-partial.config.json
```

Implementation details:

- Parse the original JSON.
- Set `greenOrders.allowPartial = true`.
- Derive the partial DB path from the original `chainSync.dbPath` directory and write an absolute path into the temp config. For the current preprod config, the output must be `/Users/aleksandr/IdeaProjects/green-order-offchain-agent/green-order-cardano-agent/e2e/preprod/.state/agent-chain-sync-partial-fill` unless `PARTIAL_AGENT_DB_PATH` is provided.
- If `PARTIAL_AGENT_DB_PATH` is provided, normalize it to an absolute path before writing it to the temp config.
- Ensure the account store path implied by the config will be stable and predictable:

```ts
const accountStorePath = `${config.chainSync.dbPath}.green-account-stores.json`;
```

- If the existing config uses `chainSync.startingPoint` and `replayFromPoint`, preserve them. Do not fetch Koios inside this helper unless the current config already has a known helper pattern for near-tip configs.
- Print the output path and whether partial is enabled.

**Step 3: Verify helper**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno test --no-lock --allow-read --allow-write --allow-env tests/partial_agent_config.test.ts
deno run --no-lock --allow-read --allow-write --allow-env 09-create-partial-agent-config.ts --out /tmp/green-order-preprod-partial.config.json
```

Expected:

- test PASS
- temp config JSON exists
- `jq '.greenOrders.allowPartial' /tmp/green-order-preprod-partial.config.json` prints `true`
- `jq -r '.chainSync.dbPath' /tmp/green-order-preprod-partial.config.json` prints an absolute path under `green-order-cardano-agent/e2e/preprod/.state/`

**Step 4: Commit**

```bash
git add green-order-cardano-agent/e2e/preprod/09-create-partial-agent-config.ts green-order-cardano-agent/e2e/preprod/tests/partial_agent_config.test.ts
git commit -m "test: add partial fill agent config helper"
```

### Task 6: Run Local Review And Static Verification

**Files:**
- No code changes expected unless review finds an issue.

**Step 1: Run Deno checks**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno check --no-lock \
  04-submit-green-order-smoke.ts \
  09-create-partial-agent-config.ts \
  10-submit-green-order-partial-smoke.ts
```

Expected: PASS.

**Step 2: Run focused tests**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno test --no-lock --allow-read --allow-write --allow-env \
  tests/config.test.ts \
  tests/account_datum.test.ts \
  tests/state.test.ts \
  tests/operator_funding_config.test.ts \
  tests/partial_agent_config.test.ts
```

Expected: PASS.

**Step 3: Run reviewer sub-agent**

Ask a reviewer sub-agent to inspect only:

- `green-order-cardano-agent/e2e/preprod/src/config.ts`
- `green-order-cardano-agent/e2e/preprod/src/account_datum.ts`
- `green-order-cardano-agent/e2e/preprod/src/state.ts`
- `green-order-cardano-agent/e2e/preprod/09-create-partial-agent-config.ts`
- `green-order-cardano-agent/e2e/preprod/10-submit-green-order-partial-smoke.ts`
- `green-order-cardano-agent/e2e/preprod/run-preprod-partial-e2e.sh`

Reviewer acceptance criteria:

- No production config mutation.
- Operator key hash comes from green-order agent batcher config.
- Partial script proves store-root mutation.
- Partial script proves a persisted pending leaf exists for the successor account root.
- Partial script proves received output is nonzero and matches the engine preflight.
- Partial runner cannot accidentally run with `allowPartial = false`.
- Existing full-fill smoke still checks.

**Step 4: Fix review findings and rerun checks**

Apply only concrete findings. Rerun Step 1 and Step 2 after each fix.

**Step 5: Commit review fixes**

```bash
git add green-order-cardano-agent/e2e/preprod
git commit -m "fix: address partial fill smoke review"
```

Skip this commit if there are no review changes.

### Task 7: Run Live Preprod Partial E2E

**Files:**
- Runtime-generated temp files only.

**Step 1: Create partial config**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno run --no-lock --allow-read --allow-write --allow-env 09-create-partial-agent-config.ts \
  --out /tmp/green-order-preprod-partial.config.json
```

Expected: helper reports `greenOrders.allowPartial=true`.

**Step 2: Restart green-order agent with partial config**

Run the agent using the same command used for the successful full-fill smoke, but with:

```bash
AGENT_CONFIG_PATH=/tmp/green-order-preprod-partial.config.json
```

Expected:

- health endpoint responds
- logs show partial green-order execution is enabled
- no Aleph contract redeploy occurs

**Step 3: Run partial E2E runner**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
AGENT_CONFIG_PATH=/tmp/green-order-preprod-partial.config.json \
FORCE_E2E_STATE=1 \
bash run-preprod-partial-e2e.sh
```

Expected:

- setup creates fresh operator funding, entitlements, bound Aleph account, Royalty V1 pool, and separator funding
- `10-submit-green-order-partial-smoke.ts` submits the Sig intent
- execution tx spends the old account and pool
- successor account and pool appear in the same tx
- script prints partial smoke success

**Step 4: Confirm on-chain internals**

From the saved state, verify:

- `partialSmoke.executionTxHash` is non-empty
- `partialSmoke.oldStoreRootHex !== partialSmoke.newStoreRootHex`
- `partialSmoke.receivedAmount` is greater than `0`
- `partialSmoke.receivedAmount` equals the engine preflight `receivedOutput`
- the persisted account store file at `<chainSync.dbPath>.green-account-stores.json` has a store entry for the successor account output
- that store entry root equals `partialSmoke.newStoreRootHex`
- that store entry has a pending continuation leaf whose remaining amounts match `remaining_after_fill`
- current account output ref points to the successor account output from the execution tx
- current pool output ref points to the successor pool output from the execution tx

**Step 5: Commit final verified E2E changes**

```bash
git status --short
git add green-order-cardano-agent/e2e/preprod docs/plans/2026-05-21-preprod-partial-fill-intent.md
git commit -m "test: cover partial green order execution on preprod"
```

### Rollback Notes

- Do not revert or mutate Aleph contract source for this work.
- Delete `/tmp/green-order-preprod-partial.config.json` if the temp config is no longer needed.
- Restore the normal agent config by restarting the green-order agent with `green-order-cardano-agent/resources/preprod.config.json`.
- Existing full-fill smoke remains available through `run-preprod-e2e.sh`.

### Success Criteria

- Static Deno checks pass.
- Focused Deno tests pass.
- Reviewer sub-agent returns no blocking findings.
- Live preprod partial smoke submits a transaction.
- The transaction partially fills the intent.
- The successor Aleph account datum has a different current ABI store root.
- The account receives nonzero output equal to the engine preflight `receivedOutput`.
- The persisted continuation proves `0 < consumedLeaving < partialLeavingLovelace`.
- No Aleph validator source is changed.
