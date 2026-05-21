# Green Account Binding Endpoint Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a local HTTP endpoint that binds an externally known green `accountId` to one observed Aleph account UTxO exactly once, after which the agent tracks future account UTxOs only through planned/persisted account transitions produced by the green-order execution flow.

**Architecture:** The binding endpoint is an admin operation on the existing loopback-only green HTTP service. It does not create or mutate on-chain state; it only connects an already-indexed Aleph account output to the external `accountId` used by incoming intents. After binding, `AccountEventHandler` and `AccountIndex` keep the account current by matching consumed/produced Aleph account UTxOs against planned MPF store-root transitions or persisted account-store snapshots; the agent must not bind arbitrary produced account outputs just because an old bound output was consumed.

**Tech Stack:** Rust, Axum, existing `AccountIndex`, existing `AlephAccountUtxo` scanner, existing account-store persistence, Cargo tests.

---

### Task 1: Add One-Time Binding Semantics To AccountIndex

**Files:**
- Modify: `green-order-cardano-agent/src/account_index.rs`

**Step 1: Write failing tests**

Add tests near the existing `bind_external_account_id` tests:

```rust
#[test]
fn external_binding_rejects_second_bind_for_same_account_id() {
    let id = account_id(20);
    let ref_1 = output_ref(20);
    let ref_2 = output_ref(21);
    let mut index = AccountIndex::default();
    index.observe_created_or_updated(account_utxo(ref_1, dummy_output(2_000_000)));
    index.observe_created_or_updated(account_utxo(ref_2, dummy_output(2_000_000)));

    assert!(index.bind_external_account_id_once(id, ref_1).is_ok());
    assert_eq!(
        index.bind_external_account_id_once(id, ref_2),
        Err(AccountBindingError::AlreadyBound)
    );
    assert_eq!(index.current(id).map(|account| account.reference()), Some(ref_1));
}

#[test]
fn external_binding_rejects_output_already_bound_to_other_account_id() {
    let id_1 = account_id(21);
    let id_2 = account_id(22);
    let out_ref = output_ref(22);
    let mut index = AccountIndex::default();
    index.observe_created_or_updated(account_utxo(out_ref, dummy_output(2_000_000)));

    assert!(index.bind_external_account_id_once(id_1, out_ref).is_ok());
    assert_eq!(
        index.bind_external_account_id_once(id_2, out_ref),
        Err(AccountBindingError::OutputAlreadyBound)
    );
}

#[test]
fn external_binding_rejects_unknown_output_ref() {
    let id = account_id(23);
    let mut index = AccountIndex::default();

    assert_eq!(
        index.bind_external_account_id_once(id, output_ref(23)),
        Err(AccountBindingError::OutputNotObserved)
    );
}

#[test]
fn one_time_binding_survives_restart_via_persisted_store() {
    let id = account_id(24);
    let out_ref = output_ref(24);
    let output = dummy_output(2_000_000);
    let path = std::env::temp_dir().join(format!(
        "green-account-binding-test-{}-once.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);

    {
        let mut index = AccountIndex::with_persistence_path(path.clone());
        index.observe_created_or_updated(account_utxo(out_ref, output.clone()));
        assert!(index.bind_external_account_id_once(id, out_ref).is_ok());
    }

    let mut reloaded = AccountIndex::with_persistence_path(path.clone());
    reloaded.observe_created_or_updated(account_utxo(out_ref, output));

    assert_eq!(reloaded.current(id).map(|account| account.reference()), Some(out_ref));
    assert_eq!(
        reloaded.bind_external_account_id_once(id, out_ref),
        Err(AccountBindingError::AlreadyBound)
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn persisted_output_ref_cannot_be_rebound_to_different_account_before_lazy_restore() {
    let id_1 = account_id(26);
    let id_2 = account_id(27);
    let out_ref = output_ref(27);
    let output = dummy_output(2_000_000);
    let path = std::env::temp_dir().join(format!(
        "green-account-binding-test-{}-persisted-output-ref.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);

    {
        let mut index = AccountIndex::with_persistence_path(path.clone());
        index.observe_created_or_updated(account_utxo(out_ref, output.clone()));
        assert!(index.bind_external_account_id_once(id_1, out_ref).is_ok());
    }

    let mut reloaded = AccountIndex::with_persistence_path(path.clone());
    assert_eq!(reloaded.current(id_1), None);

    assert_eq!(
        reloaded.bind_external_account_id_once(id_2, out_ref),
        Err(AccountBindingError::OutputAlreadyBound)
    );
    reloaded.observe_created_or_updated(account_utxo(out_ref, output));
    assert_eq!(reloaded.current(id_1).map(|account| account.reference()), Some(out_ref));
    let _ = std::fs::remove_file(path);
}
```

