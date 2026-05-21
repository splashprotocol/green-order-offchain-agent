# Green Order Funding Bootstrap Selection Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make preprod green-order execution deterministic by ensuring operator funding UTxOs that already exist on-chain are available to the engine before matchmaking, and by preferring funding UTxOs that preserve the Aleph witness input-order workaround.

**Architecture:** The agent should not wait for a slow chain replay to learn funding UTxOs. On startup it should query the configured explorer for all current UTxOs at the four operator funding addresses, filter out funding UTxOs below the configured minimum usable value, seed the funding handler index, and prepend those events to the execution funding streams without blocking bounded channels. The execution engine should use the normal lowest-ref funding choice except for the known two-consumed-input path, where it should prefer a funding UTxO whose ref sorts between the two consumed refs and therefore preserves the Aleph witness workaround.

**Tech Stack:** Rust, Tokio streams/channels, `cardano_explorer::CardanoNetwork`, `FundingEvent<FinalizedTxOut>`, `BTreeSet<FinalizedTxOut>`, existing Cargo tests.

---

### Task 0: Add Minimum Operator Funding Config

**Files:**
- Modify: `green-order-cardano-agent/src/config.rs`
- Modify: `green-order-cardano-agent/resources/preprod.config.json`

**Why:** There is already an old in-gap separator of about 2 ADA. If all funding UTxOs are bootstrapped and selection is ref-only, that small UTxO can be selected before the intended 50 ADA separator and fail the forced funding replacement path.

**Step 1: Add config field**

Add to `AppConfig`:

```rust
#[serde(default = "default_min_operator_funding_lovelace")]
pub min_operator_funding_lovelace: u64,
```

Add the default:

```rust
fn default_min_operator_funding_lovelace() -> u64 {
    10_000_000
}
```

**Step 2: Set preprod value**

Add to `green-order-cardano-agent/resources/preprod.config.json`:

```json
"minOperatorFundingLovelace": 50000000
```

Use `50_000_000` on preprod while the Aleph forced funding-replacement workaround is active.

**Step 3: Verify compile**

Run:

```bash
cargo check -p green-order-cardano-agent
```

Expected: config deserializes, and omitted values use `10_000_000`.

### Task 1: Add Funding Selection Regression Test

**Files:**
- Modify: `bloom-offchain/src/execution_engine/mod.rs`

**Why:** The last rejected preprod tx used `account -> pool -> funding` even though a separator existed. We need a unit-level guard that the engine prefers an available funding UTxO between consumed refs before falling back to the global lowest funding ref.

**Step 1: Write the failing test**

Add a small pure helper test near existing `execution_engine` tests. Keep this test generic inside `bloom-offchain`; do not import Cardano-only `OutputRef`, `TransactionHash`, or `FinalizedTxOut` into this crate.

```rust
#[test]
fn funding_selection_prefers_ref_between_consumed_refs() {
    let account = TestRef(0x31);
    let pool = TestRef(0x84);
    let separator = TestBearer(TestRef(0x78));
    let late = TestBearer(TestRef(0xd1));

    let mut funding_pool = BTreeSet::from([late.clone(), separator.clone()]);
    let consumed_versions = HashSet::from([account, pool]);
    let selected = pop_preferred_funding(&mut funding_pool, &consumed_versions).unwrap();

    assert_eq!(selected.select::<TestRef>(), separator.select::<TestRef>());
    assert!(funding_pool.contains(&late));
}
```

Also add a fallback test:

```rust
#[test]
fn funding_selection_falls_back_to_lowest_ref_without_gap_candidate() {
    let account = TestRef(0x31);
    let pool = TestRef(0x84);
    let late = TestBearer(TestRef(0xd1));

    let mut funding_pool = BTreeSet::from([late.clone()]);
    let consumed_versions = HashSet::from([account, pool]);
    let selected = pop_preferred_funding(&mut funding_pool, &consumed_versions).unwrap();

    assert_eq!(selected.select::<TestRef>(), late.select::<TestRef>());
}
```

Add the generic test fixtures in the same test module:

```rust
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
struct TestRef(u8);

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct TestBearer(TestRef);

impl Has<TestRef> for TestBearer {
    fn select<U: IsEqual<TestRef>>(&self) -> TestRef {
        self.0
    }
}
```

Also add the safety test that prevents this helper from changing behavior for recipes with more than two consumed refs:

```rust
#[test]
fn funding_selection_uses_lowest_ref_for_non_two_input_recipe() {
    let mut funding_pool = BTreeSet::from([TestBearer(TestRef(0x20)), TestBearer(TestRef(0x78))]);
    let consumed_versions = HashSet::from([TestRef(0x10), TestRef(0x50), TestRef(0x90)]);

    let selected = pop_preferred_funding(&mut funding_pool, &consumed_versions).unwrap();

    assert_eq!(selected.select::<TestRef>(), TestRef(0x20));
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p bloom-offchain funding_selection_prefers_ref_between_consumed_refs -- --nocapture
```

