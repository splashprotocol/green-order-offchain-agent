# Green Order Merkle Updates Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add complete offchain Merkle Patricia Forestry handling for green-order continuations without changing Aleph validators: insert a partially filled new intent, update a partially filled continuation, and safely finish a fully filled stored intent by preserving the on-chain root and marking the leaf completed locally.

**Architecture:** Introduce an account-scoped MPF mirror in the green-order agent and make every account execution compute an explicit `StoreDelta` before building the transaction. The offchain executor must derive the same intent key, intent digest, updated intent, proof, and next store root that the existing Aleph witness validator verifies. Aleph source code is out of scope and must remain unchanged; therefore `Path` full-fill completion must use `mpf.has(...)`, preserve the account store root, and update only durable local completion state.

**Tech Stack:** Rust workspace, `bloom-offchain-cardano`, `green-order-cardano-agent`, Aiken Aleph validators in `/Users/aleksandr/RustroverProjects/aleph`, Aiken `merkle_patricia_forestry`, CML CBOR/PlutusData encoding, `cargo test`, `aiken check`.

---

## Current Source Facts

- Aleph account datum stores the MPF root in `AccountState.store`.
- Offchain parses this field as `AlephAccountState.store_root: [u8; 32]` in `bloom-offchain-cardano/src/orders/green.rs`.
- Current phase-1 execution rejects partial execution and `Auth::Path` in `bloom-offchain-cardano/src/execution_engine/instances.rs`.
- Current `GreenAuth::new_sig` rejects non-empty `update_proof`.
- Current `AccountIndex` stores only account UTxOs. It does not store MPF snapshots, pending leaves, proofs, or root history.
- Aleph `validators/witness.ak` currently verifies:
  - `Sig` + `remainder > 0`: `next.store == mpf.insert(old.store, cbor(target_nonce), digest(updated_intent), update_proof)`.
  - `Sig` + `remainder == 0`: no store update required.
  - `Path` + `remainder > 0`: `next.store == mpf.update(old.store, cbor(target_nonce), proof, digest(intent), digest(updated_intent))`.
  - `Path` + `remainder == 0`: only `mpf.has(old.store, cbor(target_nonce), digest(intent), proof)`.
- Therefore true deletion on full `Path` completion is **not currently enforced on-chain**. Because Aleph validators must not be changed in this project, the offchain agent must leave the leaf in the root and mark it completed in durable local state so it is not emitted again.

## Required Semantics

Use these cases as the implementation contract:

| Case | Auth | Execution result | Store operation | Nonce operation |
| --- | --- | --- | --- | --- |
| New intent, full fill | `Sig` | `remainder == 0` | preserve root | advance nonce |
| New intent, partial fill | `Sig` | `remainder > 0` | insert updated remaining intent | advance nonce |
| Stored intent, partial fill | `Path` | `remainder > 0` | update existing leaf to updated remaining intent | preserve nonce |
| Stored intent, full fill | `Path` | `remainder == 0` | preserve root and mark local leaf completed | preserve nonce |

Intent identity must be deterministic:

```text
intent_key = cbor.serialise((target_nonce_index, target_nonce_value))
intent_digest = blake2b_256(cbor.serialise(intent))
```

The updated remaining intent must match the Aiken formula exactly:

```text
updated.leaving_amount = leaving_remainder
updated.expected_arriving_amount = expected_arriving_amount - arriving_asset_added_without_fee
updated.fee_lovelace = fee_lovelace - fee_consumed
fee_remainder = leaving_remainder * fee_lovelace / leaving_amount
fee_consumed = fee_lovelace - fee_remainder
```

Do not use Rust rounding behavior that differs from Aiken integer division.

---

## Task 1: Add Golden Encoding Tests For MPF Keys And Intent Digests

**Files:**
- Modify: `bloom-offchain-cardano/src/orders/green.rs`
- Test: `bloom-offchain-cardano/src/orders/green.rs`
- Reference: `/Users/aleksandr/RustroverProjects/aleph/validators/witness.ak`

**Step 1: Write failing tests**

Add tests that pin:

```rust
#[test]
fn aleph_intention_digest_is_stable() {
    let intent = sample_aleph_intention();
    assert_eq!(
        hex::encode(intent.digest()),
        "REPLACE_WITH_AIKEN_GOLDEN_DIGEST"
    );
}

#[test]
fn target_nonce_key_cbor_is_stable() {
    let key = aleph_intent_key(0, 42);
    assert_eq!(hex::encode(key), "REPLACE_WITH_AIKEN_GOLDEN_CBOR");
}
```

