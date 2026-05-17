# Aleph Green Order Execution Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the current `GreenOrder` panic bridge with real Aleph account transaction execution for full-fill green orders against Royalty V1 pools.

**Architecture:** Treat the Aleph account UTxO as the green taker bearer: each executed `GreenOrder` spends one account script input with `AccountAction::Delegate(delegate_index)`, creates one updated account output, and contributes one `AuthorizedIntention` to a shared Aleph batch-witness withdrawal redeemer. Preserve the existing Royalty V1 pool execution path, but extend the transaction blueprint so Aleph account continuation outputs are ordered exactly as the witness validator expects. Phase 1 supports full-fill `Auth::Sig` orders only; `Auth::Path` and partial MPF continuations stay rejected until separate proof support is implemented.

**Tech Stack:** Rust workspace, CML transaction builder, `bloom-offchain` execution recipes, `bloom-offchain-cardano` execution state, `spectrum-offchain-cardano` deployment helpers, Aleph Aiken validators from `/Users/aleksandr/RustroverProjects/aleph`.

---

## Source Facts From Aleph

Use these source files as the behavioral contract:

- `/Users/aleksandr/RustroverProjects/aleph/validators/account.ak`
- `/Users/aleksandr/RustroverProjects/aleph/validators/witness.ak`
- `/Users/aleksandr/RustroverProjects/aleph/validators/witness.test.ak`
- `/Users/aleksandr/RustroverProjects/aleph/lib/types.ak`

Important validator rules:

- Account datum is `AccountState { magic, allowlist, nonce, hot_cred, cold_cred, store }`.
- Account spend redeemer is `Delegate(Int)` or `Direct`.
- `Delegate(index)` only checks that transaction withdrawals contain `Script(allowlist[index])`.
- Batch witness must be invoked as a zero-amount withdrawal from the witness reward credential.
- Batch witness treats `builtin.head_list(transaction.extra_signatories)` as the authorized operator. The transaction must include the operator key hash as the first extra signer, and every executed intent must authorize that same operator.
- Batch witness redeemer is `BatchRedeemer { intentions: Data<List<AuthorizedIntention>> }`.
- `AuthorizedIntention` is `{ intent, remainder, auth }`.
- `Intention` is `{ target_nonce, leaving_asset, leaving_amount, arriving_asset, expected_arriving_amount, fee_lovelace, operator }`.
- The witness walks `transaction.inputs` and `transaction.outputs` together, but the cursor behavior is subtle:
  - account inline datum: consumes the current output and validates it as that account's next state,
  - non-account inline datum: consumes the current output without validating it,
  - non-inline datum: returns `True` immediately and stops validating the rest of the transaction.
- Therefore every Aleph account input must appear before every Royalty pool input and before any funding/payment-key input in final transaction input order. Account continuation outputs must appear first in exactly the same order as those account inputs.
- For `Auth::Sig`:
  - The signed message is `blake2b_256(prefix <> cbor(intent) <> postfix)`.
  - `nonce[target_nonce.index]` must move from `<= target_nonce.value` to exactly `target_nonce.value`.
  - If `remainder == 0`, the store does not need an MPF update.
  - If `remainder > 0`, the store must become `mpf.insert(old_store, cbor(target_nonce), digest(updated_intent), update_proof)`.
- For `Auth::Path`:
  - Full fill only checks `mpf.has(store, cbor(target_nonce), digest(intent), proof)`.
  - Partial fill checks `mpf.update(...)`.
  - Phase 1 must not enable this path because the current offchain agent has no MPF proof construction/indexing.

## Non-Negotiable Constraints

- Do not implement green-order-to-green-order execution.
- Do not enable partial fills in Phase 1.
- Do not silently use `Auth::Path` in Phase 1.
- Phase 1 supports ADA-leaving orders only. Reject non-ADA leaving assets until the current Aleph validator arithmetic for non-ADA leaving balances is fixed or separately proven with golden validator tests.
- Do not let the old generic `TxBlueprint` output ordering break Aleph witness input/output pairing.
- Ensure Aleph batch witness withdrawal ordering is deterministic; never derive `RedeemerTag::Reward` indexes from `HashMap` iteration.
- Keep Aleph validator IDs in a shared crate reachable by `bloom-offchain-cardano`; do not put constants needed by `instances.rs` in the binary crate.
- Keep the existing broad Spectrum deployment parsing until the shared event handler no longer requires `ProtocolScriptHashes`; do not use dummy script hashes.
- Keep old limit/Snek/Bloom agent code removed from the new executable path.

## Task 1: Add Shared Aleph Validator IDs And Deployment Fields