Expected: fail to compile because `pop_preferred_funding` does not exist.

**Step 3: Implement minimal funding-selection helper**

Add a helper in `bloom-offchain/src/execution_engine/mod.rs` close to the engine implementation:

```rust
fn pop_preferred_funding<Bearer, V>(
    funding_pool: &mut BTreeSet<Bearer>,
    consumed_versions: &HashSet<V>,
) -> Option<Bearer>
where
    Bearer: Has<V> + Ord + Clone,
    V: Ord + Copy,
{
    if consumed_versions.len() != 2 {
        return funding_pool.pop_first();
    }

    let mut consumed = consumed_versions.iter().copied().collect::<Vec<_>>();
    consumed.sort();

    let lower = consumed[0];
    let upper = consumed[1];
    let gap_candidate = funding_pool
        .iter()
        .find(|funding| {
            let funding_ref = funding.select::<V>();
            lower < funding_ref && funding_ref < upper
        })
        .cloned();

    if let Some(candidate) = gap_candidate {
        funding_pool.take(&candidate)
    } else {
        funding_pool.pop_first()
    }
}
```

Then replace:

```rust
if let Some(funding) = self.funding_pool.pop_first() {
```

with:

```rust
if let Some(funding) = pop_preferred_funding(&mut self.funding_pool, &consumed_versions) {
```

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test -p bloom-offchain funding_selection_ -- --nocapture
```

Expected: all three funding-selection tests pass.

### Task 2: Add Startup Funding Bootstrap Helper

**Files:**
- Create: `green-order-cardano-agent/src/funding_bootstrap.rs`
- Modify: `green-order-cardano-agent/src/main.rs`

**Why:** A funding UTxO created before the agent starts may be visible through Koios/Maestro but absent from the in-memory funding pool until the node stream replays its block. That caused the engine to ignore `78c669...#0` and select stale `d1c512...#2`.

**Step 1: Write the failing tests**

Create `green-order-cardano-agent/src/funding_bootstrap.rs` with tests first. Add a pure helper:

```rust
pub fn funding_events_from_utxos<const N: usize>(
    funding_addresses: &FundingAddresses<N>,
    skip_set: OutputRef,
    min_lovelace: u64,
    utxos: Vec<TransactionUnspentOutput>,
) -> Vec<(usize, FundingEvent<FinalizedTxOut>)> {
    todo!("implemented in Step 3")
}
```

Add tests:

```rust
#[test]
fn funding_events_from_utxos_keeps_operator_funding_outputs() {
    let funding_addresses = test_funding_addresses();
    let first = funding_addresses[0].clone();
    let utxo = tx_unspent_at(first, "78c669207607a5007f3201b0f31aea3375c9de2a12ca2b7172af484ccc9f2504", 0, 50_000_000);

    let events = funding_events_from_utxos(&funding_addresses, unrelated_ref(), 50_000_000, vec![utxo]);

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, 0);
    assert!(matches!(events[0].1, FundingEvent::Produced(_)));
}

#[test]
fn funding_events_from_utxos_skips_collateral_ref() {
    let funding_addresses = test_funding_addresses();
    let first = funding_addresses[0].clone();
    let collateral_ref = ref_from_hex("78c669207607a5007f3201b0f31aea3375c9de2a12ca2b7172af484ccc9f2504", 0);
    let utxo = tx_unspent_at(first, collateral_ref.tx_hash().to_hex(), collateral_ref.index(), 50_000_000);

    let events = funding_events_from_utxos(&funding_addresses, collateral_ref, 50_000_000, vec![utxo]);

    assert!(events.is_empty());
}

#[test]
fn funding_events_from_utxos_skips_low_value_funding() {
    let funding_addresses = test_funding_addresses();
    let first = funding_addresses[0].clone();
    let utxo = tx_unspent_at(first, "3db70e29807b91be97e9f95eb399f8364e5392e2f985c31c1a6c5eba11c5fa12", 0, 2_000_001);

    let events = funding_events_from_utxos(&funding_addresses, unrelated_ref(), 50_000_000, vec![utxo]);

    assert!(events.is_empty());
}
```