Generate the golden values from a temporary external Aiken script, read-only Aleph inspection, or committed Rust fixtures in this repository. Do not modify files under `/Users/aleksandr/RustroverProjects/aleph`.

**Step 2: Run tests and verify failure**

Run:

```bash
cargo test -p bloom-offchain-cardano green::tests::aleph_intention_digest_is_stable -- --nocapture
cargo test -p bloom-offchain-cardano green::tests::target_nonce_key_cbor_is_stable -- --nocapture
```

Expected: fail until helpers and golden values exist.

**Step 3: Implement helpers**

Add:

```rust
pub fn aleph_intent_key(target_nonce_index: u16, target_nonce_value: u64) -> Vec<u8> {
    aiken_constr(
        0,
        vec![
            u64::from(target_nonce_index).into_pd(),
            target_nonce_value.into_pd(),
        ],
    )
    .to_cbor_bytes()
}
```

If this does not match Aiken `cbor.serialise(intent.target_nonce)`, replace it with the exact PlutusData representation used by Aiken for tuple `(Int, Nonce)`.

**Step 4: Verify**

Run:

```bash
cargo test -p bloom-offchain-cardano green::tests::aleph_intention_digest_is_stable -- --nocapture
cargo test -p bloom-offchain-cardano green::tests::target_nonce_key_cbor_is_stable -- --nocapture
```

Expected: pass.

**Step 5: Commit**

```bash
git add bloom-offchain-cardano/src/orders/green.rs
git commit -m "test: pin green intent merkle encodings"
```

---

## Task 2: Introduce StoreDelta And Remaining Intent Calculation

**Files:**
- Modify: `bloom-offchain-cardano/src/orders/green.rs`
- Test: `bloom-offchain-cardano/src/orders/green.rs`

**Step 1: Write failing tests**

Add tests for exact Aiken arithmetic:

```rust
#[test]
fn computes_remaining_intent_for_partial_fill() {
    let original = sample_aleph_intention_with_amounts(1_000, 900, 100);
    let updated = original.remaining_after_fill(400, 360).unwrap();

    assert_eq!(updated.leaving_amount, 600);
    assert_eq!(updated.expected_arriving_amount, 540);
    assert_eq!(updated.fee_lovelace, 60);
}

#[test]
fn full_fill_has_no_remaining_intent() {
    let original = sample_aleph_intention_with_amounts(1_000, 900, 100);
    assert_eq!(original.remaining_after_fill(1_000, 900), None);
}
```

**Step 2: Run tests and verify failure**

Run:

```bash
cargo test -p bloom-offchain-cardano green::tests::computes_remaining_intent_for_partial_fill -- --nocapture
cargo test -p bloom-offchain-cardano green::tests::full_fill_has_no_remaining_intent -- --nocapture
```

Expected: fail because the helper does not exist.

**Step 3: Implement minimal calculation**

Add:

```rust
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum GreenStoreOp {
    Preserve,
    InsertRemaining {
        key: Vec<u8>,
        old_root: [u8; 32],
        new_root: [u8; 32],
        updated_intent: AlephIntention,
        proof: Vec<u8>,
    },
    UpdateRemaining {
        key: Vec<u8>,
        old_root: [u8; 32],
        new_root: [u8; 32],
        old_intent_digest: [u8; 32],
        updated_intent: AlephIntention,
        proof: Vec<u8>,
    },
    CompleteExisting {
        key: Vec<u8>,
        old_root: [u8; 32],
        old_intent_digest: [u8; 32],
        proof: Vec<u8>,
    },
}
```

Add:

```rust
impl AlephIntention {
    pub fn remaining_after_fill(
        &self,
        consumed_leaving_without_fee: u64,
        added_arriving_without_fee: u64,
    ) -> Option<Self> {
        let leaving_remainder = self.leaving_amount.checked_sub(consumed_leaving_without_fee)?;
        if leaving_remainder == 0 {
            return None;
        }
        let fee_remainder = leaving_remainder
            .checked_mul(self.fee_lovelace)?
            .checked_div(self.leaving_amount)?;
        Some(Self {
            leaving_amount: leaving_remainder,
            expected_arriving_amount: self
                .expected_arriving_amount
                .checked_sub(added_arriving_without_fee)?,
            fee_lovelace: fee_remainder,
            ..self.clone()
        })
    }
}
```

**Step 4: Verify**

Run:

```bash
cargo test -p bloom-offchain-cardano green::tests::computes_remaining_intent_for_partial_fill -- --nocapture
cargo test -p bloom-offchain-cardano green::tests::full_fill_has_no_remaining_intent -- --nocapture
```