**Step 2: Run tests to verify failure**

Run:

```bash
cargo test -p green-order-cardano-agent account_index::tests::external_binding -- --nocapture
```

Expected: FAIL because `AccountBindingError` and `bind_external_account_id_once` do not exist.

**Step 3: Implement binding result type**

Add near `AccountIndex`:

```rust
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum AccountBindingError {
    AlreadyBound,
    OutputAlreadyBound,
    OutputNotObserved,
    NonEmptyUnplannedRoot,
}
```

**Step 4: Preserve legacy binding and implement one-time binding**

Keep the existing `bind_external_account_id` method unchanged for current unit-test fixtures and any existing internal callers in `intent_source.rs` / `http_intent_source.rs`. Add the stricter `bind_external_account_id_once` method beside it, and make only the new HTTP `/accounts/bind` endpoint use the one-time method.

```rust
pub fn bind_external_account_id_once(
    &mut self,
    account_id: AccountId,
    output_ref: OutputRef,
) -> Result<FinalizedTxOut, AccountBindingError> {
    if self.by_account_id.contains_key(&account_id)
        || self.pending_by_account_id.contains(&account_id)
        || self
            .persisted_by_output_ref
            .values()
            .any(|(persisted_id, _)| *persisted_id == account_id)
    {
        return Err(AccountBindingError::AlreadyBound);
    }
    if self.by_output_ref.contains_key(&output_ref) {
        return Err(AccountBindingError::OutputAlreadyBound);
    }
    if self.persisted_by_output_ref.contains_key(&output_ref) {
        return Err(AccountBindingError::OutputAlreadyBound);
    }

    let account = self
        .unbound_by_output_ref
        .remove(&output_ref)
        .ok_or(AccountBindingError::OutputNotObserved)?;

    let Some(store) = AccountStore::from_observed_root(account.state.store_root) else {
        self.unbound_by_output_ref.insert(output_ref, account);
        return Err(AccountBindingError::NonEmptyUnplannedRoot);
    };

    self.bind_indexed(
        account_id,
        IndexedAccount {
            utxo: FinalizedTxOut::new(account.output, account.output_ref),
            store,
        },
    );
    self.current(account_id).ok_or(AccountBindingError::OutputNotObserved)
}
```

**Step 5: Run tests**

Run:

```bash
cargo test -p green-order-cardano-agent account_index::tests::external_binding -- --nocapture
cargo test -p green-order-cardano-agent account_index::tests::one_time_binding_survives_restart_via_persisted_store -- --nocapture
```

Expected: PASS.

Important restart model: the persisted binding stores enough information to rebind safely, but `AccountIndex::current(accountId)` is still empty immediately after process restart until the ledger scanner observes the persisted `outputRef` again. Tests and documentation must state this explicitly.

---

### Task 2: Add HTTP Binding Request/Response Types

**Files:**
- Modify: `green-order-cardano-agent/src/http_intent_source.rs`

**Step 1: Write failing unit tests**

Add tests in `http_intent_source.rs`:

```rust
fn indexed_unbound_account(
    id: AccountId,
    lovelace: u64,
    nonce: i64,
) -> (AccountIndex, AccountCtx, OutputRef) {
    let ctx = ctx();
    let out_ref = OutputRef::new(TransactionHash::from([31; 32]), 0);
    let output = account_output(ctx.account.script_hash, lovelace, nonce);
    let account = AlephAccountUtxo::try_parse(out_ref, output, &ctx).unwrap();
    let mut index = AccountIndex::default();
    index.observe_created_or_updated(account);
    (index, ctx, out_ref)
}

fn test_state(index: AccountIndex, ctx: AccountCtx) -> HttpIntentState<AccountCtx> {
    let (events, _) = event_channels(4);
    HttpIntentState {
        account_index: Arc::new(Mutex::new(index)),
        ctx,
        config: GreenOrdersConfig::default(),
        events,
    }
}

#[tokio::test]
async fn post_bind_account_binds_observed_empty_root_account() {
    let id = account_id(31);
    let (index, ctx, out_ref) = indexed_unbound_account(id, 2_000_000, 0);
    let state = test_state(index, ctx);

    let response = post_bind_account(
        State(state.clone()),
        Json(BindAccountRequest {
            account_id: hex::encode(id.bytes()),
            tx_hash: hex::encode(out_ref.tx_hash().to_raw_bytes()),
            output_index: out_ref.index(),
        }),
    )
    .await;

    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(response.1.0.status, "bound");
}

#[tokio::test]
async fn post_bind_account_rejects_second_bind() {
    let id = account_id(32);
    let (mut index, ctx, out_ref) = indexed_unbound_account(id, 2_000_000, 0);
    index.bind_external_account_id_once(id, out_ref).unwrap();
    let state = test_state(index, ctx);

    let response = post_bind_account(
        State(state),
        Json(BindAccountRequest {
            account_id: hex::encode(id.bytes()),
            tx_hash: hex::encode(out_ref.tx_hash().to_raw_bytes()),
            output_index: out_ref.index(),
        }),
    )
    .await;

    assert_eq!(response.0, StatusCode::CONFLICT);
    assert_eq!(response.1.0.reason, Some("alreadyBound"));
}
```

