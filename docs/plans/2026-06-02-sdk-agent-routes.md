# SDK Agent Routes Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add minimal SDK-friendly Green Order agent routes so the TypeScript SDK can submit, bind, and monitor without relying only on query-string status helpers.

**Architecture:** Extend the existing loopback Green HTTP intent service in `green-order-cardano-agent/src/http_intent_source.rs`. Keep existing routes backwards-compatible, add read-only SDK routes, use explicit HTTP DTOs for stable hex/string wire shapes, and update the SDK client/docs to use the new routes.

**Tech Stack:** Rust Axum handlers and unit tests, existing `AccountIndex` state, Node 18+ TypeScript SDK client/tests, Markdown docs.

---

### Task 1: Add Internal Account Summary Model

**Files:**
- Modify: `green-order-cardano-agent/src/account_index.rs`

**Step 1: Write failing Rust tests**

Add tests beside the existing `account_status` tests:

```rust
#[test]
fn account_summary_reports_current_account_reference_and_store_root() {
    let id = account_id(91);
    let output_ref = OutputRef::new(TransactionHash::from([91; 32]), 0);
    let mut index = AccountIndex::default();
    let output = dummy_output(2_000_000);
    index.observe_created_or_updated(account_utxo_with_root(output_ref, output, [0; 32]));
    index.bind_external_account_id(id, output_ref).unwrap();

    let summary = index.account_summary(id).unwrap();

    assert_eq!(summary.account_id, id);
    assert_eq!(summary.current_output_ref, Some(output_ref));
    assert_eq!(summary.current_store_root, Some([0; 32]));
    assert!(!summary.pending);
    assert_eq!(summary.persisted_outputs, 0);
}

#[test]
fn account_summary_returns_none_for_unknown_account() {
    assert!(AccountIndex::default().account_summary(account_id(92)).is_none());
}

#[test]
fn account_index_summary_counts_empty_state() {
    let summary = AccountIndex::default().summary();

    assert_eq!(summary.current_accounts, 0);
    assert_eq!(summary.pending_accounts, 0);
    assert_eq!(summary.unbound_outputs, 0);
    assert_eq!(summary.predicted_outputs, 0);
    assert_eq!(summary.persisted_outputs, 0);
}
```

Add or adapt existing tests for non-current states:
- pending account after a planned update / consumed old ref: `account_summary` returns `Some` with `pending = true`.
- persisted account loaded from disk but not yet observed: `account_summary` returns `Some` with `persisted_outputs > 0`.
- unknown account: `account_summary` returns `None`.

Use existing fixture helpers in `account_index.rs`. The snippet above is intentionally adapted to the current helper shape: build an account output first, then call the local `account_utxo_with_root(output_ref, output, store_root)` helper. If exact helper names differ after nearby edits, use the existing `account_index.rs` fixture pattern rather than introducing a new model.

**Step 2: Run tests to verify failure**

```bash
cargo test -q -p green-order-cardano-agent account_summary
```

Expected: fail because `account_summary` / `summary` models are missing.

**Step 3: Implement internal model**

Add internal structs. These are not HTTP wire contracts:

```rust
#[derive(Debug, Clone)]
pub struct AccountSummary {
    pub account_id: AccountId,
    pub current_output_ref: Option<OutputRef>,
    pub current_store_root: Option<[u8; 32]>,
    pub pending: bool,
    pub persisted_outputs: usize,
}

#[derive(Debug, Copy, Clone)]
pub struct AccountIndexSummary {
    pub current_accounts: usize,
    pub pending_accounts: usize,
    pub unbound_outputs: usize,
    pub predicted_outputs: usize,
    pub persisted_outputs: usize,
}
```

Add methods:

```rust
pub fn account_summary(&self, account_id: AccountId) -> Option<AccountSummary>;
pub fn summary(&self) -> AccountIndexSummary;
```

Rules:
- return `Some` for current accounts.
- return `Some` for pending accounts even when current output is temporarily absent.
- return `Some` for persisted accounts loaded from disk but not observed yet.
- return `None` only for genuinely unknown account ids.
- do not expose private keys, local paths, wallet seeds, raw config, or persistence file paths.

**Step 4: Run tests**

```bash
cargo test -q -p green-order-cardano-agent account_summary
```

Expected: pass.

---

### Task 2: Add SDK-Friendly HTTP DTOs and Routes

**Files:**
- Modify: `green-order-cardano-agent/src/http_intent_source.rs`

**Routes to add:**
- `GET /accounts/:accountId`
- `GET /monitoring/summary`
- `GET /monitoring/readiness`

Keep existing routes unchanged:
- `POST /intents`
- `POST /accounts/bind`
- `GET /accounts/status`

**Step 1: Write failing handler tests**

Add tests using real 32-byte hex account ids:

```rust
#[tokio::test]
async fn get_account_returns_current_account_summary_as_hex_dto() {
    let id = account_id(41);
    let (index, ctx) = indexed_account(id, 2_000_000, 0);
    let response = get_account(State(test_state(index, ctx)), Path(hex::encode(id.bytes()))).await;

    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(response.1["status"], "found");
    assert_eq!(response.1["reason"], serde_json::Value::Null);
    assert_eq!(response.1["account"]["accountId"], hex::encode(id.bytes()));
    assert_eq!(response.1["account"]["currentOutputRef"]["txHash"], hex::encode([1u8; 32]));
    assert_eq!(response.1["account"]["currentOutputRef"]["outputIndex"], 0);
    assert_eq!(response.1["account"]["currentStoreRoot"], hex::encode([0u8; 32]));
}

#[tokio::test]
async fn get_account_returns_not_found_for_unknown_account() {
    let id = account_id(42);
    let response = get_account(State(test_state(AccountIndex::default(), ctx())), Path(hex::encode(id.bytes()))).await;

    assert_eq!(response.0, StatusCode::NOT_FOUND);
    assert_eq!(response.1["status"], "notFound");
    assert_eq!(response.1["reason"], "accountNotFound");
    assert_eq!(response.1["account"], serde_json::Value::Null);
}

#[tokio::test]
async fn get_account_rejects_malformed_account_id() {
    let response = get_account(State(test_state(AccountIndex::default(), ctx())), Path("not-hex".to_string())).await;

    assert_eq!(response.0, StatusCode::BAD_REQUEST);
    assert_eq!(response.1["reason"], "malformedAccountId");
}

#[tokio::test]
async fn get_monitoring_summary_returns_sanitized_counts() {
    let response = get_monitoring_summary(State(test_state(AccountIndex::default(), ctx()))).await;

    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(response.1["status"], "ok");
    assert_eq!(response.1["accounts"]["currentAccounts"], 0);
    assert_eq!(response.1["accounts"]["pendingAccounts"], 0);
}

#[tokio::test]
async fn get_monitoring_readiness_returns_api_readiness() {
    let response = get_monitoring_readiness().await;

    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(response.1["status"], "ok");
    assert_eq!(response.1["service"], "green-order-agent");
    assert_eq!(response.1["apiVersion"], 1);
    assert_eq!(response.1["accountIndex"], "available");
}

#[tokio::test]
async fn router_keeps_legacy_accounts_status_route_before_account_id_route() {
    let id = account_id(43);
    let (index, ctx) = indexed_account(id, 2_000_000, 0);
    let app = router(test_state(index, ctx));
    let uri = format!(
        "/accounts/status?accountId={}&txHash={}&outputIndex=0",
        hex::encode(id.bytes()),
        hex::encode([1u8; 32]),
    );

    let response = app
        .oneshot(axum::http::Request::builder().uri(uri).body(axum::body::Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
```

Adjust JSON field access if using typed response structs rather than `serde_json::Value`.

**Step 2: Run tests to verify failure**

```bash
cargo test -q -p green-order-cardano-agent http_intent_source::tests::get_account
cargo test -q -p green-order-cardano-agent http_intent_source::tests::get_monitoring
```

Expected: fail because handlers/routes are missing.

**Step 3: Implement explicit HTTP DTOs**

