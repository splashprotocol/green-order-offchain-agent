# Aiken-Compatible Rust MPF Migration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the current experimental green-order MPF logic with a stable Rust implementation that is byte-compatible with Aiken `merkle-patricia-forestry` roots, proofs, and PlutusData encoding.

**Architecture:** Build a small internal `green-order-cardano-agent::mpf` module that implements the Aiken-compatible root transition and proof-generation behavior needed by the green-order account store. Validate compatibility first with published Aiken golden fixtures, then make `AccountStore` depend on that module instead of embedding MPF hashing/proof code directly. Do not modify `/Users/aleksandr/RustroverProjects/aleph` or copy upstream MPL source code verbatim; use upstream behavior, docs, and fixtures as the compatibility oracle.

**Tech Stack:** Rust 1.84 stable, `green-order-cardano-agent`, `bloom-offchain-cardano`, CML PlutusData/CBOR encoding, `cml_crypto::blake2b256`, Aiken `merkle-patricia-forestry` behavior, `cargo test`.

---

## Source Facts And Constraints

- Upstream Aiken MPF repo: `https://github.com/aiken-lang/merkle-patricia-forestry`.
- Upstream docs define `Proof = List<ProofStep>` and proof steps `Branch { skip, neighbors }`, `Fork { skip, neighbor }`, and `Leaf { skip, key, value }`.
- Upstream docs state that on-chain Aiken works from `root + proof`; full proof generation is performed by the off-chain package.
- Published Aiken fixture:
  - old root: `225a4599b804ba53745538c83bfa699ecf8077201b61484c91171f5910a4a8f9`
  - key: `0000000000000000000261a131bf48cc5a19658ade8cfede99dc1c3933300d60`
  - value: `26f711634eb26999169bb927f629870938bb4b6b4d1a078b44a6b4ec54f9e8df`
  - expected root after insert: `507c03bc4a25fd1cac2b03592befa4225c5f3488022affa0ab059ca350de2353`
- The current `mutree` probe proves `mutree` is not a drop-in replacement:
  - it requires nightly because `mutree 0.1.0` uses `#![feature(coverage_attribute)]`
  - it reconstructs the Aiken fixture old root as `c1418f5c...`, not the published `225a4599...`
- Current local code in `green-order-cardano-agent/src/account_store.rs` has an ignored Aiken fixture test. That test must become a normal passing test before any production wiring is considered complete.
- Aleph source under `/Users/aleksandr/RustroverProjects/aleph` is read-only for this work.
- Do not import `green-order-cardano-agent` from `bloom-offchain-cardano`; keep the dependency direction unchanged.
- All implementation commands in this plan must run with `cwd=/Users/aleksandr/IdeaProjects/green-order-offchain-agent` unless a step explicitly says otherwise.
- Any Aleph reference must be read-only. Do not run commands with `/Users/aleksandr/RustroverProjects/aleph` as `cwd`, do not format it, do not generate files into it, and do not modify source under that tree.
- The sole allowed Aleph command is the read-only cleanliness check `git -C /Users/aleksandr/RustroverProjects/aleph status --short`, run from the green-order repo or any other non-Aleph cwd.
- Upstream `aiken-lang/merkle-patricia-forestry` is MPL-2.0. Literal code copying or line-by-line translation requires explicit approval and attribution review. Default implementation mode is clean-room behavior matching from public docs, published fixtures, and generated fixture outputs.

## Compatibility Contract

The Rust MPF module must expose and test these operations:

```rust
pub const EMPTY_ROOT: [u8; 32] = [0; 32];

pub struct MpfRoot([u8; 32]);

pub enum ProofStep {
    Branch { skip: usize, neighbors: Vec<u8> },
    Fork { skip: usize, neighbor: Neighbor },
    Leaf { skip: usize, key: [u8; 32], value: [u8; 32] },
}

pub struct Neighbor {
    pub nibble: u8,
    pub prefix: Vec<u8>,
    pub root: [u8; 32],
}

pub fn has(root: [u8; 32], key: &[u8], value: &[u8], proof: &[ProofStep]) -> bool;
pub fn miss(root: [u8; 32], key: &[u8], proof: &[ProofStep]) -> bool;
pub fn insert(root: [u8; 32], key: &[u8], value: &[u8], proof: &[ProofStep]) -> Result<[u8; 32], MpfError>;
pub fn update(root: [u8; 32], key: &[u8], proof: &[ProofStep], old_value: &[u8], new_value: &[u8]) -> Result<[u8; 32], MpfError>;
pub fn delete(root: [u8; 32], key: &[u8], value: &[u8], proof: &[ProofStep]) -> Result<[u8; 32], MpfError>;
```

Use the same convention as Aiken:

```text
path = blake2b_256(key)
value_hash = blake2b_256(value)
```

For green orders, `value` is the 32-byte `AlephIntention::digest()`; therefore the MPF leaf value hash is `blake2b_256(intent_digest)`.

Proof payload semantics to implement:

- Public APIs accept raw external `key` and raw external `value`.
- Internal path is always `blake2b_256(key)`.
- Internal value hash is always `blake2b_256(value)`.
- Aiken `Leaf { key, value }` in a proof stores the already-hashed terminal `path` and `value_hash`, not the raw external key/value.
- `Branch.neighbors` must be exactly 128 bytes: `[neighbor_8, neighbor_4, neighbor_2, neighbor_1]`, each 32 bytes.
- `skip` is a nibble count. Every recursive step must reject cursor movement beyond 64 nibbles.
- `Fork.neighbor.root` stores the authenticated root of the neighbor subtree, `neighbor.nibble` stores the branch nibble, and `neighbor.prefix` stores the compressed nibble prefix in the exact byte format pinned by upstream fixtures in Task 0.
- A task is not complete if it passes only self-generated Rust fixtures. Every proof shape introduced by the Rust implementation must be covered by upstream-generated fixture data first.

---

### Task 0: Freeze Upstream Semantics And Fixture Corpus

**Files:**
- Create: `green-order-cardano-agent/resources/mpf-fixtures/upstream/README.md`
- Create: `green-order-cardano-agent/resources/mpf-fixtures/upstream/bitcoin-845602.json`
- Create: `green-order-cardano-agent/resources/mpf-fixtures/upstream/small-branch.json`
- Create: `green-order-cardano-agent/resources/mpf-fixtures/upstream/leaf-exclusion.json`
- Create: `green-order-cardano-agent/resources/mpf-fixtures/upstream/fork-compression.json`
- Create: `green-order-cardano-agent/resources/mpf-fixtures/upstream/delete-collapse.json`
- Create: `green-order-cardano-agent/resources/mpf-fixtures/upstream/negative-cases.json`

**Step 1: Record licensing and source boundary**

Create `README.md`:

```markdown
# Upstream MPF Fixtures

These fixtures are generated from public behavior of `aiken-lang/merkle-patricia-forestry`.
They are test inputs/outputs only. Do not copy upstream MPL-2.0 source code into this repository
without explicit approval and attribution review.

Aleph source under `/Users/aleksandr/RustroverProjects/aleph` is read-only for this work.
No fixture generation may write into that directory.
```

**Step 2: Generate or collect upstream fixtures outside Aleph**

Use the upstream JS/off-chain package only as a dev/test oracle. Acceptable options:

```bash
mkdir -p /private/tmp/mpf-oracle
```

Then either:

```bash
npm pack @aiken-lang/merkle-patricia-forestry
```

or clone the upstream repository into `/private/tmp/mpf-oracle`.

Do not commit `node_modules`, package tarballs, cloned upstream code, generated scripts copied from upstream, or any Aleph artifacts. Commit only JSON fixtures.

**Step 3: Fixture requirements**

The fixture set must include:

- Published Bitcoin insert fixture from Aiken docs.
- Empty trie insert.
- Second insert into singleton trie.
- Inclusion proof for existing key.
- Exclusion proof using `Leaf`.
- Exclusion or update proof using `Fork`.
- Update existing value.
- Delete one of two leaves.
- Delete singleton to empty.
- Delete causing branch-to-leaf collapse.
- Multi-level shared-prefix branch/fork case.
- Negative cases:
  - wrong old root
  - wrong key
  - wrong value
  - duplicate insert
  - missing update
  - missing delete
  - malformed branch neighbor length
  - malformed root length at raw fixture/CBOR decoding boundary
  - malformed skip that moves cursor beyond 64 nibbles
  - malformed fork prefix/root length at raw fixture/CBOR decoding boundary

Each positive fixture must include:

```json
{
  "name": "case-name",
  "operation": "insert|has|miss|update|delete",
  "old_root": "...",
  "key": "...",
  "old_value": "...",
  "new_value": "...",
  "proof": [],
  "new_root": "..."
}
```

Each proof step must explicitly identify that `Leaf.key` and `Leaf.value` fields are hashed path/value hash. If upstream output cannot expose this directly, document the derivation in the fixture README and pin the serialized proof CBOR from upstream.

Malformed length fixtures must be tested through raw fixture/CBOR decoding helpers, not typed APIs. The typed public API uses `[u8; 32]` for roots and fork roots, so invalid lengths cannot be represented after successful decoding.

**Step 4: Verify fixture provenance**

Run only outside Aleph:

```bash
git -C /Users/aleksandr/RustroverProjects/aleph status --short
```

Expected: no output. This read-only command is the only allowed Aleph command in this plan.

**Step 5: Commit**

```bash
git add green-order-cardano-agent/resources/mpf-fixtures/upstream
git commit -m "test: add upstream mpf compatibility fixtures"
```

### Task 1: Remove mutree From The Migration Path

**Files:**
- Modify: `green-order-cardano-agent/Cargo.toml`
- Modify: `green-order-cardano-agent/src/account_store.rs`

**Step 1: Write the cleanup patch**

Remove:

```toml
[features]
mutree-compat = ["dep:mutree"]

mutree = { version = "0.1.0", features = ["blake2"], optional = true }
```

Remove the ignored `mutree_bitcoin_insert_fixture_matches_published_aiken_roots` test from `account_store.rs`.

Do not delete the ignored `aiken_mpf_bitcoin_insert_fixture_matches_published_roots` test yet. Task 3 moves it into `mpf.rs` and makes it non-ignored.

Keep `blake2 = "0.10.6"` only if later tasks use it directly. Prefer `cml_crypto::blake2b256` for Cardano consistency.

**Step 2: Verify stable build is unaffected**

Run:

```bash
cargo test -p green-order-cardano-agent mutree -- --nocapture
```

Expected: `0` matching tests and no mutree compile.

**Step 3: Commit**

```bash
git add green-order-cardano-agent/Cargo.toml green-order-cardano-agent/src/account_store.rs
git commit -m "chore: remove mutree compatibility probe"
```

---

### Task 2: Create The Internal MPF Module Skeleton

**Files:**
- Create: `green-order-cardano-agent/src/mpf.rs`
- Modify: `green-order-cardano-agent/src/main.rs`
- Test: `green-order-cardano-agent/src/mpf.rs`

**Step 1: Write failing type-level tests**