If `OutputRef` has different accessor names, adapt the test to the actual type methods.

Also add explicit malformed/error tests:

```rust
#[tokio::test]
async fn post_bind_account_rejects_malformed_account_id() {
    let id = account_id(34);
    let (index, ctx, out_ref) = indexed_unbound_account(id, 2_000_000, 0);
    let response = post_bind_account(
        State(test_state(index, ctx)),
        Json(BindAccountRequest {
            account_id: "not-hex".to_string(),
            tx_hash: hex::encode(out_ref.tx_hash().to_raw_bytes()),
            output_index: out_ref.index(),
        }),
    )
    .await;
    assert_eq!(response.0, StatusCode::BAD_REQUEST);
    assert_eq!(response.1.0.reason, Some("malformedBinding"));
}

#[tokio::test]
async fn post_bind_account_rejects_malformed_tx_hash() {
    let id = account_id(35);
    let (index, ctx, out_ref) = indexed_unbound_account(id, 2_000_000, 0);
    let response = post_bind_account(
        State(test_state(index, ctx)),
        Json(BindAccountRequest {
            account_id: hex::encode(id.bytes()),
            tx_hash: "abcd".to_string(),
            output_index: out_ref.index(),
        }),
    )
    .await;
    assert_eq!(response.0, StatusCode::BAD_REQUEST);
    assert_eq!(response.1.0.reason, Some("malformedBinding"));
}

#[tokio::test]
async fn post_bind_account_rejects_unknown_output() {
    let id = account_id(36);
    let state = test_state(AccountIndex::default(), ctx());
    let response = post_bind_account(
        State(state),
        Json(BindAccountRequest {
            account_id: hex::encode(id.bytes()),
            tx_hash: hex::encode([36u8; 32]),
            output_index: 0,
        }),
    )
    .await;
    assert_eq!(response.0, StatusCode::NOT_FOUND);
    assert_eq!(response.1.0.reason, Some("outputNotObserved"));
}

#[tokio::test]
async fn post_bind_account_rejects_non_empty_unplanned_root() {
    let id = account_id(37);
    let ctx = ctx();
    let out_ref = OutputRef::new(TransactionHash::from([37; 32]), 0);
    let output = account_output_with_store_root(ctx.account.script_hash, 2_000_000, 0, [9; 32]);
    let account = AlephAccountUtxo::try_parse(out_ref, output, &ctx).unwrap();
    let mut index = AccountIndex::default();
    index.observe_created_or_updated(account);

    let response = post_bind_account(
        State(test_state(index, ctx)),
        Json(BindAccountRequest {
            account_id: hex::encode(id.bytes()),
            tx_hash: hex::encode(out_ref.tx_hash().to_raw_bytes()),
            output_index: out_ref.index(),
        }),
    )
    .await;
    assert_eq!(response.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(response.1.0.reason, Some("nonEmptyUnplannedRoot"));
}
```

If `account_output_with_store_root` does not exist, add it next to the current test-only `account_output` helper and make `account_output` call it with `[0; 32]`.

**Step 2: Run tests to verify failure**

Run:

```bash
cargo test -p green-order-cardano-agent http_intent_source::tests::post_bind_account -- --nocapture
```

Expected: FAIL because binding endpoint types/handler do not exist.

**Step 3: Add request and response types**