Do not serialize `AccountId`, `OutputRef`, or `[u8; 32]` directly. Add DTOs in `http_intent_source.rs`:

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HttpOutputRef {
    tx_hash: String,
    output_index: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HttpAccountSummary {
    account_id: String,
    current_output_ref: Option<HttpOutputRef>,
    current_store_root: Option<String>,
    pending: bool,
    persisted_outputs: usize,
}
```

Mapping rules:
- `AccountId` -> lowercase 32-byte hex string.
- `OutputRef` -> `{ txHash, outputIndex }`.
- store root `[u8; 32]` -> lowercase hex string.

**Step 4: Implement handlers and routes**

Add:

```rust
.route("/accounts/:account_id", get(get_account::<C>))
.route("/monitoring/summary", get(get_monitoring_summary::<C>))
.route("/monitoring/readiness", get(get_monitoring_readiness))
```

`GET /accounts/:accountId` found:

```json
{
  "status": "found",
  "reason": null,
  "account": {
    "accountId": "...",
    "currentOutputRef": { "txHash": "...", "outputIndex": 0 },
    "currentStoreRoot": "...",
    "pending": false,
    "persistedOutputs": 0
  }
}
```

Missing account:

```json
{
  "status": "notFound",
  "reason": "accountNotFound",
  "account": null
}
```

Monitoring summary:

```json
{
  "status": "ok",
  "accounts": {
    "currentAccounts": 0,
    "pendingAccounts": 0,
    "unboundOutputs": 0,
    "predictedOutputs": 0,
    "persistedOutputs": 0
  }
}
```

Readiness:

```json
{
  "status": "ok",
  "service": "green-order-agent",
  "apiVersion": 1,
  "accountIndex": "available"
}
```

This readiness endpoint is HTTP/API readiness only. It must not claim node sync, explorer availability, funding readiness, or end-to-end execution readiness.

**Step 5: Run tests**

```bash
cargo test -q -p green-order-cardano-agent http_intent_source
```

Expected: pass.

---

### Task 3: Update TypeScript SDK Client for New Routes

**Files:**
- Modify: `sdk/typescript/src/client.ts`
- Modify: `sdk/typescript/src/account.ts`
- Modify: `sdk/typescript/src/index.ts`
- Test: `sdk/typescript/test/client.test.ts`

**Step 1: Write failing SDK tests**

Use 64-character account-id fixtures:

```ts
test("queries account summaries through GET /accounts/:accountId", async () => {
  const accountId = "aa".repeat(32);
  const client = new GreenOrderClient({
    baseUrl: "http://127.0.0.1:9031",
    fetch: async (url, init) => {
      assert.equal(String(url), `http://127.0.0.1:9031/accounts/${accountId}`);
      assert.equal(init?.method, "GET");
      return new Response(JSON.stringify({ status: "found", reason: null, account: { accountId } }), {
        status: 200,
      });
    },
  });

  assert.deepEqual(await client.getAccount(accountId), {
    status: "found",
    reason: null,
    account: { accountId },
  });
});

test("rejects malformed account ids before calling GET /accounts/:accountId", async () => {
  const client = new GreenOrderClient({
    baseUrl: "http://127.0.0.1:9031",
    fetch: async () => {
      throw new Error("fetch should not be called");
    },
  });

  await assert.rejects(() => client.getAccount("aa"), /accountId must be 64 hex chars/);
});

test("queries green-order monitoring summary", async () => {
  const client = new GreenOrderClient({
    baseUrl: "http://127.0.0.1:9031",
    fetch: async (url) => {
      assert.equal(String(url), "http://127.0.0.1:9031/monitoring/summary");
      return new Response(JSON.stringify({ status: "ok", accounts: { currentAccounts: 0 } }), { status: 200 });
    },
  });

  assert.deepEqual(await client.getMonitoringSummary(), {
    status: "ok",
    accounts: { currentAccounts: 0 },
  });
});
```

**Step 2: Run SDK tests to verify failure**

```bash
cd sdk/typescript
npm test
```

Expected: fail because `getAccount`, account-id validation, and `getMonitoringSummary` are missing.

**Step 3: Implement SDK methods**

Add:

```ts
getAccount(accountId: string): Promise<unknown> {
  assertAccountId(accountId);
  return this.get(`/accounts/${encodeURIComponent(accountId)}`);
}

getMonitoringSummary(): Promise<unknown> {
  return this.get("/monitoring/summary");
}

getMonitoringReadiness(): Promise<unknown> {
  return this.get("/monitoring/readiness");
}
```

Keep `getReadiness()` mapped to `/health` for backwards compatibility.

**Step 4: Run SDK tests**

```bash
cd sdk/typescript
npm test
npm run test:coverage
```

Expected: pass and coverage output updates.

---

### Task 4: Update Documentation and Coverage Evidence

**Files:**
- Modify: `sdk/typescript/README.md`
- Modify: `docs/sdk/api-contract.md`
- Modify: `docs/sdk/typescript.md`
- Modify: `docs/sdk/test-coverage.md`
- Modify: `docs/sdk/examples/monitor-account.md`

**Step 1: Update docs**

Document:
- `GET /accounts/:accountId`
- `GET /monitoring/summary`
- `GET /monitoring/readiness`
- SDK methods `getAccount`, `getMonitoringSummary`, `getMonitoringReadiness`
- `GET /monitoring/readiness` is API handler readiness, not chain/node readiness.
- The old `GET /accounts/status` remains available for output-ref-specific checks.

**Step 2: Update coverage evidence**

Replace coverage numbers in `docs/sdk/test-coverage.md` with the latest `npm run test:coverage` output.

**Step 3: Verify docs mention GitBook publication requirement**

Add a short note that these Markdown files are the source for GitBook publication evidence.

---

### Task 5: Final Verification

Run:

```bash
cargo test -q -p green-order-cardano-agent http_intent_source
cargo test -q -p green-order-cardano-agent account_summary
cargo check -p green-order-cardano-agent
cargo test -q -p green-order-cardano-agent
cd sdk/typescript && npm test
cd sdk/typescript && npm run test:coverage
git status --short --branch
```

Expected:
- Rust route tests pass.
- Rust account summary tests pass.
- Full green-order-cardano-agent check/test pass.
- SDK tests pass.
- Coverage evidence has been updated.
- Worktree shows only intended SDK/API/docs changes plus pre-existing unrelated untracked state files.