Keep fixture helpers local to the test module. Use the same address construction pattern as `spectrum-offchain-cardano/src/creds.rs` tests, or reuse the four operator addresses from `green-order-cardano-agent/e2e/preprod/.env.operator` only as literal test addresses.

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p green-order-cardano-agent funding_events_from_utxos -- --nocapture
```

Expected: fail because the helper is `todo!()`.

**Step 3: Implement pure helper**

Implementation rules:
- Convert each `TransactionUnspentOutput` into `(TransactionOutput, OutputRef)`.
- Keep only UTxOs whose output address belongs to one of `funding_addresses`.
- Skip `skip_set` exactly, because collateral must never enter the funding pool.
- Skip UTxOs whose lovelace amount is below `min_lovelace`.
- Emit `FundingEvent::Produced(FinalizedTxOut(output, output_ref))`.
- Sort events by `(partition, OutputRef)` before returning for deterministic tests/logs.

**Step 4: Add funding index seeding helper**

Add a helper that mirrors `FundingEventHandler` indexing behavior for startup events:

```rust
pub async fn seed_funding_index<Index>(
    funding_index: Arc<Mutex<Index>>,
    events: &[(usize, FundingEvent<FinalizedTxOut>)],
)
where
    Index: KvIndex<OutputRef, (usize, FinalizedTxOut)> + Send,
{
    let mut index = funding_index.lock().await;
    index.run_eviction();
    for (pt, event) in events {
        match event {
            FundingEvent::Consumed(consumed) => {
                index.register_for_eviction(consumed.reference());
            }
            FundingEvent::Produced(produced) => {
                index.put(produced.reference(), (*pt, produced.clone()));
            }
        }
    }
}
```

This is required because direct bootstrap events sent to execution streams do not pass through `FundingEventHandler`. Without this step, future consumed events for bootstrapped refs may not be recognized by the funding handler index, leaving stale funding in engine state after external spends or replay gaps.

**Step 5: Add async explorer bootstrap function**

In the same file add:

```rust
pub async fn bootstrap_funding_from_explorer<const N: usize, Net>(
    explorer: &Net,
    funding_addresses: FundingAddresses<N>,
    skip_set: OutputRef,
    min_lovelace: u64,
    page_limit: u16,
) -> Vec<(usize, FundingEvent<FinalizedTxOut>)>
where
    Net: CardanoNetwork + Sync,
{
    // page each funding address with offset += page_limit until an empty page is returned
}
```

Rules:
- Query `explorer.utxos_by_address(address, offset, page_limit).await`.
- Continue paging until the returned page is empty.
- Use `funding_events_from_utxos` to normalize results.
- Log count per address and accepted bootstrapped refs at `info!` level, including partition and lovelace.
- Log skipped low-value funding refs at `debug!` level.
- Do not panic on an empty address.

**Step 6: Run tests**

Run:

```bash
cargo test -p green-order-cardano-agent funding_bootstrap -- --nocapture
```

Expected: all bootstrap tests pass.

### Task 3: Inject Bootstrapped Funding Without Blocking Startup

**Files:**
- Modify: `green-order-cardano-agent/src/main.rs`
- Modify: `green-order-cardano-agent/src/funding_bootstrap.rs`

**Step 1: Wire module**

Add:

```rust
mod funding_bootstrap;
```

and import:

```rust
use crate::funding_bootstrap::{bootstrap_funding_from_explorer, seed_funding_index};
```

**Step 2: Bootstrap after funding channels are created**

After `funding_index` is created and before `FundingEventHandler::new(...)`, run:

```rust
let bootstrapped_funding = bootstrap_funding_from_explorer(
    &explorer,
    funding_addresses.clone(),
    collateral.reference(),
    config.min_operator_funding_lovelace,
    100,
).await;
seed_funding_index(Arc::clone(&funding_index), &bootstrapped_funding).await;
```

**Step 3: Split bootstrapped events per partition**

```rust
let mut bootstrap_funding_p1 = Vec::new();
let mut bootstrap_funding_p2 = Vec::new();
let mut bootstrap_funding_p3 = Vec::new();
let mut bootstrap_funding_p4 = Vec::new();
for (pt, event) in bootstrapped_funding {
    match pt {
        0 => bootstrap_funding_p1.push(event),
        1 => bootstrap_funding_p2.push(event),
        2 => bootstrap_funding_p3.push(event),
        3 => bootstrap_funding_p4.push(event),
        _ => unreachable!("operator funding partition out of range"),
    }
}
```

Do not send these through bounded channels before receivers are running. A pre-spawn `send(...).await` can deadlock if an address has more UTxOs than the configured channel capacity.

**Step 4: Prepend bootstrap events to each engine funding stream**

When constructing each `execution_part_stream`, replace the raw receiver argument:

```rust
funding_upd_recv_p1,
```

with:

```rust
futures::stream::iter(bootstrap_funding_p1).chain(funding_upd_recv_p1),
```

Repeat for partitions 2, 3, and 4.

**Important ordering:** The chained funding stream must be passed into `execution_part_stream(...)` before the engine task is spawned. Because the stream emits bootstrapped funding before live funding updates, the funding pool is populated before the engine can matchmake after `state_synced` becomes true.

**Step 5: Add startup log**

Log:

```rust
info!("Bootstrapped {} current operator funding UTxOs from explorer", count);
```

Also log the exact accepted refs:

```rust
info!("Bootstrapped operator funding refs: {}", display_vec(&bootstrapped_refs));
```

The runtime E2E check must be able to confirm `78c669207607a5007f3201b0f31aea3375c9de2a12ca2b7172af484ccc9f2504#0` was loaded before smoke submission.