**Files:**
- Modify: `bloom-offchain-cardano/src/orders/green.rs`
- Create: `green-order-cardano-agent/src/deployment.rs`
- Modify: `green-order-cardano-agent/src/main.rs`
- Modify: `green-order-cardano-agent/src/context.rs`
- Modify: `green-order-cardano-agent/resources/preprod.deployment.json`
- Modify: `green-order-cardano-agent/resources/mainnet.deployment.json`

**Step 1: Add failing deployment parse test**

Create unit tests in `green-order-cardano-agent/src/deployment.rs`:

```rust
#[test]
fn parses_green_deployment_with_aleph_validators() {
    let raw = include_str!("../resources/preprod.deployment.json");
    let deployment: GreenDeployedValidators = serde_json::from_str(raw).unwrap();
    assert_eq!(
        deployment.aleph_account.hash.to_string().len(),
        56
    );
    assert_eq!(
        deployment.aleph_batch_witness.hash.to_string().len(),
        56
    );
}
```

Run:

```bash
cargo test -p green-order-cardano-agent deployment::tests::parses_green_deployment_with_aleph_validators -- --nocapture
```

Expected: fail because `deployment` module/types do not exist yet.

**Step 2: Define shared Aleph validator IDs**

In `bloom-offchain-cardano/src/orders/green.rs`, add validator type IDs because `bloom-offchain-cardano/src/execution_engine/instances.rs` must use these constants without depending on the binary crate:

```rust
pub const ALEPH_ACCOUNT_VALIDATOR: u8 = 200;
pub const ALEPH_BATCH_WITNESS_VALIDATOR: u8 = 201;
```

All `Has<DeployedValidator<{ ... }>>` bounds in library code must import these constants from `crate::orders::green`.

**Step 3: Define green deployment structs**

In `green-order-cardano-agent/src/deployment.rs`:

```rust
use cardano_explorer::CardanoNetwork;
use bloom_offchain_cardano::orders::green::{
    ALEPH_ACCOUNT_VALIDATOR, ALEPH_BATCH_WITNESS_VALIDATOR,
};
use spectrum_offchain_cardano::deployment::{
    DeployedScriptInfo, DeployedValidator, DeployedValidatorRef, DeployedValidators, ProtocolDeployment,
};

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GreenDeployedValidators {
    #[serde(flatten)]
    pub spectrum: DeployedValidators,
    pub aleph_account: DeployedValidatorRef,
    pub aleph_batch_witness: DeployedValidatorRef,
}

#[derive(Clone)]
pub struct GreenProtocolDeployment {
    pub spectrum: ProtocolDeployment,
    pub aleph_account: DeployedValidator<{ ALEPH_ACCOUNT_VALIDATOR }>,
    pub aleph_batch_witness: DeployedValidator<{ ALEPH_BATCH_WITNESS_VALIDATOR }>,
}

#[derive(Copy, Clone)]
pub struct GreenScriptHashes {
    pub spectrum: spectrum_offchain_cardano::deployment::ProtocolScriptHashes,
    pub aleph_account: DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>,
    pub aleph_batch_witness: DeployedScriptInfo<{ ALEPH_BATCH_WITNESS_VALIDATOR }>,
}
```

Implement `GreenProtocolDeployment::unsafe_pull(...)` by:

1. calling `ProtocolDeployment::unsafe_pull(deployment.spectrum, explorer).await`,
2. pulling `aleph_account`,
3. pulling `aleph_batch_witness`.

This deliberately keeps the full Spectrum deployment object for `HandlerContextProto` and `ProtocolScriptHashes` compatibility. Prune it only after a later task replaces the shared event handler context.

**Step 4: Wire green deployment into `main.rs`**

Replace the runtime deployment parse in `green-order-cardano-agent/src/main.rs`:

```rust
let deployment: GreenDeployedValidators =
    serde_json::from_str(&raw_deployment).expect("Invalid green deployment file");
let green_deployment = GreenProtocolDeployment::unsafe_pull(deployment, &explorer).await;
let protocol_deployment = green_deployment.spectrum.clone();
```

Continue using `ProtocolScriptHashes::from(&protocol_deployment)` for `HandlerContextProto`. Do not synthesize dummy hashes.

**Step 5: Add `Has` impls in `context.rs`**

Update `ExecutionContext`:

```rust
pub deployment: GreenProtocolDeployment,
```

Add:

```rust
impl Has<DeployedValidator<{ ALEPH_ACCOUNT_VALIDATOR }>> for ExecutionContext { ... }
impl Has<DeployedValidator<{ ALEPH_BATCH_WITNESS_VALIDATOR }>> for ExecutionContext { ... }
impl Has<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>> for ExecutionContext { ... }
impl Has<DeployedScriptInfo<{ ALEPH_BATCH_WITNESS_VALIDATOR }>> for ExecutionContext { ... }
impl Has<DeployedValidator<{ RoyaltyPoolV1 as u8 }>> for ExecutionContext { ... }
impl Has<DeployedValidator<{ RoyaltyPoolV1LedgerFixed as u8 }>> for ExecutionContext { ... }
```