Expected: pass.

**Step 5: Commit**

```bash
git add bloom-offchain-cardano/src/orders/green.rs
git commit -m "feat: model green intent store deltas"
```

---

## Task 3: Add Local MPF Snapshot Storage To AccountIndex

**Files:**
- Modify: `green-order-cardano-agent/src/account_index.rs`
- Create: `green-order-cardano-agent/src/account_store.rs`
- Modify: `green-order-cardano-agent/src/main.rs`
- Test: `green-order-cardano-agent/src/account_store.rs`

**Step 1: Choose MPF implementation**

First inspect whether the Rust workspace already has a compatible MPF crate. If not, add the smallest compatible dependency to `green-order-cardano-agent/Cargo.toml` and `bloom-offchain-cardano/Cargo.toml`.

Preferred order:

1. A Rust library that produces proofs compatible with Aiken `aiken/merkle_patricia_forestry`.
2. A thin local wrapper around the exact proof format, backed by golden tests against Aiken.
3. Do not hand-roll proof bytes without Aiken golden tests.

**Step 2: Write failing store tests**

Create tests:

```rust
#[test]
fn empty_store_root_matches_aleph_null_hash() {
    let store = AccountStore::empty();
    assert_eq!(store.root(), ALEPH_MPF_EMPTY_ROOT);
}

#[test]
fn insert_remaining_intent_changes_root_and_produces_proof() {
    let mut store = AccountStore::empty();
    let intent = sample_remaining_intent();
    let digest = intent.digest();
    let delta = store.insert_remaining(sample_key(), intent.clone()).unwrap();

    assert_eq!(delta.old_root, ALEPH_MPF_EMPTY_ROOT);
    assert_ne!(delta.new_root, delta.old_root);
    assert!(!delta.proof.is_empty());
    let leaf = store.pending_leaf(&sample_key()).unwrap();
    assert_eq!(leaf.intent, intent);
    assert_eq!(leaf.digest, digest);
    assert_eq!(leaf.status, StoredIntentStatus::Pending);
}
```

**Step 3: Run tests and verify failure**

Run:

```bash
cargo test -p green-order-cardano-agent account_store -- --nocapture
```

Expected: fail because `account_store` does not exist.

**Step 4: Implement AccountStore**

Implement:

```rust
pub struct AccountStore {
    root: [u8; 32],
    leaves: BTreeMap<Vec<u8>, StoredIntentLeaf>,
}

pub struct StoredIntentLeaf {
    pub intent: AlephIntention,
    pub digest: [u8; 32],
    pub status: StoredIntentStatus,
}

pub enum StoredIntentStatus {
    Pending,
    Completed,
}

impl AccountStore {
    pub fn empty() -> Self;
    pub fn root(&self) -> [u8; 32];
    pub fn insert_remaining(&mut self, key: Vec<u8>, intent: AlephIntention) -> Result<MpfDelta, StoreError>;
    pub fn update_remaining(&mut self, key: Vec<u8>, old_digest: [u8; 32], new_intent: AlephIntention) -> Result<MpfDelta, StoreError>;
    pub fn prove_existing(&self, key: &[u8], digest: [u8; 32]) -> Result<Vec<u8>, StoreError>;
    pub fn mark_completed(&mut self, key: Vec<u8>, old_digest: [u8; 32]) -> Result<MpfCompletion, StoreError>;
    pub fn pending_leaf(&self, key: &[u8]) -> Option<&StoredIntentLeaf>;
    pub fn pending_leaves(&self) -> impl Iterator<Item = (&Vec<u8>, &StoredIntentLeaf)>;
}
```

The `root` must be computed by the compatible MPF implementation, not by hashing `leaves` manually. The MPF leaf value is the digest, but the offchain store must also retain the full remaining intent and completion status; a digest alone is not enough to reconstruct a continuation order safely.

**Step 5: Extend AccountIndex**

Change `AccountIndex` to store:

```rust
by_account_id: HashMap<AccountId, IndexedAccount>,

pub struct IndexedAccount {
    pub utxo: FinalizedTxOut,
    pub store: AccountStore,
}
```

For initially observed accounts:

- If `store_root == empty_root`, initialize `AccountStore::empty()`.
- If `store_root != empty_root`, require a persisted snapshot for that account or mark it unexecutable until reconstructed.

Do not guess store contents from root alone.

**Step 6: Verify**

Run:

```bash
cargo test -p green-order-cardano-agent account_index account_store -- --nocapture
cargo check -p green-order-cardano-agent
```

Expected: pass.

**Step 7: Commit**

