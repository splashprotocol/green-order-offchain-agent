# Green Fee Estimation Overflow Fix Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix green-order transaction fee estimation so preprod execution can build script-aware fees without CML dummy ex-unit overflow.

**Architecture:** The fix belongs in the generic execution projection path, not in the preprod scripts. Redeemer witness indexes must be derived from CML's canonical redeemer indexing rules: spend redeemers are indexed by the lexicographically sorted set of all transaction inputs, and reward redeemers are indexed by the lexicographically sorted withdrawal map keys.

**Tech Stack:** Rust, CML transaction builder, `bloom-offchain-cardano`, `green-order-cardano-agent`, Deno preprod E2E scripts.

---

## Current Evidence

The restarted preprod flow now reaches transaction construction:

- The 10M/10M pool is observed from ledger.
- The green intent is observed from mempool.
- LiquidityBook forms a valid `GreenOrder -> RoyaltyV1PoolOnly` batch.
- `TxBlueprint` projects two script inputs: Aleph account and Royalty V1 pool.
- Reference scripts are small and valid:
  - Aleph account ref script: `120` raw Plutus bytes.
  - Royalty pool ref script: `3529` raw Plutus bytes.
- `min_fee(false)` succeeds with `217245`.
- `min_fee(true)` fails with `Arithmetic(IntegerOverflow)`.

The likely cause is not funding. The additional 2k tADA helps later submission/change selection, but this failure happens before balancing and before submission.

## Root Cause Hypothesis

`TxBlueprint::project_onto_builder` builds `all_io` from script inputs plus funding/operator outputs.

The loop uses the custom `all_io` enumeration index as the spend redeemer index:

```rust
for (ix, io) in enumerated_io {
    ...
    txb.set_exunits(
        RedeemerWitnessKey::new(RedeemerTag::Spend, ix as u64),
        script.cost.compute(&ctx).into(),
    );
}
```

That is not CML's indexing rule. CML's `RedeemerSetBuilder` stores spends in a `BTreeMap<TransactionInput, ...>` and documents that spend redeemer indexes are computed over the lexicographically sorted set of all transaction inputs, not the order chosen by this off-chain projection loop.

The current `all_io` sort is application-specific:

- account script inputs are forced before non-account script inputs;
- funding inputs, when present, are placed after script inputs;
- output-only operator-fee outputs are also present in `all_io` even though they are not transaction inputs.

This custom ordering can diverge from CML's canonical input ordering. In the observed green smoke transaction, the account input is `e187...#0` and the pool input is `84fb...#0`. CML sorts the pool input before the account input lexicographically, while the custom account-priority ordering places the account first. That can write ex-units to the wrong CML spend redeemer entries. If any real redeemer entry is left with CML's dummy ex-units, `min_fee(true)` sums dummy `u64::MAX` values and returns `Arithmetic(IntegerOverflow)`.

The plan must verify the exact remaining dummy redeemer source before implementing the fix. Candidate sources are:

- spend indexes computed from `enumerated_io` instead of CML input ordering;
- reward indexes computed from local `witness_scripts` sort order instead of CML withdrawal-key ordering;
- a missing `set_exunits` call for one of the generated redeemer entries.

The previously suspected no-input operator-fee output is not itself the direct shift: with the current comparator it sorts after script inputs. It remains relevant because high green fees trigger the operator-output branch and make this execution shape easier to reproduce, but the durable fix must be canonical redeemer indexing.

## Why Existing Bloom Flow Usually Does Not Hit It

There are two relevant execution paths:

1. Classical `RunOrder` path in `spectrum-offchain-cardano/src/data/pool.rs`
   - Builds pool/order transactions directly.
   - Sets spend ex-units from explicit pool/order input indexes.
   - Has a known strict-fee workaround for some CML fee bugs.
   - It does not use the generic `all_io` enumeration for spend redeemer indexes.

2. Generic TLB recipe path in `bloom-offchain-cardano/src/execution_engine`
   - Used by the green flow.
   - Can also be used by limit/adhoc/grid paths, but not every recipe creates the same combination of account-prioritized script inputs, pool inputs, reward withdrawals, and operator-fee accounting.
   - Existing Bloom runs may avoid the failure when their custom ordering happens to match CML lexicographic ordering, when they execute through the older direct `RunOrder` path, or when they do not produce a dummy-exunit redeemer in the built transaction.

So this is not a Cardano funding shortage. It is an index-accounting bug exposed by green orders because the green transaction combines Aleph account input ordering, Royalty pool input ordering, and an Aleph batch reward witness in the generic recipe interpreter.

---

## Task 1: Add a Focused Failing Unit Test

**Files:**
- Modify: `bloom-offchain-cardano/src/execution_engine/execution_state.rs`

**Step 1: Write the failing test**

Add a unit test under the existing `#[cfg(test)]` area, or create one if this file has no local test module.

The test should build a minimal `TxBlueprint` with:

- two script inputs;
- script input references deliberately ordered so custom account-priority ordering differs from CML lexicographic `TransactionInput` ordering;
- optionally one non-script funding input whose `TransactionInput` sorts before or between script inputs;
- optionally `operator_interest = MIN_SAFE_LOVELACE_VALUE` to include the green-like operator-fee output shape;
- deterministic script costs;
- projection through `project_onto_builder`.

Then assert that all real spend redeemers have non-dummy ex-units and that the ex-units are attached to the expected CML indexes.

If CML exposes redeemers through `build_for_evaluation(...).build()`, use that and assert:

```rust
assert_ne!(redeemers[0].ex_units, ExUnits::dummy());
assert_ne!(redeemers[1].ex_units, ExUnits::dummy());
```

If direct redeemer inspection is awkward, call `tx_builder.min_fee(true)` and assert it does not return `Arithmetic(IntegerOverflow)`, but prefer explicit redeemer inspection because it proves the index mapping instead of only proving the symptom disappeared.

Add a second focused test for reward withdrawals if the diagnostic proves the dummy entry is reward-related:

- create two witness scripts whose local `witness_scripts` sort order differs from CML `RewardAddress` ordering;
- project the blueprint;
- assert reward redeemer ex-units are attached to the CML reward indexes.

**Step 2: Run the test and verify it fails**

Run:

```bash
cargo test -p bloom-offchain-cardano canonical_cml_spend_indexes_receive_exunits
```

Expected before the fix:

- at least one CML spend or reward redeemer keeps dummy ex-units;
- or `min_fee(true)` returns `Arithmetic(IntegerOverflow)`.

---

## Task 2: Fix Canonical Redeemer Index Assignment

**Files:**
- Modify: `bloom-offchain-cardano/src/execution_engine/execution_state.rs`

**Step 1: Build canonical CML spend indexes**

In `TxBlueprint::project_onto_builder`, stop using the custom `enumerated_io` index directly as a `RedeemerTag::Spend` index.

Before adding inputs, build a canonical map from each real input `OutputRef` to its CML spend redeemer index:

```rust
let mut cml_inputs = all_io
    .iter()
    .filter_map(|io| match io {
        Either::Left((input, _)) => Some(input.reference),
        Either::Right((Some(input), _)) => Some(input.reference()),
        Either::Right((None, _)) => None,
    })
    .collect::<Vec<_>>();
cml_inputs.sort();
let cml_spend_indexes = cml_inputs
    .into_iter()
    .enumerate()
    .map(|(ix, reference)| (reference, ix as u64))
    .collect::<HashMap<_, _>>();
```

Do not call `set_exunits` immediately while iterating custom-ordered inputs. CML's
`RedeemerSetBuilder` indexes into redeemer entries that already exist in its
internal `BTreeMap`; if the account is visited first but its canonical CML index
is `1`, setting index `1` before the pool redeemer exists can panic. Instead,
store the computed ex-units while projecting inputs and apply them after every
real input has been added to the builder.

Collect pending spend ex-units like this:

```rust
txb.add_input(input).expect("add script input ok");
let cml_spend_ix = *cml_spend_indexes
    .get(&reference)
    .expect("script input must have CML spend index");
pending_spend_exunits.push((
    RedeemerWitnessKey::new(RedeemerTag::Spend, cml_spend_ix),
    script.cost.compute(&ctx).into(),
));
```

After the full input loop finishes and all real inputs have been inserted, apply
the pending assignments:

```rust
for (key, ex_units) in pending_spend_exunits {
    txb.set_exunits(key, ex_units);
}
```

**Step 2: Use the same canonical map for delayed redeemers**

Current `TxInputsOrdering` is built from `enumerated_io`, which mirrors the custom off-chain ordering, not CML input ordering. Replace it with the same canonical input map:

```rust
let inputs_ordering = TxInputsOrdering::new(HashMap::from_iter(
    cml_spend_indexes
        .iter()
        .map(|(reference, ix)| (*reference, *ix as usize)),
));
```

This matters because delayed redeemers such as Royalty pool redeemers compute pool input indexes from `TxInputsOrdering`.

**Step 3: Build canonical CML reward indexes**

The same class of bug can exist for reward withdrawals. CML indexes reward redeemers by lexicographic `RewardAddress` order, not by local `witness_scripts` vector order. Before projecting witness scripts, derive each witness reward address and sort by `RewardAddress`.

Use the canonical reward index when assigning ex-units, but account for the same
CML timing rule as spend inputs. Either add withdrawal entries in canonical
`RewardAddress` order, or add all withdrawals first and only then apply reward
ex-units. Prefer the latter because it keeps the current witness-script
projection structure intact.

Collect pending reward ex-units while adding withdrawals, then apply them after
all withdrawals have been inserted:

```rust
pending_reward_exunits.push((
    RedeemerWitnessKey::new(RedeemerTag::Reward, cml_reward_ix),
    ex_units.into(),
));

...

for (key, ex_units) in pending_reward_exunits {
    txb.set_exunits(key, ex_units);
}
```