Add:

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BindAccountRequest {
    account_id: String,
    tx_hash: String,
    output_index: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BindAccountResponse {
    pub status: &'static str,
    pub reason: Option<&'static str>,
}
```

**Step 4: Add parser**

Add:

```rust
fn parse_bind_account_request(req: BindAccountRequest) -> Result<(AccountId, OutputRef), ()> {
    let account_id_bytes = hex::decode(req.account_id).map_err(|_| ())?;
    let account_id = AccountId::try_from_slice(&account_id_bytes).map_err(|_| ())?;
    let tx_hash_bytes = hex::decode(req.tx_hash).map_err(|_| ())?;
    let tx_hash = TransactionHash::from_raw_bytes(&tx_hash_bytes).map_err(|_| ())?;
    Ok((account_id, OutputRef::new(tx_hash, req.output_index)))
}
```

Import `cml_crypto::TransactionHash`.

**Step 5: Add handler**

Add:

```rust
async fn post_bind_account<C>(
    State(state): State<HttpIntentState<C>>,
    Json(req): Json<BindAccountRequest>,
) -> (StatusCode, Json<BindAccountResponse>)
where
    C: Has<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>> + Clone + Send + Sync + 'static,
{
    let Ok((account_id, output_ref)) = parse_bind_account_request(req) else {
        return bind_rejected(StatusCode::BAD_REQUEST, "malformedBinding");
    };

    let result = state
        .account_index
        .lock()
        .expect("account index lock poisoned")
        .bind_external_account_id_once(account_id, output_ref);

    match result {
        Ok(_) => (
            StatusCode::OK,
            Json(BindAccountResponse {
                status: "bound",
                reason: None,
            }),
        ),
        Err(err) => bind_rejected(status_for_binding_error(err), reason_for_binding_error(err)),
    }
}
```

Add helpers:

```rust
fn bind_rejected(status: StatusCode, reason: &'static str) -> (StatusCode, Json<BindAccountResponse>) {
    (
        status,
        Json(BindAccountResponse {
            status: "rejected",
            reason: Some(reason),
        }),
    )
}

fn status_for_binding_error(err: AccountBindingError) -> StatusCode {
    match err {
        AccountBindingError::AlreadyBound | AccountBindingError::OutputAlreadyBound => StatusCode::CONFLICT,
        AccountBindingError::OutputNotObserved => StatusCode::NOT_FOUND,
        AccountBindingError::NonEmptyUnplannedRoot => StatusCode::UNPROCESSABLE_ENTITY,
    }
}

fn reason_for_binding_error(err: AccountBindingError) -> &'static str {
    match err {
        AccountBindingError::AlreadyBound => "alreadyBound",
        AccountBindingError::OutputAlreadyBound => "outputAlreadyBound",
        AccountBindingError::OutputNotObserved => "outputNotObserved",
        AccountBindingError::NonEmptyUnplannedRoot => "nonEmptyUnplannedRoot",
    }
}
```

**Step 6: Register route**

Update router:

```rust
Router::new()
    .route("/intents", post(post_intent::<C>))
    .route("/accounts/bind", post(post_bind_account::<C>))
    .with_state(state)