```bash
git add green-order-cardano-agent/src/account_index.rs green-order-cardano-agent/src/account_store.rs green-order-cardano-agent/src/main.rs green-order-cardano-agent/Cargo.toml
git commit -m "feat: track green account merkle stores"
```

---

## Task 4: Generate Insert Proof For New Partial Sig Intent

**Files:**
- Modify: `bloom-offchain-cardano/src/orders/green.rs`
- Modify: `green-order-cardano-agent/src/intent_source.rs`
- Modify: `green-order-cardano-agent/src/context.rs`
- Modify: `green-order-cardano-agent/src/account_index.rs`
- Modify: `bloom-offchain-cardano/src/execution_engine/instances.rs`
- Test: `green-order-cardano-agent/src/intent_source.rs`
- Test: `green-order-cardano-agent/src/context.rs`
- Test: `green-order-cardano-agent/src/account_index.rs`
- Test: `bloom-offchain-cardano/src/execution_engine/instances.rs`

**Step 1: Write failing admission test**

Add a test where `allow_partial = true`, a `Sig` intent is admitted, and the account store snapshot is available.

Expected admitted order contains:

- original signed intent,
- `update_proof` for insert is still empty before execution,
- enough metadata to compute insert delta during execution.

**Step 2: Run failing test**

Run:

```bash
cargo test -p green-order-cardano-agent intent_source::tests::admits_sig_intent_when_partial_enabled -- --nocapture
```

Expected: fail until partial admission is enabled.

**Step 3: Add shared store-planning context API**

`bloom-offchain-cardano` must not depend on `green-order-cardano-agent`, so do not import `AccountStore` into `bloom-offchain-cardano`. Define a trait in `bloom-offchain-cardano/src/orders/green.rs` that returns serializable store planning data:

```rust
pub trait GreenStorePlanner {
    fn plan_sig_insert(
        &self,
        account_id: AccountId,
        old_account_ref: spectrum_cardano_lib::OutputRef,
        key: Vec<u8>,
        updated_intent: AlephIntention,
    ) -> Result<PlannedStoreDelta, GreenStorePlanningError>;

    fn plan_path_update(
        &self,
        account_id: AccountId,
        old_account_ref: spectrum_cardano_lib::OutputRef,
        key: Vec<u8>,
        old_digest: [u8; 32],
        updated_intent: AlephIntention,
    ) -> Result<PlannedStoreDelta, GreenStorePlanningError>;

    fn plan_path_completion(
        &self,
        account_id: AccountId,
        old_account_ref: spectrum_cardano_lib::OutputRef,
        key: Vec<u8>,
        old_digest: [u8; 32],
    ) -> Result<PlannedStoreCompletion, GreenStorePlanningError>;
}

pub struct PlannedStoreDelta {
    pub old_root: [u8; 32],
    pub new_root: [u8; 32],
    pub proof: Vec<u8>,
    pub predicted_snapshot_id: StoreSnapshotId,
}

pub struct PlannedStoreCompletion {
    pub root: [u8; 32],
    pub proof: Vec<u8>,
    pub predicted_snapshot_id: StoreSnapshotId,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, serde::Serialize, serde::Deserialize)]
pub struct StoreSnapshotId(u64);
```

Define `StoreSnapshotId` in `bloom-offchain-cardano/src/orders/green.rs` beside the planning structs so both `ExecutionState` and the binary crate can pass it without a dependency cycle. Generate ids inside `AccountIndex` with a monotonic in-memory counter; do not persist unconfirmed ids as durable state.

Implement this trait for `green-order-cardano-agent/src/context.rs` by delegating to `AccountIndex`. `AccountIndex` must compute the plan from a cloned current `AccountStore` snapshot, save the clone as a pending predicted snapshot keyed by `StoreSnapshotId`, and leave the committed store unchanged.

**Step 4: Implement execution-time insert delta**

In `GreenOrder::exec`, when `Auth::Sig` and transition result is partial:

1. Compute `leaving_remainder`.
2. Compute `updated_intent`.
3. Compute `intent_key`.
4. Call `context.plan_sig_insert(...)`.
5. Use the returned proof and new root.
6. Set `next_state.store_root = insert_delta.new_root`.
7. Set `authorized.remainder = leaving_remainder`.
8. Set `authorized.auth = Sig { update_proof: insert_delta.proof, ..original_sig }`.
9. Keep nonce update exactly as the current full-fill `Sig` path does.
10. Include the returned `predicted_snapshot_id` in the execution preview/effect so the agent can bind that pending snapshot only after the predicted account output is observed.