Royalty impls should delegate through `self.deployment.spectrum.royalty_pool` and `self.deployment.spectrum.royalty_pool_ledger_fixed`.
Aleph `DeployedScriptInfo` impls should be `DeployedScriptInfo::from(&self.deployment.aleph_account)` and `DeployedScriptInfo::from(&self.deployment.aleph_batch_witness)`.

Keep old `Has<DeployedValidator<...>>` impls only if the compiler still needs them through shared compatibility code. Remove them after the green-specific event handler task proves they are unused.

**Step 6: Update deployment JSON resources**

Keep existing deployment JSON fields required by `DeployedValidators`, and add:

```json
{
  "...": "existing Spectrum deployment fields remain for now",
  "alephAccount": {
    "hash": "df4b5d2d8ba26d2b6bd2a442f868b57109e149d7f7594ba650b4057f",
    "referenceUtxo": { "txHash": "0000000000000000000000000000000000000000000000000000000000000000", "outputIndex": 0 },
    "cost": { "mem": 0, "steps": 0 },
    "marginalCost": { "mem": 0, "steps": 0 }
  },
  "alephBatchWitness": {
    "hash": "6f9aa8f0dc33673884a82c529322fe8caa44e13dd4d3ca1fd4bf6e2f",
    "referenceUtxo": { "txHash": "0000000000000000000000000000000000000000000000000000000000000001", "outputIndex": 1 },
    "cost": { "mem": 0, "steps": 0 },
    "marginalCost": { "mem": 0, "steps": 0 }
  }
}
```

Use real reference UTxOs before running the agent against a network. If runtime resource files should remain deployable, put dummy refs only in separate test fixtures such as `green-order-cardano-agent/test_resources/deployment.with-aleph.json`; do not commit literal placeholder strings that fail serde parsing.

**Step 7: Verify**

Run:

```bash
cargo test -p green-order-cardano-agent deployment -- --nocapture
cargo check -p green-order-cardano-agent
```

Expected: pass.

## Task 2: Model Aleph Account Datum And Redeemers In Rust

**Files:**
- Modify: `bloom-offchain-cardano/src/orders/green.rs`
- Test: `bloom-offchain-cardano/src/orders/green.rs`

**Step 1: Add failing datum/redeemer roundtrip tests**

Add tests for:

- `AccountAction::Delegate(0)` encodes as Aiken constructor alternative `0` with one integer field.
- `AccountAction::Direct` encodes as alternative `1`.
- `AccountState` parses the six-field Aiken datum.
- `AccountState::with_sig_full_fill_nonce(...)` preserves `magic`, `allowlist`, `hot_cred`, `cold_cred`, `store`, and updates only the target nonce slot.
- `Intention` CBOR digest is stable.
- `BatchRedeemer { intentions: [...] }` encodes as constructor alternative `0` with one list field.

Run:

```bash
cargo test -p bloom-offchain-cardano green::tests::aleph_account_action_delegate_encoding -- --nocapture
```

Expected: fail because types/encoders do not exist.

**Step 2: Add domain structs**

In `green.rs`, add:

```rust
pub struct AlephAccountState {
    pub magic: Vec<u8>,
    pub allowlist: Vec<[u8; 28]>,
    pub nonce: Vec<i64>,
    pub main_key: Vec<u8>,
    pub co_key: Option<Vec<u8>>,
    pub cold_key_hash: [u8; 28],
    pub store_root: [u8; 32],
}

pub enum AlephAccountAction {
    Delegate(u64),
    Direct,
}

pub struct AlephIntention { ... }          // exact fields from witness.ak
pub struct AlephAuthorizedIntention { ... }
pub struct AlephBatchRedeemer { ... }
```

Change `GreenAuth` to mirror `/Users/aleksandr/RustroverProjects/aleph/lib/types.ak`; do not store public keys in `GreenAuth` because keys live in `AccountState.hot_cred`:

```rust
pub enum GreenAuth {
    Sig {
        prefix: Vec<u8>,
        postfix: Vec<u8>,
        signature: Vec<u8>,
        update_proof: Vec<u8>,
    },
    Path {
        proof: Vec<u8>,
    },
}
```

Phase 1 constructors must require `signature.len() == 64` and `update_proof.is_empty()`.

Keep `GreenOrder` as the execution taker, but replace ad hoc fields with these Aleph-native fields where possible:

```rust
pub struct GreenOrder {
    pub id: GreenOrderId,
    pub account_id: AccountId,
    pub account_state: AlephAccountState,
    pub delegate_index: u64,
    pub intention: AlephIntention,
    pub accumulated_output: u64,
    pub current_remainder: u64,
    pub auth: GreenAuth,
}
```

**Step 3: Implement Plutus encoders**