**Step 6: Verify compile**

Run:

```bash
cargo check -p green-order-cardano-agent
```

Expected: compile succeeds.

### Task 4: Update Preprod Separator Script Guardrails

**Files:**
- Modify: `green-order-cardano-agent/e2e/preprod/08-create-separator-funding.ts`
- Modify: `green-order-cardano-agent/e2e/preprod/deno.json`

**Step 1: Keep existing 50 ADA default**

Keep:

```ts
const separatorLovelace = BigInt(Deno.env.get("SEPARATOR_FUNDING_LOVELACE") ?? "50000000");
```

Keep:

```ts
const forceCreate = Deno.env.get("FORCE_SEPARATOR_FUNDING") === "1";
```

**Step 2: Add script to Deno check task**

Update `deno.json` so `08-create-separator-funding.ts` is included in the `check` task.

**Step 3: Verify Deno**

Run:

```bash
deno fmt green-order-cardano-agent/e2e/preprod/08-create-separator-funding.ts
deno check --no-lock green-order-cardano-agent/e2e/preprod/08-create-separator-funding.ts
```

Expected: both pass.

### Task 5: Synthetic Preprod Verification Without Waiting for Chain Replay

**Files:**
- Test only; no source changes expected.

**Step 1: Unit verification**

Run:

```bash
cargo test -p bloom-offchain funding_selection_ -- --nocapture
cargo test -p green-order-cardano-agent funding_bootstrap -- --nocapture
cargo test -p bloom-offchain-cardano aleph_ -- --nocapture
```

Expected: all pass.

**Step 2: Runtime verification**

Start the agent normally:

```bash
cargo run -p green-order-cardano-agent -- --config-path green-order-cardano-agent/resources/preprod.config.json --deployment-path green-order-cardano-agent/resources/preprod.deployment.json --validation-rules-path green-order-cardano-agent/resources/validation-rules.json.template --log4rs-path green-order-cardano-agent/resources/log4rs.debug.yaml
```

Expected log before any smoke intent:

```text
Bootstrapped N current operator funding UTxOs from explorer
Bootstrapped operator funding refs: [...]
```

where `N >= 1` and includes `78c669207607a5007f3201b0f31aea3375c9de2a12ca2b7172af484ccc9f2504#0` if it is still unspent.

**Step 3: Smoke verification**

Run:

```bash
deno run --no-lock --allow-net --allow-read --allow-write --allow-env green-order-cardano-agent/e2e/preprod/04-submit-green-order-smoke.ts
```

Expected:
- Built tx uses a funding input whose ref sorts between account `310eca...#0` and pool `84fb...#0`; currently expected `78c669...#0` or `3db70e...#0`.
- No `ValidationTagMismatch` for Aleph batch witness.
- Smoke script observes successor account and successor pool.

### Task 6: Commit

**Files:**
- `bloom-offchain/src/execution_engine/mod.rs`
- `green-order-cardano-agent/src/config.rs`
- `green-order-cardano-agent/src/funding_bootstrap.rs`
- `green-order-cardano-agent/src/main.rs`
- `green-order-cardano-agent/resources/preprod.config.json`
- `green-order-cardano-agent/e2e/preprod/08-create-separator-funding.ts`
- `green-order-cardano-agent/e2e/preprod/deno.json`
- `docs/plans/2026-05-20-green-order-funding-bootstrap-selection.md`

Run:

```bash
git add bloom-offchain/src/execution_engine/mod.rs green-order-cardano-agent/src/config.rs green-order-cardano-agent/src/funding_bootstrap.rs green-order-cardano-agent/src/main.rs green-order-cardano-agent/resources/preprod.config.json green-order-cardano-agent/e2e/preprod/08-create-separator-funding.ts green-order-cardano-agent/e2e/preprod/deno.json docs/plans/2026-05-20-green-order-funding-bootstrap-selection.md
git commit -m "fix: bootstrap green order funding selection"
```

Expected: commit succeeds after all tests above pass.