Do not mutate the committed local account store inside `GreenOrder::exec`. Transaction construction only creates a predicted snapshot and pending delta.

**Step 5: Verify against Aiken**

Do not modify Aleph source files. If a golden proof fixture is needed, generate it from a separate temporary script or from committed Rust test fixtures in this repository only.

Run:

```bash
cd /Users/aleksandr/RustroverProjects/aleph
aiken check
```

Expected: pass. This command is read-only verification of the existing Aleph validators; do not edit files under `/Users/aleksandr/RustroverProjects/aleph`.

**Step 6: Verify Rust**

Run:

```bash
cargo test -p green-order-cardano-agent intent_source -- --nocapture
cargo test -p bloom-offchain-cardano green -- --nocapture
cargo check -p green-order-cardano-agent
```

Expected: pass.

**Step 7: Commit**

```bash
git add bloom-offchain-cardano/src/orders/green.rs bloom-offchain-cardano/src/execution_engine/instances.rs green-order-cardano-agent/src/intent_source.rs green-order-cardano-agent/src/context.rs green-order-cardano-agent/src/account_index.rs
git commit -m "feat: insert remaining green intents into merkle store"
```

---

## Task 5: Generate Path Orders From Stored Leaves

**Files:**
- Modify: `green-order-cardano-agent/src/account_store.rs`
- Create: `green-order-cardano-agent/src/continuation_scanner.rs`
- Modify: `green-order-cardano-agent/src/main.rs`
- Test: `green-order-cardano-agent/src/continuation_scanner.rs`

**Step 1: Write failing scanner test**

Create a test with one account store containing one remaining intent leaf. The scanner must emit a `RawGreenIntent` or `GreenOrder` with:

- `Auth::Path { proof }`,
- same target nonce,
- remaining `leaving_amount`,
- remaining `expected_arriving_amount`,
- remaining `fee_lovelace`,
- same operator hash.

**Step 2: Run failing test**

Run:

```bash
cargo test -p green-order-cardano-agent continuation_scanner -- --nocapture
```

Expected: fail because scanner does not exist.

**Step 3: Implement scanner**

The scanner must:

1. Iterate executable accounts.
2. Iterate store leaves.
3. Generate `mpf.has` proof for each leaf.
4. Emit a local event into the same pair-partitioned event stream used by HTTP/TCP ingress.

Do not resubmit a continuation while the account is pending update.

**Step 4: Verify**

Run:

```bash
cargo test -p green-order-cardano-agent continuation_scanner account_store -- --nocapture
cargo check -p green-order-cardano-agent
```

Expected: pass.

**Step 5: Commit**

```bash
git add green-order-cardano-agent/src/continuation_scanner.rs green-order-cardano-agent/src/account_store.rs green-order-cardano-agent/src/main.rs
git commit -m "feat: emit green path continuations from merkle store"
```

---

## Task 6: Generate Update Proof For Partial Path Continuation

**Files:**
- Modify: `bloom-offchain-cardano/src/execution_engine/instances.rs`
- Modify: `bloom-offchain-cardano/src/orders/green.rs`
- Modify: `green-order-cardano-agent/src/account_store.rs`
- Modify: `green-order-cardano-agent/src/account_index.rs`
- Modify: `green-order-cardano-agent/src/context.rs`
- Test: `bloom-offchain-cardano/src/execution_engine/instances.rs`
- Test: `green-order-cardano-agent/src/account_store.rs`
- Test: `green-order-cardano-agent/src/account_index.rs`
- Test: `green-order-cardano-agent/src/context.rs`

**Step 1: Write failing tests**

Add tests for:

- existing leaf digest is required,
- wrong old digest is rejected,
- partial `Path` updates root,
- authorized redeemer contains `remainder > 0`.

**Step 2: Run failing tests**

Run:

```bash
cargo test -p green-order-cardano-agent account_store::tests::update_requires_existing_digest -- --nocapture
cargo test -p bloom-offchain-cardano green_path_partial -- --nocapture
```

Expected: fail until path partial execution is implemented.

**Step 3: Implement update delta**

In `GreenOrder::exec`, when `Auth::Path` and transition result is partial:

1. Compute `intent_digest` from the current remaining intent.
2. Compute updated remaining intent.
3. Call `context.plan_path_update(...)`.
4. Set `next_state.store_root = update_delta.new_root`.
5. Set `authorized.remainder = leaving_remainder`.
6. Set `authorized.auth = Path { proof: update_delta.proof }`.
7. Preserve nonce unchanged.
8. Include the returned `predicted_snapshot_id` in the execution preview/effect so confirmation can bind the pending snapshot.