Use `spectrum_cardano_lib::plutus_data::make_constr_pd_indefinite_arr` for Aiken-compatible constructors.

Mappings:

- `AccountState`: constructor `0`, fields `[magic, allowlist, nonce, (main_key, co_key_bytes), cold_cred, store]`.
- `AccountAction::Delegate(index)`: constructor `0`, fields `[index]`.
- `AccountAction::Direct`: constructor `1`, fields `[]`.
- `Intention`: constructor `0`, fields `[target_nonce, leaving_asset, leaving_amount, arriving_asset, expected_arriving_amount, fee_lovelace, operator]`.
- `AuthorizedIntention`: constructor `0`, fields `[intent, remainder, auth]`.
- `BatchRedeemer`: constructor `0`, fields `[intentions_list]`.
- `Auth::Path`: constructor `0`, fields `[proof]`.
- `Auth::Sig`: constructor `1`, fields `[prefix, postfix, signature, update_proof]`.

For tuple-like Aiken fields, encode as constructor `0` with the tuple elements unless generated CBOR tests prove Aiken uses a different tuple encoding in this CML/Aiken combination.

**Step 4: Implement Plutus parsers**

Implement `TryFromPData for AlephAccountState`.

Reject:

- non-32-byte store roots,
- non-33-byte main secp256k1 key,
- co-key that is neither empty nor 33 bytes,
- non-28-byte cold key hash,
- non-28-byte allowlist entries.

**Step 5: Verify with golden vectors**

Generate or copy CBOR from Aleph tests for:

- a datum like `AccountState { nonce: [0,0,0,0,0], store: null_hash, ... }`,
- `Delegate(0)`,
- one `BatchRedeemer` from `exec_intent_atomic_success`.

Run:

```bash
cargo test -p bloom-offchain-cardano green -- --nocapture
```

Expected: pass.

## Task 3: Track Aleph Account UTxOs With Stable External Account IDs

**Files:**
- Modify: `bloom-offchain-cardano/src/orders/green.rs`
- Create: `green-order-cardano-agent/src/account_events.rs`
- Create: `green-order-cardano-agent/src/account_index.rs`
- Modify: `green-order-cardano-agent/src/main.rs`

**Step 1: Add failing parser test**

Add a test that constructs a `TransactionOutput` at the Aleph account script address with inline `AccountState` datum.

Expected:

```rust
let account = AlephAccountUtxo::try_parse(output_ref, output, ctx).unwrap();
assert_eq!(account.state.store_root, [0; 32]);
assert_eq!(account.output_ref, output_ref);
```

Run:

```bash
cargo test -p bloom-offchain-cardano green::tests::parses_aleph_account_output -- --nocapture
```

Expected: fail.

**Step 2: Implement `AlephAccountUtxo`**

In `green.rs`:

```rust
pub struct AlephAccountUtxo {
    pub output_ref: OutputRef,
    pub output: TransactionOutput,
    pub state: AlephAccountState,
}
```

Do not derive `AccountId` from `OutputRef`. `OutputRef` is the account version, and it changes on every account update. The stable `AccountId` must come from the external intent/account registry from the first implementation.

Implement a plain parser, not a `Stable`/`EntitySnapshot` implementation:

```rust
impl AlephAccountUtxo {
    pub fn try_parse<C>(output_ref: OutputRef, repr: TransactionOutput, ctx: C) -> Option<Self>
    where
        C: Has<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>>,
    { ... }
}
```

Parser requirements:

- payment credential equals Aleph account script hash,
- datum is inline,
- datum parses to `AlephAccountState`,
- datum `magic` equals configured Aleph magic or `/Users/aleksandr/RustroverProjects/aleph/lib/constants.ak` value.

**Step 3: Add registry**

Create `green-order-cardano-agent/src/account_index.rs`:

```rust
pub struct AccountIndex {
    by_account_id: HashMap<AccountId, FinalizedTxOut>,
    by_output_ref: HashMap<OutputRef, AccountId>,
    unbound_by_output_ref: HashMap<OutputRef, AlephAccountUtxo>,
}
```

Expose methods:

- `observe_created_or_updated(AlephAccountUtxo)`
- `observe_consumed(OutputRef)`
- `current(AccountId) -> Option<FinalizedTxOut>`
- `bind_external_account_id(AccountId, OutputRef)`
- `mark_pending_update(AccountId, old_ref: OutputRef, new_output: TransactionOutput)` for local submission responsiveness if the predicted output ref is known.

Rules:

- Binding is created only from external intent/account metadata.
- When a bound output ref is consumed, keep the account unavailable until the produced continuation is observed or a local predicted update is recorded.
- When an unbound account UTxO is observed, keep it in `unbound_by_output_ref`; do not invent an `AccountId`.

**Step 4: Add a dedicated account event scanner**