Add:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_root_is_aiken_empty_root() {
        assert_eq!(EMPTY_ROOT, [0u8; 32]);
    }

    #[test]
    fn rejects_malformed_branch_neighbor_lengths() {
        let proof = vec![ProofStep::Branch {
            skip: 0,
            neighbors: vec![0; 127],
        }];
        assert!(matches!(
            insert(EMPTY_ROOT, b"k", b"v", &proof),
            Err(MpfError::InvalidBranchNeighbors { .. })
        ));
    }
}
```

**Step 2: Run tests and verify failure**

Run:

```bash
cargo test -p green-order-cardano-agent mpf::tests::empty_root_is_aiken_empty_root -- --nocapture
cargo test -p green-order-cardano-agent mpf::tests::rejects_malformed_branch_neighbor_lengths -- --nocapture
```

Expected: fail because `mpf` module does not exist.

**Step 3: Implement module skeleton**

Implement:

```rust
pub const EMPTY_ROOT: [u8; 32] = [0; 32];

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ProofStep {
    Branch { skip: usize, neighbors: Vec<u8> },
    Fork { skip: usize, neighbor: Neighbor },
    Leaf { skip: usize, key: [u8; 32], value: [u8; 32] },
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Neighbor {
    pub nibble: u8,
    pub prefix: Vec<u8>,
    pub root: [u8; 32],
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum MpfError {
    InvalidBranchNeighbors { actual: usize },
    InvalidProof,
    ExistingKey,
    MissingKey,
    PathExhausted,
}
```

Add placeholder functions that validate branch neighbor length, fork root length, and skip bounds where possible. Return `Err(MpfError::InvalidProof)` until later tasks fill behavior.

Add a raw fixture/CBOR decoding boundary for test inputs:

```rust
#[cfg(test)]
fn proof_from_fixture_json(raw: &serde_json::Value) -> Result<Vec<ProofStep>, MpfError>;

#[cfg(test)]
fn root_from_fixture_hex(raw: &str) -> Result<[u8; 32], MpfError>;
```

Malformed root length and malformed fork root length tests must target these helpers, because the typed API cannot represent those invalid states.

Modify `main.rs`:

```rust
mod mpf;
```

**Step 4: Verify**

Run:

```bash
cargo test -p green-order-cardano-agent mpf::tests:: -- --nocapture
```

Expected: pass the skeleton tests.

**Step 5: Commit**

```bash
git add green-order-cardano-agent/src/main.rs green-order-cardano-agent/src/mpf.rs
git commit -m "feat: add aiken-compatible mpf module skeleton"
```

---

### Task 3: Port Aiken Root Transition Verification Before Full Trie Generation

**Files:**
- Modify: `green-order-cardano-agent/src/mpf.rs`
- Modify: `green-order-cardano-agent/src/account_store.rs`

**Step 1: Move the Bitcoin fixture into `mpf.rs` as a normal failing test**

Move the current ignored fixture from `account_store.rs` into `mpf.rs` and unignore it:

```rust
#[test]
fn bitcoin_845602_insert_matches_aiken_fixture() {
    let old_root = hex32("225a4599b804ba53745538c83bfa699ecf8077201b61484c91171f5910a4a8f9");
    let new_root = hex32("507c03bc4a25fd1cac2b03592befa4225c5f3488022affa0ab059ca350de2353");
    let key = hex::decode("0000000000000000000261a131bf48cc5a19658ade8cfede99dc1c3933300d60").unwrap();
    let value = hex::decode("26f711634eb26999169bb927f629870938bb4b6b4d1a078b44a6b4ec54f9e8df").unwrap();
    let proof = proof_bitcoin_845602();

    assert!(miss(old_root, &key, &proof));
    assert_eq!(insert(old_root, &key, &value, &proof).unwrap(), new_root);
}
```

Keep the five published `Branch` proof steps exactly as in the Aiken docs and the existing `account_store.rs` ignored test. This test is authoritative only for the published branch-only insert case; other proof shapes must come from Task 0 upstream fixtures.

**Step 2: Run test and verify failure**

Run:

```bash
cargo test -p green-order-cardano-agent mpf::tests::bitcoin_845602_insert_matches_aiken_fixture -- --nocapture
```

Expected: fail until root transition logic matches Aiken.

**Step 3: Implement Aiken-compatible transition functions**

Implement these private helpers in `mpf.rs`:

```rust
fn hash(bytes: impl AsRef<[u8]>) -> [u8; 32] {
    cml_crypto::blake2b256(bytes.as_ref())
}

fn combine(left: &[u8], right: &[u8]) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(left.len() + right.len());
    bytes.extend_from_slice(left);
    bytes.extend_from_slice(right);
    hash(bytes)
}

fn nibble(path: &[u8; 32], index: usize) -> Result<usize, MpfError>;
fn nibbles(path: &[u8; 32], start: usize, end: usize) -> Result<Vec<u8>, MpfError>;
fn suffix(path: &[u8; 32], cursor: usize) -> Result<Vec<u8>, MpfError>;
fn merkle_16(branch: usize, root: &[u8; 32], neighbors: &[u8]) -> Result<[u8; 32], MpfError>;
fn branch_root(path: &[u8; 32], cursor: usize, next_cursor: usize, child_root: &[u8; 32], neighbors: &[u8]) -> Result<[u8; 32], MpfError>;
```

Implement root transitions recursively:

```rust
fn including(path: &[u8; 32], value_hash: &[u8; 32], cursor: usize, proof: &[ProofStep]) -> Result<[u8; 32], MpfError>;
fn excluding(path: &[u8; 32], cursor: usize, proof: &[ProofStep]) -> Result<[u8; 32], MpfError>;
fn membership(path: &[u8; 32], value_hash: &[u8; 32], cursor: usize, proof: &[ProofStep]) -> Result<[u8; 32], MpfError>;
```

Be explicit about value hashing:

```rust
pub fn insert(root: [u8; 32], key: &[u8], value: &[u8], proof: &[ProofStep]) -> Result<[u8; 32], MpfError> {
    let path = hash(key);
    let value_hash = hash(value);
    let old = excluding(&path, 0, proof)?;
    if old != root {
        return Err(MpfError::InvalidProof);
    }
    including(&path, &value_hash, 0, proof)
}
```

**Step 4: Verify**

Run:

```bash
cargo test -p green-order-cardano-agent mpf::tests::bitcoin_845602_insert_matches_aiken_fixture -- --nocapture
```

Expected: pass.

**Step 5: Remove the old ignored account-store Aiken helper test**

Delete duplicated `aiken_mpf_bitcoin_insert_fixture_matches_published_roots` and local helper copies from `account_store.rs`.

**Step 6: Commit**

```bash
git add green-order-cardano-agent/src/mpf.rs green-order-cardano-agent/src/account_store.rs
git commit -m "feat: match aiken mpf insert fixture"
```

---

### Task 4: Add Proof PlutusData Encoding In One Place

**Files:**
- Modify: `green-order-cardano-agent/src/mpf.rs`
- Modify: `green-order-cardano-agent/src/account_store.rs`
- Test: `green-order-cardano-agent/src/mpf.rs`

**Step 1: Write failing encoding tests**

Add:

```rust
#[test]
fn branch_proof_encodes_as_aiken_constr_zero() {
    let proof = vec![ProofStep::Branch {
        skip: 0,
        neighbors: vec![1; 128],
    }];

    let cbor = proof_to_cbor(&proof);
    assert_eq!(hex::encode(cbor), "REPLACE_WITH_UPSTREAM_AIKEN_CBOR");
}
```

Generate expected CBOR from Task 0 upstream fixtures, not from the current local encoder. Add separate CBOR tests for `Branch`, `Fork`, `Leaf`, and whole proof lists.

**Step 2: Run test and verify failure**

Run:

```bash
cargo test -p green-order-cardano-agent mpf::tests::branch_proof_encodes_as_aiken_constr_zero -- --nocapture
```

Expected: fail until encoder is implemented in `mpf.rs`.

**Step 3: Move encoder logic**

Move these functions from `account_store.rs` into `mpf.rs`:

```rust
pub fn proof_to_plutus_data(proof: &[ProofStep]) -> PlutusData;
pub fn proof_to_cbor(proof: &[ProofStep]) -> Vec<u8>;
fn step_to_plutus_data(step: &ProofStep) -> PlutusData;
fn aiken_constr(alternative: u64, fields: Vec<PlutusData>) -> PlutusData;
```

Map alternatives:

```text
Branch -> 0
Fork   -> 1
Leaf   -> 2
```

Encode `Neighbor` as the same constructor shape expected by Aiken.

**Step 4: Verify**

Run:

```bash
cargo test -p green-order-cardano-agent mpf::tests::branch_proof_encodes_as_aiken_constr_zero -- --nocapture
cargo test -p green-order-cardano-agent mpf::tests::bitcoin_845602_insert_matches_aiken_fixture -- --nocapture
```

Expected: pass.

**Step 5: Commit**

```bash
git add green-order-cardano-agent/src/mpf.rs green-order-cardano-agent/src/account_store.rs
git commit -m "feat: centralize aiken mpf proof encoding"
```

---

### Task 5: Implement Full Trie Root And Exclusion Proof Generation

**Files:**
- Modify: `green-order-cardano-agent/src/mpf.rs`
- Test: `green-order-cardano-agent/src/mpf.rs`

**Step 1: Write failing tests for generated proofs**

Add:

```rust
#[test]
fn empty_trie_insert_generates_valid_exclusion_proof() {
    let mut trie = Trie::empty();
    let proof = trie.proof_for_missing(b"k").unwrap();
    let new_root = insert(trie.root(), b"k", b"v", &proof).unwrap();

    trie.insert(b"k".to_vec(), b"v".to_vec()).unwrap();
    assert_eq!(trie.root(), new_root);
}

#[test]
fn second_insert_generated_proof_verifies_against_current_root() {
    let mut trie = Trie::empty();
    trie.insert(b"k1".to_vec(), b"v1".to_vec()).unwrap();

    let proof = trie.proof_for_missing(b"k2").unwrap();
    let new_root = insert(trie.root(), b"k2", b"v2", &proof).unwrap();

    trie.insert(b"k2".to_vec(), b"v2".to_vec()).unwrap();
    assert_eq!(trie.root(), new_root);
}
```

Also add fixture-driven tests from Task 0:

```rust
#[test]
fn upstream_leaf_exclusion_fixture_passes() {
    let fixture = load_upstream_fixture("leaf-exclusion.json");
    assert_fixture_passes(fixture);
}

#[test]
fn upstream_fork_compression_fixture_passes() {
    let fixture = load_upstream_fixture("fork-compression.json");
    assert_fixture_passes(fixture);
}
```

**Step 2: Run tests and verify failure**

Run:

```bash
cargo test -p green-order-cardano-agent mpf::tests::empty_trie_insert_generates_valid_exclusion_proof -- --nocapture
cargo test -p green-order-cardano-agent mpf::tests::second_insert_generated_proof_verifies_against_current_root -- --nocapture
```

Expected: fail because `Trie` does not exist.

**Step 3: Implement `Trie`**

Implement:

```rust
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Trie {
    leaves: BTreeMap<Vec<u8>, Vec<u8>>,
}

impl Trie {
    pub fn empty() -> Self;
    pub fn from_leaves(leaves: BTreeMap<Vec<u8>, Vec<u8>>) -> Self;
    pub fn root(&self) -> [u8; 32];
    pub fn insert(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<[u8; 32], MpfError>;
    pub fn update(&mut self, key: &[u8], value: Vec<u8>) -> Result<[u8; 32], MpfError>;
    pub fn delete(&mut self, key: &[u8]) -> Result<[u8; 32], MpfError>;
    pub fn proof_for_missing(&self, key: &[u8]) -> Result<Vec<ProofStep>, MpfError>;
    pub fn proof_for_present(&self, key: &[u8]) -> Result<Vec<ProofStep>, MpfError>;
}
```

Internally use:

```rust
struct Leaf {
    key: Vec<u8>,
    path: [u8; 32],
    value: Vec<u8>,
    value_hash: [u8; 32],
}
```

The root generator must use the same root-shape helpers that passed the Aiken fixture. Do not keep a separate hash implementation.

Generated Rust proofs must match upstream fixture roots and proof CBOR for equivalent datasets. If generated proof shape differs but root transition is valid, stop and inspect; do not accept alternate proof shapes until the on-chain Aiken verifier accepts their CBOR in an upstream-oracle test.

**Step 4: Verify**

Run:

```bash
cargo test -p green-order-cardano-agent mpf::tests::empty_trie_insert_generates_valid_exclusion_proof -- --nocapture
cargo test -p green-order-cardano-agent mpf::tests::second_insert_generated_proof_verifies_against_current_root -- --nocapture
cargo test -p green-order-cardano-agent mpf::tests::bitcoin_845602_insert_matches_aiken_fixture -- --nocapture
```

Expected: pass.

**Step 5: Commit**

```bash
git add green-order-cardano-agent/src/mpf.rs
git commit -m "feat: generate aiken-compatible mpf roots and exclusion proofs"
```

---

### Task 6: Implement Inclusion, Update, Delete, Fork, And Leaf Proof Cases

**Files:**
- Modify: `green-order-cardano-agent/src/mpf.rs`
- Test: `green-order-cardano-agent/src/mpf.rs`

**Step 1: Write failing tests**

Add tests:

```rust
#[test]
fn generated_inclusion_proof_validates_existing_leaf() {
    let mut trie = Trie::empty();
    trie.insert(b"k1".to_vec(), b"v1".to_vec()).unwrap();

    let proof = trie.proof_for_present(b"k1").unwrap();

    assert!(has(trie.root(), b"k1", b"v1", &proof));
}

#[test]
fn generated_update_proof_transitions_root() {
    let mut trie = Trie::empty();
    trie.insert(b"k1".to_vec(), b"v1".to_vec()).unwrap();
    let old_root = trie.root();
    let proof = trie.proof_for_present(b"k1").unwrap();

    let expected = update(old_root, b"k1", &proof, b"v1", b"v2").unwrap();
    trie.update(b"k1", b"v2".to_vec()).unwrap();

    assert_eq!(trie.root(), expected);
}

#[test]
fn generated_delete_proof_transitions_root() {
    let mut trie = Trie::empty();
    trie.insert(b"k1".to_vec(), b"v1".to_vec()).unwrap();
    trie.insert(b"k2".to_vec(), b"v2".to_vec()).unwrap();
    let old_root = trie.root();
    let proof = trie.proof_for_present(b"k1").unwrap();

    let expected = delete(old_root, b"k1", b"v1", &proof).unwrap();
    trie.delete(b"k1").unwrap();

    assert_eq!(trie.root(), expected);
}
```

Add explicit collision-shape tests with two keys whose hashed paths share several nibbles. Use deterministic byte keys found by a small local helper test, then hard-code them.

Add required negative tests:

```rust
#[test]
fn insert_rejects_wrong_old_root() { /* fixture proof, mutated old root */ }

#[test]
fn insert_rejects_duplicate_key() { /* inclusion proof supplied to insert */ }

#[test]
fn has_rejects_wrong_value() { /* inclusion proof, wrong value */ }

#[test]
fn update_rejects_missing_key() { /* exclusion proof supplied to update */ }

#[test]
fn delete_rejects_missing_key() { /* exclusion proof supplied to delete */ }

#[test]
fn delete_singleton_returns_empty_root() { /* upstream fixture */ }

#[test]
fn delete_branch_to_leaf_collapse_matches_upstream() { /* upstream fixture */ }

#[test]
fn malformed_skip_past_path_end_is_rejected() { /* cursor > 64 */ }

#[test]
fn malformed_fork_prefix_is_rejected() { /* invalid prefix/root shape */ }

#[test]
fn fixture_decoder_rejects_malformed_root_length() { /* raw JSON root not 32 bytes */ }

#[test]
fn fixture_decoder_rejects_malformed_fork_root_length() { /* raw JSON fork root not 32 bytes */ }
```

**Step 2: Run tests and verify failure**

Run:

```bash
cargo test -p green-order-cardano-agent mpf::tests::generated_inclusion_proof_validates_existing_leaf -- --nocapture
cargo test -p green-order-cardano-agent mpf::tests::generated_update_proof_transitions_root -- --nocapture
cargo test -p green-order-cardano-agent mpf::tests::generated_delete_proof_transitions_root -- --nocapture
```

Expected: fail until inclusion/update/delete and compressed proof cases work.

**Step 3: Implement the missing cases**

Implement:
- `ProofStep::Leaf` handling for exclusion proofs where the searched path diverges from a single existing leaf. The leaf payload is `key = existing hashed path` and `value = existing hashed value`.
- `ProofStep::Fork` handling for compressed non-leaf neighbor cases.
- `has`, `update`, and `delete`.
- Branch compression after delete, including transition from branch to leaf when only one child remains.

Use one shared implementation for recursive root calculation so generated roots and proof-verified roots cannot drift.

**Step 4: Verify**

Run:

```bash
cargo test -p green-order-cardano-agent mpf::tests:: -- --nocapture
```

Expected: all `mpf` tests pass.

**Step 5: Commit**

```bash
git add green-order-cardano-agent/src/mpf.rs
git commit -m "feat: support aiken mpf inclusion update and delete"
```

---

### Task 7: Replace AccountStore Embedded MPF Logic

**Files:**
- Modify: `green-order-cardano-agent/src/account_store.rs`
- Test: `green-order-cardano-agent/src/account_store.rs`

**Step 1: Write failing account-store compatibility tests**

Add or update tests:

```rust
#[test]
fn account_store_root_matches_mpf_trie_for_pending_and_completed_leaves() {
    let mut store = AccountStore::empty();
    store.insert_remaining(vec![1, 2, 3], order_id(1), intent(1_000)).unwrap();
    store.insert_remaining(vec![4, 5, 6], order_id(2), intent(2_000)).unwrap();

    let mut trie = crate::mpf::Trie::empty();
    for leaf in store.snapshot_leaves() {
        trie.insert(leaf.key, leaf.digest.to_vec()).unwrap();
    }

    assert_eq!(store.root(), trie.root());
}
```

Add tests for:
- `insert_remaining` uses exclusion proof accepted by `mpf::insert`
- `update_remaining` uses inclusion proof accepted by `mpf::update`
- `mark_completed` preserves root and returns inclusion proof accepted by `mpf::has`
- `from_snapshot` rejects a root that does not match snapshot leaves

**Step 2: Run tests and verify failure**

Run:

```bash
cargo test -p green-order-cardano-agent account_store::tests::account_store_root_matches_mpf_trie_for_pending_and_completed_leaves -- --nocapture
```

Expected: fail until `AccountStore` delegates to `mpf::Trie`.

**Step 3: Refactor AccountStore**

Remove from `account_store.rs`:
- local `MpfLeaf`
- local `MpfProofStep`
- local hash helpers
- local `root_for`
- local `proof_for_path`
- local PlutusData proof encoder

Keep only account-specific types and mapping:

```rust
fn mpf_values(leaves: &BTreeMap<Vec<u8>, StoredIntentLeaf>) -> BTreeMap<Vec<u8>, Vec<u8>> {
    leaves
        .iter()
        .map(|(key, leaf)| (key.clone(), leaf.digest.to_vec()))
        .collect()
}
```

Use:

```rust
let trie = mpf::Trie::from_leaves(mpf_values(&self.leaves));
let proof = trie.proof_for_missing(&key)?;
let new_root = mpf::insert(self.root, &key, &digest, &proof)?;
let proof_cbor = mpf::proof_to_cbor(&proof);
```

For update:

```rust
let proof = trie.proof_for_present(&key)?;
let new_root = mpf::update(self.root, &key, &proof, &old_digest, &new_digest)?;
```

For full completion:

```rust
let proof = trie.proof_for_present(&key)?;
if !mpf::has(self.root, &key, &old_digest, &proof) {
    return Err(GreenStorePlanningError::RootMismatch { ... });
}
```

Preserve existing behavior: `mark_completed` does not delete from the MPF root because current Aleph validator checks `mpf.has(...)` for full `Path` completion and does not enforce deletion.

**Step 4: Map errors**

Add:

```rust
impl From<crate::mpf::MpfError> for GreenStorePlanningError {
    fn from(_: crate::mpf::MpfError) -> Self {
        GreenStorePlanningError::Unsupported
    }
}
```

If more specific variants already exist, map `InvalidProof` to root mismatch and malformed proof to unsupported.

**Step 5: Verify**

Run:

```bash
cargo test -p green-order-cardano-agent account_store::tests:: -- --nocapture
cargo test -p green-order-cardano-agent mpf::tests:: -- --nocapture
```

Expected: pass.

**Step 6: Commit**

```bash
git add green-order-cardano-agent/src/account_store.rs green-order-cardano-agent/src/mpf.rs
git commit -m "refactor: use aiken-compatible mpf in account store"
```

---

### Task 8: Add Golden End-To-End Store Planning Tests

**Files:**
- Modify: `green-order-cardano-agent/src/account_index.rs`
- Modify: `green-order-cardano-agent/src/account_store.rs`
- Test: `green-order-cardano-agent/src/account_index.rs`

**Step 1: Write failing tests**

Add tests that exercise the actual planning layer:

```rust
#[test]
fn partial_sig_plan_produces_insert_proof_and_new_root_verified_by_mpf() {
    // Build empty account store.
    // Plan new partially filled intent.
    // Decode returned proof through mpf test helper or compare root by calling mpf::insert.
}

#[test]
fn partial_path_plan_produces_update_proof_and_new_root_verified_by_mpf() {
    // Insert initial remaining intent.
    // Plan path partial fill.
    // Assert mpf::update(old_root, key, proof, old_digest, new_digest) == plan.new_root.
}

#[test]
fn full_path_plan_returns_inclusion_proof_and_preserves_root() {
    // Insert initial remaining intent.
    // Plan path full fill.
    // Assert mpf::has(root, key, old_digest, proof).
    // Assert returned root unchanged.
}
```

If proof CBOR decoding is not available, add `mpf::proof_from_plutus_data_for_tests` behind `#[cfg(test)]`. The decoder must be tested against upstream CBOR fixtures from Task 0, not only local encoder round trips.

**Step 2: Run tests and verify failure**

Run:

```bash
cargo test -p green-order-cardano-agent account_index::tests::partial_sig_plan_produces_insert_proof_and_new_root_verified_by_mpf -- --nocapture
cargo test -p green-order-cardano-agent account_index::tests::partial_path_plan_produces_update_proof_and_new_root_verified_by_mpf -- --nocapture
cargo test -p green-order-cardano-agent account_index::tests::full_path_plan_returns_inclusion_proof_and_preserves_root -- --nocapture
```

Expected: fail until tests and proof decoding helpers are complete.

**Step 3: Implement test helpers only if needed**

Prefer testing `AccountStore` directly. If testing `AccountIndex` requires decoding CBOR, add test-only decode helpers in `mpf.rs` and keep them under `#[cfg(test)]`.

**Step 4: Verify**

Run:

```bash
cargo test -p green-order-cardano-agent account_index::tests:: -- --nocapture
```

Expected: pass.

**Step 5: Commit**

```bash
git add green-order-cardano-agent/src/account_index.rs green-order-cardano-agent/src/account_store.rs green-order-cardano-agent/src/mpf.rs
git commit -m "test: verify green store plans with aiken-compatible mpf"
```

---

### Task 9: Add Deterministic Regression Fixtures

**Files:**
- Create: `green-order-cardano-agent/resources/mpf-fixtures/rust-regression/small-trie.json`
- Modify: `green-order-cardano-agent/src/mpf.rs`

**Step 1: Write Rust regression fixture format**

Create Rust-generated regression JSON fixtures only after all upstream fixtures pass. These fixtures are not compatibility authorities; they protect against accidental Rust behavior changes.

Use this shape:

```json
{
  "name": "bitcoin-845602",
  "old_root": "225a4599b804ba53745538c83bfa699ecf8077201b61484c91171f5910a4a8f9",
  "operation": "insert",
  "key": "0000000000000000000261a131bf48cc5a19658ade8cfede99dc1c3933300d60",
  "value": "26f711634eb26999169bb927f629870938bb4b6b4d1a078b44a6b4ec54f9e8df",
  "new_root": "507c03bc4a25fd1cac2b03592befa4225c5f3488022affa0ab059ca350de2353",
  "proof": [
    { "branch": { "skip": 0, "neighbors": "..." } }
  ]
}
```

For `rust-regression/small-trie.json`, include:
- empty insert
- second insert
- inclusion proof
- update
- delete

Generate `small-trie.json` with the Rust implementation after all Task 0 upstream fixtures pass. Cross-check any case that overlaps upstream fixtures before committing.

**Step 2: Add fixture loading tests**

Add:

```rust
#[test]
fn fixture_bitcoin_845602_passes() {
    let fixture = load_upstream_fixture("bitcoin-845602.json");
    assert_eq!(insert(fixture.old_root, &fixture.key, &fixture.value, &fixture.proof).unwrap(), fixture.new_root);
}
```

**Step 3: Run tests**

Run:

```bash
cargo test -p green-order-cardano-agent mpf::tests::upstream_fixture_ -- --nocapture
cargo test -p green-order-cardano-agent mpf::tests::rust_regression_fixture_ -- --nocapture
```

Expected: pass.

**Step 4: Commit**

```bash
git add green-order-cardano-agent/resources/mpf-fixtures green-order-cardano-agent/src/mpf.rs
git commit -m "test: add mpf regression fixtures"
```

---

### Task 10: Final Verification And Documentation

**Files:**
- Modify: `README.md`
- Modify: `docs/plans/2026-05-19-aiken-compatible-rust-mpf-migration.md`

**Step 1: Document operational invariant**

Add to `README.md`:

```markdown
### Green Order MPF Compatibility

The green-order agent maintains an Aiken-compatible Merkle Patricia Forestry mirror per Aleph account.
For partial fills it emits proofs and roots accepted by Aleph `mpf.insert` / `mpf.update`.
For full stored-intent fills it emits an inclusion proof and preserves the on-chain root, because the current Aleph validator checks `mpf.has` and does not delete the leaf.
```

**Step 2: Run full relevant test suite**

Run:

```bash
cargo fmt
cargo test -p green-order-cardano-agent -- --nocapture
cargo test -p bloom-offchain-cardano green -- --nocapture
cargo check -p green-order-cardano-agent
```

Expected: pass.

**Step 3: Check Aleph untouched**

Run:

```bash
git -C /Users/aleksandr/RustroverProjects/aleph status --short
```

Expected: no output.

**Step 4: Commit**

```bash
git add README.md docs/plans/2026-05-19-aiken-compatible-rust-mpf-migration.md
git commit -m "docs: document aiken-compatible mpf migration"
```

---

## Rollback Plan

If the Rust MPF implementation fails the Bitcoin fixture or cannot generate valid update/delete proofs:

1. Keep `AccountStore` on the existing implementation.
2. Leave the new `mpf` module behind a compile-time feature only if it does not affect stable builds.
3. Do not wire it into transaction planning.
4. Record the failing root/proof case in `green-order-cardano-agent/resources/mpf-fixtures/`.
5. Re-evaluate whether to call the upstream JS off-chain package as an external dev/test oracle or runtime proof-generation process. Runtime use requires explicit dependency, deployment, and licensing approval.

## Definition Of Done

- `mutree` is removed from the default dependency graph.
- A normal, non-ignored Rust test passes for the published Bitcoin Aiken insert fixture.
- `AccountStore` computes roots and proofs through the new `mpf` module.
- Insert, update, membership, non-membership, and delete root transitions are tested.
- Green-order planning tests prove returned proofs match returned roots.
- Aleph source remains untouched.
- Stable `cargo test -p green-order-cardano-agent -- --nocapture` passes.
- `git -C /Users/aleksandr/RustroverProjects/aleph status --short` prints no output.