Do not mutate the committed local account store inside `GreenOrder::exec`. The update proof must be computed from a cloned snapshot held as pending until the matching account output is observed.

**Step 4: Verify against Aiken**

Run:

```bash
cd /Users/aleksandr/RustroverProjects/aleph
aiken check
```

Expected: existing `Path` partial validator tests pass. Any new golden tests or fixtures required by this implementation must live in `green-order-offchain-agent` or a temporary external script, never under `/Users/aleksandr/RustroverProjects/aleph`.

**Step 5: Verify Rust**

Run:

```bash
cargo test -p green-order-cardano-agent account_store continuation_scanner -- --nocapture
cargo test -p bloom-offchain-cardano green -- --nocapture
cargo check -p green-order-cardano-agent
```

Expected: pass.

**Step 6: Commit**

```bash
git add bloom-offchain-cardano/src/orders/green.rs bloom-offchain-cardano/src/execution_engine/instances.rs green-order-cardano-agent/src/account_store.rs green-order-cardano-agent/src/account_index.rs green-order-cardano-agent/src/context.rs
git commit -m "feat: update green merkle continuations"
```

---

## Task 7: Implement Full Path Completion Without On-Chain Delete

**Files:**
- Modify: `green-order-cardano-agent/src/account_store.rs`
- Modify: `green-order-cardano-agent/src/account_index.rs`
- Modify: `green-order-cardano-agent/src/context.rs`
- Modify: `green-order-cardano-agent/src/continuation_scanner.rs`
- Modify: `bloom-offchain-cardano/src/execution_engine/instances.rs`
- Modify: `bloom-offchain-cardano/src/orders/green.rs`

Aleph validator source must not be changed. Do not implement offchain deletion because the current validator does not enforce it. Offchain must:

1. Generate `mpf.has` proof.
2. Preserve `next_state.store_root`.
3. Emit `AuthorizedIntention { remainder: 0, auth: Path { proof } }`.
4. Create a pending predicted snapshot where the leaf status is `Completed`, but leave the committed store unchanged.
5. Mark the leaf as locally completed only after tx confirmation so scanner does not emit it again.
5. Accept that the on-chain MPF root still contains the old leaf.

This leaks completed leaves into the root forever and requires durable local completed-state tracking. That is the required tradeoff while Aleph remains unchanged.

**Step 1: Write failing test**

```rust
#[test]
fn path_full_fill_preserves_root_but_marks_leaf_completed() {
    let mut store = store_with_one_leaf();
    let old_root = store.root();
    store.mark_completed(sample_key(), sample_digest()).unwrap();
    assert_eq!(store.root(), old_root);
    assert!(!store.pending_leaves().any(|leaf| leaf.key == sample_key()));
}
```

**Step 2: Run failing test**

Run:

```bash
cargo test -p green-order-cardano-agent account_store::tests::path_full_fill -- --nocapture
```

Expected: fail.

**Step 3: Implement completion marker**

Implement only root-preserving completion through `context.plan_path_completion(...)`. Do not support a delete runtime branch. The planned completion must clone the current store, mark the cloned leaf completed, keep the same root, and return an `mpf.has` proof plus `predicted_snapshot_id`.

**Step 4: Verify**

```bash
cargo test -p green-order-cardano-agent account_store continuation_scanner -- --nocapture
cargo test -p bloom-offchain-cardano green -- --nocapture
cargo check -p green-order-cardano-agent
```

Expected: pass.

**Step 5: Commit**

```bash
git add bloom-offchain-cardano/src/orders/green.rs bloom-offchain-cardano/src/execution_engine/instances.rs green-order-cardano-agent/src/account_store.rs green-order-cardano-agent/src/account_index.rs green-order-cardano-agent/src/context.rs green-order-cardano-agent/src/continuation_scanner.rs
git commit -m "feat: complete green path intents without merkle deletion"
```

---

## Task 8: Apply Store Deltas Only After Transaction Confirmation

**Files:**
- Modify: `green-order-cardano-agent/src/account_index.rs`
- Modify: `green-order-cardano-agent/src/account_events.rs`
- Modify: `bloom-offchain-cardano/src/execution_engine/execution_state.rs`
- Test: `green-order-cardano-agent/src/account_index.rs`

**Step 1: Write failing rollback tests**

Add tests:

```rust
#[test]
fn pending_store_delta_is_not_committed_before_confirmed_output() {
    let mut index = AccountIndex::with_account_and_store(...);
    let old_root = index.current_store_root(account_id).unwrap();
    let predicted_store = store_with_inserted_remaining_intent();

    let snapshot_id =
        index.reserve_predicted_store_snapshot(account_id, old_ref, predicted_store);
    index.attach_predicted_output_ref(snapshot_id, predicted_ref).unwrap();

    assert_eq!(index.current_store_root(account_id), Some(old_root));
}

#[test]
fn confirmed_predicted_output_binds_pending_snapshot() {
    let mut index = AccountIndex::with_account_and_store(...);
    let predicted_store = store_with_inserted_remaining_intent();
    let predicted_root = predicted_store.root();

    let snapshot_id =
        index.reserve_predicted_store_snapshot(account_id, old_ref, predicted_store);
    index.attach_predicted_output_ref(snapshot_id, predicted_ref).unwrap();
    index.bind_confirmed_predicted_store(predicted_ref, account_utxo_at(predicted_ref)).unwrap();

    assert_eq!(index.current_store_root(account_id), Some(predicted_root));
}

#[test]
fn rollback_restores_previous_store_snapshot_and_drops_pending_snapshot() {
    let mut index = AccountIndex::with_account_and_store(...);
    let old_root = index.current_store_root(account_id).unwrap();
    let predicted_store = store_with_inserted_remaining_intent();

    let snapshot_id =
        index.reserve_predicted_store_snapshot(account_id, old_ref, predicted_store);
    index.attach_predicted_output_ref(snapshot_id, predicted_ref).unwrap();
    index.observe_rollback_consumed(old_ref);

    assert_eq!(index.current_store_root(account_id), Some(old_root));
    assert!(index.pending_snapshot(snapshot_id).is_none());
}
```

**Step 2: Run failing tests**

Run:

```bash
cargo test -p green-order-cardano-agent account_index::tests::pending_store_delta -- --nocapture
cargo test -p green-order-cardano-agent account_index::tests::rollback_restores_previous_store_snapshot -- --nocapture
```

Expected: fail.

**Step 3: Implement pending delta lifecycle**

Extend `AccountIndex` with:

```rust
pending_store_by_snapshot_id: HashMap<StoreSnapshotId, (AccountId, OutputRef, AccountStore)>,
pending_snapshot_by_output_ref: HashMap<OutputRef, StoreSnapshotId>,
pending_store_by_output_ref: HashMap<OutputRef, (AccountId, AccountStore)>,
rollback_store_by_spent_ref: HashMap<OutputRef, (AccountId, AccountStore)>,
```

Add explicit handoff methods:

```rust
pub fn reserve_predicted_store_snapshot(
    &mut self,
    account_id: AccountId,
    old_ref: OutputRef,
    predicted_store: AccountStore,
) -> StoreSnapshotId;

pub fn attach_predicted_output_ref(
    &mut self,
    snapshot_id: StoreSnapshotId,
    predicted_output_ref: OutputRef,
) -> Result<(), AccountIndexError>;

pub fn bind_confirmed_predicted_store(
    &mut self,
    predicted_output_ref: OutputRef,
    account: AlephAccountUtxo,
) -> Result<(), AccountIndexError>;
```

Add tests that:

- reserve a `StoreSnapshotId` during planning,
- attach it to the predicted account output ref after transaction output indexing is known,
- bind only the matching pending snapshot when that output is observed,
- reject a produced output ref that has no pending snapshot,
- drop pending snapshots on failed submission/restart unless they have been confirmed,
- restore the previous committed snapshot on rollback.

Rules:

- During transaction planning, create `StoreSnapshotId -> cloned predicted snapshot` without changing the committed account store.
- When the transaction builder knows the predicted account output ref, register `StoreSnapshotId -> OutputRef`.
- Transaction execution must never mutate the committed store directly; it may only create a pending predicted snapshot from a clone and return its `StoreSnapshotId`.
- On consumed old account output, remove current account and store.
- On observed created predicted output, bind only the matching predicted store snapshot to the new account UTxO.
- On rollback, restore old UTxO and old store snapshot.
- On restart, load only confirmed snapshots from persistence. Pending snapshots that were not confirmed by observed ledger outputs must be discarded or rebuilt from resubmitted transactions.
- On unrecognized non-empty root, do not create fake store contents.

**Step 4: Verify**

Run:

```bash
cargo test -p green-order-cardano-agent account_index account_events -- --nocapture
cargo check -p green-order-cardano-agent
```

Expected: pass.

**Step 5: Commit**

```bash
git add green-order-cardano-agent/src/account_index.rs green-order-cardano-agent/src/account_events.rs bloom-offchain-cardano/src/execution_engine/execution_state.rs
git commit -m "feat: confirm green merkle deltas with account utxos"
```