Do not force account UTxOs through the existing `PairUpdateHandler`/`TLB` entity path. Account UTxOs are not tradable and do not have a stable ID on-chain.

Create `green-order-cardano-agent/src/account_events.rs` that scans each ledger/mempool transaction:

```rust
pub enum AccountEvent {
    Consumed(OutputRef),
    Produced(AlephAccountUtxo),
}
```

The scanner must:

- detect consumed refs from transaction inputs and remove/busy them in `AccountIndex`,
- parse produced outputs with `AlephAccountUtxo::try_parse`,
- update `AccountIndex`,
- run for ledger and mempool paths so pending account state is not double-used.

Keep `EvolvingCardanoEntity` limited to `GreenOrder` and `RoyaltyV1PoolOnly`.

**Step 5: Verify**

Run:

```bash
cargo test -p bloom-offchain-cardano green -- --nocapture
cargo check -p green-order-cardano-agent
```

Expected: pass.

## Task 4: Force Aleph Account Inputs And Outputs To The Front

**Files:**
- Modify: `bloom-offchain-cardano/src/execution_engine/execution_state.rs`
- Test: `bloom-offchain-cardano/src/execution_engine/execution_state.rs`

**Step 1: Write failing ordering test**

Create tests where:

- pool input ref sorts before account input ref,
- both are script inputs,
- the final transaction input order must still put the account input before the pool input,
- account output must still be first output,
- pool output must be second output,
- the pool redeemer still receives the real pool input index from the final order.

Expected assertion:

```rust
assert_eq!(tx.inputs().get(0), account_input);
assert_eq!(tx.inputs().get(1), pool_input);
assert_eq!(tx.outputs().get(0), account_continuation_output);
assert_eq!(tx.outputs().get(1), pool_output);
```

Also add negative tests proving the old behavior is invalid:

- if an inline-datum pool input is before an account input, Aleph `fold_ios` consumes output 0 for the pool and account validation pairs against the wrong output;
- if a payment-key/funding input with no inline datum is before an account input, Aleph `fold_ios` returns `True` before validating accounts.

Run:

```bash
cargo test -p bloom-offchain-cardano execution_engine::execution_state::test::aleph_account_inputs_and_outputs_are_first -- --nocapture
```

Expected: fail with current `TxBlueprint`.

**Step 2: Add explicit script IO kind**

Change:

```rust
pub struct ScriptInputBlueprint { ... }
```

to include:

```rust
pub enum ScriptInputRole {
    Normal,
    AlephAccount,
}

pub role: ScriptInputRole,
```

Default existing call sites to `ScriptInputRole::Normal`.

**Step 3: Build final input order with account inputs first**

In `TxBlueprint::project_onto_builder`:

1. Partition all IO into:
   - Aleph account script IO,
   - normal script IO,
   - funding/payment-key IO,
   - pure output-only IO.
2. Sort Aleph account script IO by `OutputRef`.
3. Sort normal script IO by `OutputRef`.
4. Sort funding/payment-key inputs by `OutputRef`.
5. Final transaction input order must be:
   - all Aleph account inputs,
   - all normal script inputs,
   - all funding/payment-key inputs.
6. Build `inputs_ordering` from that final order.
7. Add inputs in that final order.
8. Add outputs in this order:
   - all Aleph account continuation outputs in the same order as Aleph account inputs,
   - all normal script outputs in normal script input order,
   - all funding/operator output-only outputs.

Do not let generic `OutputRef` sorting move a pool or funding input before an Aleph account input.

**Step 4: Keep witness aggregation compatible**

Ensure existing `add_witness(...)` behavior still works for limit/order witnesses. Do not change semantics in this task except where needed for output ordering.

**Step 5: Verify**

Run:

```bash
cargo test -p bloom-offchain-cardano execution_engine::execution_state -- --nocapture
cargo test -p bloom-offchain-cardano green -- --nocapture
cargo check -p green-order-cardano-agent
```

Expected: pass.

## Task 5: Add Deterministic Aleph Batch Witness Aggregation

**Files:**
- Modify: `bloom-offchain-cardano/src/execution_engine/execution_state.rs`
- Modify: `bloom-offchain-cardano/src/orders/green.rs`
- Test: `bloom-offchain-cardano/src/execution_engine/execution_state.rs`

**Step 1: Add failing aggregation test**

Create two account IO blueprints with two different `AuthorizedIntention`s.

Expected:

- one withdrawal is added for Aleph batch witness,
- redeemer is `BatchRedeemer { intentions: [intent0, intent1] }`,
- order of intentions matches Aleph account input/output order from Task 4,
- ex units are `base + marginal * 2`.
- when a static witness and Aleph witness are both present, withdrawal order and `RedeemerTag::Reward` indexes are stable across repeated projections.

Run:

```bash
cargo test -p bloom-offchain-cardano execution_engine::execution_state::test::aleph_batch_witness_merges_intentions_in_account_order -- --nocapture
```