For one Aleph batch witness this will still be index `0`, but the generic code should be correct for multiple witnesses too.

**Step 4: Preserve script context semantics**

Do not blindly replace `ScriptContextPreview { self_index: ix }` unless the scripts use it as the real input index.

For script cost scaling, decide explicitly:

- If `self_index` is meant to be CML transaction input index, use `cml_spend_ix`.
- If it is meant to be custom execution sequence index, keep the current custom `ix`.

Current account and pool validators use this mostly for marginal-cost scaling. Since the cost scaling should match the validator's real input index in the transaction, prefer `cml_spend_ix` after verifying tests.

```rust
let ctx = ScriptContextPreview {
    self_index: cml_spend_ix as usize,
};
```

**Step 5: Remove temporary diagnostic logs or lower them to trace**

Keep useful trace logs if they help future debugging, but remove panic-oriented diagnostics from `interpreter.rs` if they were only added for this investigation.

---

## Task 3: Add a Regression Test for Fee Estimation

**Files:**
- Modify: `bloom-offchain-cardano/src/execution_engine/interpreter.rs`
- Or modify existing tests near `fee_without_residue` / `fee_with_residue`

**Step 1: Add a green-like recipe test**

Create a test that forces:

- a taker fee/operator interest of at least `MIN_SAFE_LOVELACE_VALUE`;
- two script inputs;
- one no-input operator output.

Assert:

```rust
let fee = tx_builder.min_fee(true);
assert!(fee.is_ok(), "{fee:?}");
```

**Step 2: Run targeted tests**

Run:

```bash
cargo test -p bloom-offchain-cardano execution_state
cargo test -p bloom-offchain-cardano interpreter
```

Expected: all pass.

---

## Task 4: Restore Error Handling Around Fee Estimation

**Files:**
- Modify: `bloom-offchain-cardano/src/execution_engine/interpreter.rs`

**Step 1: Replace panic-on-fee-correction where practical**

Current `run()` panics on `execute_recipe(...)` errors:

```rust
.unwrap_or_else(|err| panic!("fee correction failed: {}", err));
```

This makes runtime debugging expensive. If the surrounding `ExecutionResult` type can represent a failed recipe, return that instead. If it cannot, keep the panic but ensure fee-estimation errors include enough context:

- `min_fee(false)` result;
- number of spend inputs;
- number of reward withdrawals;
- witness script hashes;
- whether any dummy ex-units remain, if inspectable.

**Step 2: Run compile check**

Run:

```bash
cargo check -p bloom-offchain-cardano -p green-order-cardano-agent
```

Expected: pass.

---

## Task 5: Re-run Preprod Smoke Flow

**Files:**
- Existing scripts under `green-order-cardano-agent/e2e/preprod`

**Step 1: Start agent**

Run:

```bash
cargo run -p green-order-cardano-agent -- \
  --config-path green-order-cardano-agent/resources/preprod.config.json \
  --deployment-path green-order-cardano-agent/resources/preprod.deployment.json \
  --validation-rules-path green-order-cardano-agent/resources/validation-rules.json.template \
  --log4rs-path green-order-cardano-agent/resources/log4rs.local.yaml
```

Expected:

- health API starts;
- new pool is observed;
- no startup panic.

**Step 2: Submit smoke intent**

Run:

```bash
deno run --no-lock --allow-net --allow-read --allow-write --allow-env \
  green-order-cardano-agent/e2e/preprod/04-submit-green-order-smoke.ts
```

Expected:

- intent appears in mempool;
- match is formed;
- `min_fee(true)` succeeds;
- transaction is built and submitted.

**Step 3: Verify on-chain result**

Check:

- Aleph account UTxO is spent and recreated;
- pool UTxO is spent and recreated;
- account output receives the expected green token amount;
- account datum/root updates consistently;
- smoke script reports success.

---

## Task 6: Final Cleanup

**Files:**
- Modify: `bloom-offchain-cardano/src/execution_engine/interpreter.rs`
- Modify: `bloom-offchain-cardano/src/execution_engine/execution_state.rs`
- Modify only if needed: `green-order-cardano-agent/resources/log4rs.local.yaml`

**Step 1: Remove temporary debug-only changes**

Remove logs added only to isolate this failure, especially if they expose large payloads or noisy trace output.

**Step 2: Restore normal local logging**

If `log4rs.local.yaml` was changed to very verbose trace mode, return it to a sane local default after preprod validation.

**Step 3: Run final verification**

Run:

```bash
cargo check -p bloom-offchain-cardano -p green-order-cardano-agent
cargo test -p bloom-offchain-cardano canonical_cml_spend_indexes_receive_exunits
```

Expected: pass.

**Step 4: Commit**

Run:

```bash
git add bloom-offchain-cardano/src/execution_engine/execution_state.rs \
        bloom-offchain-cardano/src/execution_engine/interpreter.rs \
        docs/plans/2026-05-20-green-fee-estimation-overflow.md
git commit -m "fix: keep spend redeemer indexes stable with operator outputs"
```