---

## Task 9: Persist Account Store Snapshots

**Files:**
- Create: `green-order-cardano-agent/src/store_persistence.rs`
- Modify: `green-order-cardano-agent/src/account_store.rs`
- Modify: `green-order-cardano-agent/src/main.rs`
- Modify: `green-order-cardano-agent/src/config.rs`
- Test: `green-order-cardano-agent/src/store_persistence.rs`

**Step 1: Write failing persistence tests**

Create tests for:

- save/load account store snapshot,
- root mismatch rejects load,
- completed leaves persist,
- pending deltas are not marked confirmed after restart.

**Step 2: Run failing tests**

Run:

```bash
cargo test -p green-order-cardano-agent store_persistence -- --nocapture
```

Expected: fail.

**Step 3: Implement persistence**

Use the existing configured `dbPath` and persist:

```text
account_id -> current_output_ref
account_id -> current_store_root
account_id -> leaves: key, intent, digest, status
account_id -> confirmed slot/hash watermark
```

On startup:

1. Load snapshot.
2. Wait for ledger sync to observe the account UTxO.
3. Bind snapshot only if observed datum `store_root` equals persisted root.
4. If roots differ, mark account unexecutable and require rescan/rebuild.

**Step 4: Verify**

Run:

```bash
cargo test -p green-order-cardano-agent store_persistence account_index -- --nocapture
cargo check -p green-order-cardano-agent
```

Expected: pass.

**Step 5: Commit**

```bash
git add green-order-cardano-agent/src/store_persistence.rs green-order-cardano-agent/src/account_store.rs green-order-cardano-agent/src/main.rs green-order-cardano-agent/src/config.rs
git commit -m "feat: persist green merkle account stores"
```

---

## Task 10: Enable Partial Execution In Liquidity Book Safely

**Files:**
- Modify: `bloom-offchain-cardano/src/orders/green.rs`
- Modify: `bloom-offchain-cardano/src/execution_engine/instances.rs`
- Modify: `green-order-cardano-agent/src/config.rs`
- Modify: `green-order-cardano-agent/resources/preprod.config.json`
- Test: `bloom-offchain-cardano/src/orders/green.rs`
- Test: `green-order-cardano-agent/src/config.rs`

**Step 1: Write failing config tests**

Add tests:

```rust
#[test]
fn partial_requires_merkle_store_enabled() {
    let cfg = GreenOrdersConfig {
        allow_partial: true,
        merkle_store: MerkleStoreConfig::Disabled,
        ..Default::default()
    };
    assert!(cfg.validate().is_err());
}
```

**Step 2: Run failing test**

Run:

```bash
cargo test -p green-order-cardano-agent config::tests::partial_requires_merkle_store_enabled -- --nocapture
```

Expected: fail.

**Step 3: Implement config gating**

Add config:

```json
{
  "greenOrders": {
    "allowPartial": true,
    "merkleStore": {
      "enabled": true,
      "completePathFullFillLocally": true
    }
  }
}
```

Rules:

- `allowPartial=true` requires local MPF snapshot persistence.
- `completePathFullFillLocally=true` is the only supported full-fill path mode while Aleph remains unchanged.
- Default remains conservative: `allowPartial=false`.

**Step 4: Verify**

Run:

```bash
cargo test -p green-order-cardano-agent config -- --nocapture
cargo test -p bloom-offchain-cardano green -- --nocapture
cargo check -p green-order-cardano-agent
```

Expected: pass.

**Step 5: Commit**

```bash
git add green-order-cardano-agent/src/config.rs green-order-cardano-agent/resources/preprod.config.json bloom-offchain-cardano/src/orders/green.rs bloom-offchain-cardano/src/execution_engine/instances.rs
git commit -m "feat: gate green partial execution on merkle store"
```

---

## Final Verification

Run all of these before claiming completion:

```bash
cargo fmt
cargo test -p bloom-offchain-cardano green -- --nocapture
cargo test -p green-order-cardano-agent -- --nocapture
cargo check -p green-order-cardano-agent
cd /Users/aleksandr/RustroverProjects/aleph
aiken check
```

`aiken check` is read-only verification here. Do not modify Aleph source files or deployment artifacts as part of this plan.

## Non-Negotiable Decision

Aleph validators are not changed by this project. Keep the root unchanged on `Path` full-fill and persist local completed markers so the offchain scanner does not re-emit completed leaves.

Do not implement offchain deletion. The current validator would not enforce deletion, and changing the offchain root locally would make the local MPF snapshot diverge from the on-chain account datum root.