Expected: fail.

**Step 2: Introduce witness blueprint variants**

Replace the internal `witness_scripts` map:

```rust
HashMap<DeployedValidatorErased, (PlutusData, ScalingFactor)>
```

with a deterministic structure:

```rust
pub struct WitnessEntry {
    pub witness: DeployedValidatorErased,
    pub blueprint: WitnessBlueprint,
}

pub enum WitnessBlueprint {
    Static {
        redeemer: PlutusData,
        scaling_factor: u64,
    },
    AlephBatch {
        intentions: Vec<(OutputRef, AlephAuthorizedIntention)>,
    },
}
```

Store these entries in a `Vec<WitnessEntry>` and merge by `witness.hash` when adding. During projection, sort entries by `hex(witness.hash)` before adding withdrawals. Do not iterate a `HashMap` to determine withdrawal order.

Keep `TxBlueprint::add_witness(...)` for existing static witnesses; it should merge into the deterministic vector.

Add:

```rust
pub fn add_aleph_intention(
    &mut self,
    witness: DeployedValidatorErased,
    account_ref: OutputRef,
    intention: AlephAuthorizedIntention,
)
```

**Step 3: Build Aleph redeemer during projection**

When projecting `WitnessBlueprint::AlephBatch`:

1. Sort intentions by `inputs_ordering.index_of(account_ref)`.
2. Convert to `AlephBatchRedeemer`.
3. Add a zero withdrawal with the witness ref script.
4. Add withdrawals in sorted witness order.
5. Set ex units on `RedeemerTag::Reward` using the actual withdrawal index from that sorted order.

Important: current code sets every withdrawal ex units at `Reward, 0`. This must be fixed so multiple witness scripts cannot get the wrong ex-unit assignment.

Reject or panic during projection if an Aleph batch intention references an account ref that is not present as an `AlephAccount` script input in this transaction. This catches accidental witness redeemers that cannot line up with `fold_ios`.

**Step 4: Verify**

Run:

```bash
cargo test -p bloom-offchain-cardano execution_engine::execution_state -- --nocapture
cargo check -p green-order-cardano-agent
```

Expected: pass.

## Task 6: Implement Full-Fill `GreenOrder` BatchExec

**Files:**
- Modify: `bloom-offchain-cardano/src/execution_engine/instances.rs`
- Modify: `bloom-offchain-cardano/src/orders/green.rs`
- Test: `bloom-offchain-cardano/src/execution_engine/instances.rs`

**Step 1: Add failing full-fill execution test**

Build a `Take<GreenOrder, FinalizedTxOut>` where:

- account input contains 1_000 lovelace and one non-ADA token,
- leaving asset is ADA,
- arriving asset is token,
- leaving amount is 500,
- expected arriving amount is 500,
- fee lovelace is 10,
- result is `Next::Term(TerminalTake { remaining_input: 0, accumulated_output: 500, ... })`.

Expected produced account output:

- same account address,
- inline account datum,
- lovelace decreased by `removed_input + consumed_fee`,
- arriving token increased by `added_output`,
- nonce target slot set to target nonce,
- store unchanged,
- role is `ScriptInputRole::AlephAccount`,
- account input redeemer is `Delegate(delegate_index)`,
- Aleph batch witness has one authorized intention with `remainder = 0`.
- operator key hash is present as the first extra signer / required signer,
- `order.intention.operator` equals the runtime `OperatorCred`.

Add negative tests:

- missing operator signer fails transaction/witness validation,
- wrong first extra signer fails witness validation,
- `order.intention.operator != context OperatorCred` is rejected before building blueprint,
- `delegate_index` out of range is rejected,
- `account_state.allowlist[delegate_index] != aleph_batch_witness.hash` is rejected,
- non-ADA leaving asset is rejected in Phase 1.

Run:

```bash
cargo test -p bloom-offchain-cardano execution_engine::instances::tests::green_order_full_fill_updates_aleph_account_utxo -- --nocapture
```

Expected: fail because `GreenOrder` execution panics.

**Step 2: Correct `GreenOrder` fee/budget semantics**

Phase 1 uses Aleph `fee_lovelace` as operator compensation, not as the transaction execution budget.

Update `MarketTaker for GreenOrder`:

```rust
fn fee(&self) -> FeeAsset<u64> { self.intention.fee_lovelace }
fn budget(&self) -> FeeAsset<u64> { 0 }
fn consumable_budget(&self) -> FeeAsset<u64> { 0 }
```

Keep `operator_fee(input_consumed)` proportional:

```rust
fee_lovelace * input_consumed / leaving_amount
```

Add tests proving full fill consumes full operator fee and zero execution budget.

Specifically test the finalized transition path, not only `operator_fee(...)`:

```rust
let final_take = take_in_progress.finalized(0);
assert_eq!(final_take.0.consumed_fee(), order.intention.fee_lovelace);
assert_eq!(final_take.0.consumed_budget(), 0);
```

If this fails, fix `try_terminate`/`with_fee_charged` interaction so `TerminalTake.remaining_fee` is reduced by the charged operator fee in finalized recipes.

**Step 3: Implement account value transition helper**

In `green.rs`, add:

```rust
pub fn apply_full_fill_to_account_output(
    output: &mut TransactionOutput,
    order: &GreenOrder,
    removed_input: u64,
    added_output: u64,
    consumed_fee: u64,
) -> Result<(), GreenExecutionError>
```

Rules matching `witness.ak`:

- If leaving asset is ADA: subtract `removed_input + consumed_fee` lovelace.
- If leaving asset is non-ADA: subtract `removed_input` of leaving asset.
- If arriving asset is ADA: add `added_output - consumed_fee` lovelace; reject if `added_output < consumed_fee`.
- If arriving asset is non-ADA: add `added_output` of arriving asset.
- If neither leaving nor arriving asset is ADA: subtract `consumed_fee` lovelace.
- Preserve every unrelated token exactly.
- Never mutate the account address.

**Step 4: Implement account datum transition helper**

For `Auth::Sig` and `remainder == 0`:

- parse current `AccountState` from input datum,
- update `nonce[target_nonce.index] = target_nonce.value`,
- preserve `store_root`,
- write inline datum back into output.

Reject:

- `Auth::Path`,
- `remainder > 0`,
- target nonce index out of bounds,
- current nonce greater than target nonce,
- missing inline datum.

**Step 5: Implement `BatchExec` body**

In `instances.rs`, replace the panic:

```rust
impl<Ctx> BatchExec<ExecutionState, EffectPreview<GreenOrder>, Ctx>
    for Magnet<Take<GreenOrder, FinalizedTxOut>>
where
    Ctx: Has<NetworkId>
        + Has<OperatorCred>
        + Has<DeployedValidator<{ ALEPH_ACCOUNT_VALIDATOR }>>
        + Has<DeployedValidator<{ ALEPH_BATCH_WITNESS_VALIDATOR }>>,
{
    fn exec(...)
}
```

Implementation outline:

1. Extract `removed_input`, `added_output`, `consumed_fee`, `consumed_budget`.
2. Assert/reject `consumed_budget == 0`.
3. Require `order.intention.leaving_asset` is ADA for Phase 1.
4. Require `order.intention.operator == Ed25519KeyHash::from(context.select::<OperatorCred>())`.
5. Require `order.delegate_index` is in bounds and `order.account_state.allowlist[delegate_index] == context.select::<DeployedValidator<{ ALEPH_BATCH_WITNESS_VALIDATOR }>>().hash`.
6. Require `result` is `Next::Term` with `remaining_input == 0`.
7. Build `ScriptInputBlueprint`:
   - `role: ScriptInputRole::AlephAccount`,
   - `reference: account_ref`,
   - `utxo: consumed_account_output.clone()`,
   - script hash/cost from Aleph account validator,
   - redeemer `AlephAccountAction::Delegate(order.delegate_index).into_pd()`,
   - required signers contains exactly the operator key hash needed by the witness.
8. Ensure `TxBlueprint`/transaction builder emits the operator as the first extra signer. If CML `RequiredSigners` does not preserve order, add an explicit ordered-extra-signers field to `TxBlueprint` and test it.
9. Clone consumed account output into candidate output.
10. Apply account value transition.
11. Apply account datum transition.
12. Add account IO to `state.tx_blueprint`.
13. Add account validator reference input.
14. Add Aleph batch witness intention:
    - `intent` equals original Aleph intention,
    - `remainder = 0`,
    - `auth` equals the original `GreenAuth::Sig { prefix, postfix, signature, update_proof: [] }`.
15. Add witness reference input through `add_aleph_intention`.
16. `state.add_operator_interest(consumed_fee)`.
17. Return `ExecutionEff::Eliminated(consumed_bundle)` for full fill. The green taker is eliminated from `TLB`; the produced account output is emitted through the tx blueprint and tracked by `AccountIndex` through local prediction/ledger observation, not as a produced taker.

**Step 6: Verify**

Run:

```bash
cargo test -p bloom-offchain-cardano green -- --nocapture
cargo test -p bloom-offchain-cardano execution_engine::instances::tests::green_order_full_fill_updates_aleph_account_utxo -- --nocapture
cargo check -p green-order-cardano-agent
```

Expected: pass.

## Task 7: Add End-To-End Recipe Interpretation Test

**Files:**
- Modify: `bloom-offchain-cardano/src/execution_engine/interpreter.rs`
- Modify: `bloom-offchain-cardano/src/execution_engine/instances.rs`