```

**Step 7: Run tests**

Run:

```bash
cargo test -p green-order-cardano-agent http_intent_source::tests::post_bind_account -- --nocapture
```

Expected: PASS.

---

### Task 3: Verify Binding Enables Intent Admission

**Files:**
- Modify: `green-order-cardano-agent/src/http_intent_source.rs`
- Modify: `green-order-cardano-agent/src/account_index.rs` only if test helper visibility is needed

**Step 1: Write failing integration-style unit test**

Add:

```rust
#[tokio::test]
async fn bound_account_can_accept_intent_after_single_binding() {
    let id = account_id(33);
    let (index, ctx, out_ref) = indexed_unbound_account(id, 5_000_000, 0);
    let state = test_state(index, ctx);

    let bind = post_bind_account(
        State(state.clone()),
        Json(BindAccountRequest {
            account_id: hex::encode(id.bytes()),
            tx_hash: hex::encode(out_ref.tx_hash().to_raw_bytes()),
            output_index: out_ref.index(),
        }),
    )
    .await;
    assert_eq!(bind.0, StatusCode::OK);

    let intent = post_intent(State(state.clone()), Json(wire_intent(id))).await;
    assert_eq!(intent.0, StatusCode::ACCEPTED);
}
```

Use existing `wire_intent`, `event_channels`, and account fixture helpers if possible. If helper names differ, keep the test behavior identical.

**Step 2: Run test**

Run:

```bash
cargo test -p green-order-cardano-agent http_intent_source::tests::bound_account_can_accept_intent_after_single_binding -- --nocapture
```

Expected: PASS after Task 2. If it fails with `missingAccount`, the binding did not update `AccountIndex::current`.

---

### Task 4: Prove Planned Future UTxOs Are Tracked Without Rebinding

**Files:**
- Modify: `green-order-cardano-agent/src/account_index.rs`

**Step 1: Write test**

Add:

```rust
#[test]
fn once_bound_account_tracks_planned_later_account_output_without_rebinding() {
    let id = account_id(25);
    let old_ref = output_ref(25);
    let new_ref = output_ref(26);
    let old_output = dummy_output(2_000_000);
    let new_output = dummy_output(2_100_000);
    let canonical_order_id = GreenOrderId::new(id, 0, 42, [25; 32]);
    let updated_intent = intent(500);
    let mut index = AccountIndex::default();

    index.observe_created_or_updated(account_utxo(old_ref, old_output));
    index.bind_external_account_id_once(id, old_ref).unwrap();

    let planned = index
        .plan_sig_insert(
            id,
            old_ref,
            canonical_order_id,
            updated_intent.intent_key(),
            updated_intent,
        )
        .unwrap();

    index.observe_transaction(
        [old_ref],
        [account_utxo_with_root(new_ref, new_output, planned.new_root)],
    );

    assert_eq!(index.current(id).map(|account| account.reference()), Some(new_ref));
    assert_eq!(
        index.bind_external_account_id_once(id, new_ref),
        Err(AccountBindingError::AlreadyBound)
    );
}
```

**Step 2: Run test**

Run:

```bash
cargo test -p green-order-cardano-agent account_index::tests::once_bound_account_tracks_planned_later_account_output_without_rebinding -- --nocapture
```

Expected: PASS.

This test is the direct proof of the business requirement: the endpoint is called once, then the agent follows later account UTxOs itself when those UTxOs match a planned green-order account-store transition. Do not implement broad behavior that binds any produced Aleph account output after consuming a bound account; that would be unsafe because unrelated or malicious outputs could become associated with the account id.

---

### Task 5: Add Preprod Operator Documentation

**Files:**
- Modify: `README.md`

**Step 1: Add short runbook section**

Add a section like:

```markdown
### Binding a Preprod Green Account

The green order agent must bind an externally known `accountId` to one observed Aleph account UTxO before it can accept intents for that account.

1. Start the agent and wait until chain sync reaches the block containing the Aleph account output.
2. Call the loopback-only admin endpoint:

```bash
curl -sS -X POST http://127.0.0.1:9031/accounts/bind \
  -H 'content-type: application/json' \
  -d '{
    "accountId": "<32-byte hex account id>",
    "txHash": "<32-byte transaction hash>",
    "outputIndex": 0
  }'
```

Expected success:

```json
{"status":"bound","reason":null}
```

The endpoint is one-time per `accountId`. After it succeeds, the agent tracks future account UTxOs from ledger and mempool events when they match planned MPF store-root transitions made by the execution flow. Calling it again for the same `accountId` returns `alreadyBound`.

After a restart, the persisted binding is restored lazily: `current(accountId)` becomes available only after chain sync observes the persisted account output again.
```

**Step 2: Run docs-adjacent checks**

Run:

```bash
cargo test -p green-order-cardano-agent http_intent_source::tests::post_bind_account -- --nocapture
```

Expected: PASS.

---

### Task 6: Full Verification

**Files:**
- No source changes unless verification fails

**Step 1: Run focused tests**

Run:

```bash
cargo test -p green-order-cardano-agent account_index::tests:: -- --nocapture
cargo test -p green-order-cardano-agent http_intent_source::tests:: -- --nocapture
```

Expected: PASS.

**Step 2: Run package test/check**

Run:

```bash
cargo test -p green-order-cardano-agent -- --nocapture
cargo check -p green-order-cardano-agent
```

Expected: PASS.

**Step 3: Optional smoke command once agent is running**

Run:

```bash
curl -sS -X POST http://127.0.0.1:9031/accounts/bind \
  -H 'content-type: application/json' \
  -d '{"accountId":"<account-id-hex>","txHash":"<tx-hash-hex>","outputIndex":0}'
```

Expected:

```json
{"status":"bound","reason":null}
```

Then submit a normal `POST /intents` request for the same `accountId`.

---

### Task 7: Commit

**Files:**
- Stage only files changed by this plan

**Step 1: Check status**

Run:

```bash
git status --short
```

Expected: inspect current worktree and stage only files touched by this plan. Do not rely on the full worktree containing only these files, because adjacent green-order work may already exist in the repository.

**Step 2: Commit**

Run:

```bash
git add green-order-cardano-agent/src/account_index.rs green-order-cardano-agent/src/http_intent_source.rs README.md
git commit -m "Add one-time green account binding endpoint"
```

Expected: commit succeeds.