**Step 1: Add failing integration test**

Build a one-take/one-make `ExecutionRecipe<GreenOrder, RoyaltyV1PoolOnly, FinalizedTxOut>` with:

- one Aleph account input,
- one Royalty V1 pool input,
- one operator funding input,
- one full-fill trade.

Run it through `CardanoRecipeInterpreter`.

Assert final unsigned tx has:

- account script input with `Delegate(index)`,
- account input appears before pool and funding inputs,
- operator key hash is the first extra signer,
- pool script input with Royalty V1 swap redeemer,
- batch witness withdrawal with one authorized intention,
- account output first,
- pool output after account output,
- operator reward output if fee is above min ADA threshold,
- no limit-order witness/reference input.

Run:

```bash
cargo test -p bloom-offchain-cardano execution_engine::interpreter::tests::green_order_royalty_v1_recipe_builds_aleph_tx -- --nocapture
```

Expected: fail until previous tasks are wired.

**Step 2: Implement missing test fixtures**

Use minimal synthetic CML `TransactionOutput`s where possible. Avoid network calls.

If Royalty V1 datum construction is too expensive for this test, split into:

- account/witness tx-building integration test using a dummy maker effect,
- existing Royalty V1 pool execution tests for pool output update.

**Step 3: Verify**

Run:

```bash
cargo test -p bloom-offchain-cardano execution_engine::interpreter::tests::green_order_royalty_v1_recipe_builds_aleph_tx -- --nocapture
cargo check -p green-order-cardano-agent
```

Expected: pass.

## Task 8: Add External Intent Admission Guardrails

**Files:**
- Modify: `green-order-cardano-agent/src/main.rs`
- Create: `green-order-cardano-agent/src/intent_source.rs`
- Modify: `green-order-cardano-agent/src/config.rs`

**Step 1: Add failing admission tests**

Tests:

- rejects `Auth::Path` when `greenOrders.allowPartial=false`,
- rejects any intent whose current account UTxO is missing from `AccountIndex`,
- rejects intent if target nonce is lower than current account nonce,
- rejects intent if leaving asset is not ADA in Phase 1,
- rejects intent if full-fill liquidity check cannot satisfy `leaving_amount`,
- injects accepted intent as `Bundled<GreenOrder, FinalizedTxOut(account_utxo, account_ref)>`.

**Step 2: Add config**

```rust
#[derive(Deserialize)]
pub struct GreenOrdersConfig {
    #[serde(default)]
    pub allow_partial: bool,
    pub intent_source: IntentSourceConfig,
}
```

Default `allow_partial=false`.

**Step 3: Implement minimal source abstraction**

Create trait:

```rust
pub trait GreenIntentSource {
    type Stream: Stream<Item = RawGreenIntent>;
    fn stream(self) -> Self::Stream;
}
```

Phase 1 can use a JSON-lines/local channel source so execution can be tested without a production relay.

**Step 4: Verify**

Run:

```bash
cargo test -p green-order-cardano-agent intent_source -- --nocapture
cargo check -p green-order-cardano-agent
```

Expected: pass.

## Task 9: Final Verification And Failure-Mode Tests

**Files:**
- Modify tests only unless failures expose implementation gaps.

**Required tests:**

- Full fill with leaving ADA and arriving token.
- Reject leaving token and arriving ADA in Phase 1.
- Token-to-token is not a required passing scenario until the current Aleph validator arithmetic is fixed or proven with golden tests. `witness.ak` currently computes one non-ADA leaving-output quantity using the arriving asset identifiers, so implementation must either reject token-to-token in admission or add an Aleph validator-level proof before enabling it.
- Reject partial fill before tx building.
- Reject `Auth::Path` in Phase 1.
- Reject account datum without Aleph magic.
- Reject account continuation if nonce target index is out of range.
- Reject account continuation if current nonce is already greater than target nonce.
- Verify account inputs and outputs are first even when pool/funding refs sort before account refs.
- Verify multiple green orders in one batch produce one witness withdrawal with intentions in account-input order.

**Final commands:**

```bash
cargo test -p bloom-offchain-cardano green -- --nocapture
cargo test -p bloom-offchain-cardano execution_engine -- --nocapture
cargo test -p green-order-cardano-agent -- --nocapture
cargo check -p green-order-cardano-agent
```

Expected: pass.

## Open Decisions Before Implementation

- Real `alephAccount` and `alephBatchWitness` reference UTxOs must be supplied for preprod/mainnet resources.
- Product must supply or approve an external stable account registry. The validator datum does not contain `account_id`, and deriving it from current `OutputRef` is invalid because the output ref changes on every account update.
- Phase 2 partial fills require MPF proof generation and store-root indexing. Do not implement partial fills in this plan.
- Phase 2 `Auth::Path` needs a separate security decision because current witness logic allows path-auth full fill without nonce movement.
