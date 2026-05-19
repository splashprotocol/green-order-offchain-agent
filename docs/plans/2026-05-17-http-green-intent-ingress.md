# HTTP Green Intent Ingress Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Accept incoming signed green intents through a minimal local HTTP endpoint and inject admitted intents into the existing green-order execution flow.

**Architecture:** Reuse the current intent admission path in `green-order-cardano-agent/src/intent_source.rs`: parse wire intent, validate account state via `AccountIndex`, build `GreenOrder`, and send the accepted event into the pair update stream. Add a small Axum HTTP server with one `POST /intents` endpoint; keep the existing TCP source intact unless business explicitly asks to remove it later.

**Tech Stack:** Rust, Tokio, Axum, Serde JSON, existing `AccountIndex`, existing `GreenOrdersConfig`, existing `GreenIntentEvent` channel.

---

## Non-Goals

- Do not build a public API.
- Do not add authentication in this iteration.
- Do not persist intents.
- Do not add retries or a queue.
- Do not support partial/path-auth execution unless `greenOrders.allowPartial` is explicitly enabled later.
- Do not change execution matching logic; intents still execute only against `RoyaltyV1PoolOnly`.

---

## API Contract

Endpoint:

```text
POST /intents
Content-Type: application/json
```

Request body uses the same fields as the current JSON-lines TCP source:

```json
{
  "accountId": "0000000000000000000000000000000000000000000000000000000000000000",
  "originalIntentDigest": "1111111111111111111111111111111111111111111111111111111111111111",
  "inputAsset": "00",
  "outputAsset": "policy_id_hex_followed_by_asset_name_hex",
  "leavingAmount": 1000000,
  "expectedArrivingAmount": 900000,
  "feeLovelace": 100000,
  "targetNonceSlot": 0,
  "targetNonceValue": 42,
  "operatorKeyHash": "22222222222222222222222222222222222222222222222222222222",
  "auth": {
    "type": "sig",
    "prefix": "",
    "postfix": "",
    "signature": "64_byte_signature_hex",
    "updateProof": ""
  }
}
```

Success response:

```json
{
  "status": "accepted",
  "reason": null
}
```

Error responses:

```json
{
  "status": "rejected",
  "reason": "missingAccount"
}
```

Recommended status mapping:

- `202 Accepted`: intent parsed, admitted, and sent into execution stream.
- `400 Bad Request`: JSON parse failure or malformed hex/signature/account id.
- `409 Conflict`: stale nonce or missing/currently unavailable account.
- `422 Unprocessable Entity`: admission rejected by business rules, such as non-ADA input or insufficient account balance.
- `503 Service Unavailable`: internal event channel is closed or currently full.

---

## Task 1: Make Wire Intent Parsing Reusable

**Files:**
- Modify: `green-order-cardano-agent/src/intent_source.rs`

**Step 1: Write the failing test**

Add tests in the existing `#[cfg(test)] mod tests`:

```rust
#[test]
fn parses_wire_sig_intent_into_raw_intent() {
    let json = serde_json::json!({
        "accountId": hex::encode([1u8; 32]),
        "originalIntentDigest": hex::encode([2u8; 32]),
        "inputAsset": "00",
        "outputAsset": hex::encode(AssetClass::Token(token(1)).to_bytes()),
        "leavingAmount": 1_000_000,
        "expectedArrivingAmount": 900_000,
        "feeLovelace": 100_000,
        "targetNonceSlot": 0,
        "targetNonceValue": 10,
        "operatorKeyHash": hex::encode([7u8; 28]),
        "auth": {
            "type": "sig",
            "prefix": "",
            "postfix": "",
            "signature": hex::encode([3u8; 64]),
            "updateProof": ""
        }
    });

    let wire: WireGreenIntent = serde_json::from_value(json).unwrap();
    let raw = RawGreenIntent::try_from(wire).unwrap();

    assert_eq!(account_id(1), raw.account_id);
    assert_eq!([2u8; 32], raw.original_intent_digest);
    assert_eq!(AssetClass::Native, raw.intention.input_asset);
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p green-order-cardano-agent intent_source::tests::parses_wire_sig_intent_into_raw_intent -- --nocapture
```

Expected: fail if `WireGreenIntent` and parsing helpers are private or token byte helper is missing.

**Step 3: Write minimal implementation**

Expose only what HTTP needs:

```rust
pub(crate) struct WireGreenIntent { ... }
pub(crate) enum WireGreenAuth { ... }
```

If needed, add a narrow helper:

```rust
pub(crate) fn parse_wire_intent(wire: WireGreenIntent) -> Result<RawGreenIntent, ()> {
    RawGreenIntent::try_from(wire)
}
```

Keep admission logic unchanged.

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test -p green-order-cardano-agent intent_source::tests::parses_wire_sig_intent_into_raw_intent -- --nocapture
```

Expected: PASS.

**Step 5: Commit**

```bash
git add green-order-cardano-agent/src/intent_source.rs
git commit -m "Prepare green intent parsing for HTTP ingress"
```

---

## Task 2: Add HTTP Intent Source Module

**Files:**
- Create: `green-order-cardano-agent/src/http_intent_source.rs`
- Modify: `green-order-cardano-agent/src/main.rs`

**Step 1: Write the failing tests**

Add unit tests in `http_intent_source.rs` for pure handler behavior using an in-memory channel:

```rust
#[tokio::test]
async fn post_intent_accepts_valid_full_fill_intent() {
    // Build AccountIndex with one current Aleph account.
    // Submit a valid WireGreenIntent through the handler.
    // Assert HTTP status is 202.
    // Assert one GreenIntentEvent was sent to the mpsc receiver.
}

#[tokio::test]
async fn post_intent_rejects_missing_account() {
    // Empty AccountIndex.
    // Submit a syntactically valid intent.
    // Assert HTTP status is 409.
    // Assert no GreenIntentEvent was sent.
}

#[tokio::test]
async fn post_intent_rejects_path_auth_when_partial_disabled() {
    // Account exists.
    // Submit path-auth intent with default GreenOrdersConfig.
    // Assert HTTP status is 422.
    // Assert no GreenIntentEvent was sent.
}

#[tokio::test]
async fn post_intent_returns_503_when_event_channel_is_full() {
    // Build AccountIndex with one current Aleph account.
    // Create bounded mpsc channels with capacity 1.
    // Fill the selected pair channel before submitting the request.
    // Submit a valid WireGreenIntent through the handler.
    // Assert HTTP status is 503.
}
```

Keep fixture helpers small by copying the account fixture pattern from `intent_source.rs` tests if needed.

**Step 2: Run tests to verify they fail**

Run:

```bash
cargo test -p green-order-cardano-agent http_intent_source -- --nocapture
```

Expected: FAIL because module and handler do not exist.

**Step 3: Implement the minimal module**

Create `green-order-cardano-agent/src/http_intent_source.rs`:

```rust
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use futures::channel::mpsc;
use serde::Serialize;
use spectrum_offchain::domain::Has;
use spectrum_offchain::partitioning::Partitioned;
use spectrum_offchain_cardano::data::pair::PairId;
use spectrum_offchain_cardano::deployment::DeployedScriptInfo;

use bloom_offchain_cardano::orders::green::ALEPH_ACCOUNT_VALIDATOR;

use crate::account_index::AccountIndex;
use crate::intent_source::{
    admitted_intent_to_event, admit_green_intent, AdmissionError, GreenIntentEvent, GreenOrdersConfig,
    WireGreenIntent,
};

#[derive(Clone)]
pub struct HttpIntentState<C> {
    pub account_index: Arc<Mutex<AccountIndex>>,
    pub ctx: C,
    pub config: GreenOrdersConfig,
    pub events: Partitioned<4, PairId, mpsc::Sender<GreenIntentEvent>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentResponse {
    pub status: &'static str,
    pub reason: Option<&'static str>,
}

pub fn router<C>(state: HttpIntentState<C>) -> Router
where
    C: Has<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>> + Clone + Send + Sync + 'static,
{
    Router::new().route("/intents", post(post_intent::<C>)).with_state(state)
}
```

Implement `post_intent`:

```rust
async fn post_intent<C>(
    State(mut state): State<HttpIntentState<C>>,
    Json(wire): Json<WireGreenIntent>,
) -> (StatusCode, Json<IntentResponse>)
where
    C: Has<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>> + Clone + Send + Sync + 'static,
{
    let Ok(raw) = parse_wire_intent(wire) else {
        return rejected(StatusCode::BAD_REQUEST, "malformedIntent");
    };

    let admitted = {
        let index = state.account_index.lock().expect("account index lock poisoned");
        admit_green_intent(raw, &index, &state.ctx, state.config)
    };

    let admitted = match admitted {
        Ok(admitted) => admitted,
        Err(err) => return rejected(status_for_admission_error(err), reason_for_admission_error(err)),
    };

    let event = admitted_intent_to_event(admitted);
    let pair = event.0;
    if state.events.get_mut(pair).try_send(event).is_err() {
        return rejected(StatusCode::SERVICE_UNAVAILABLE, "eventChannelUnavailable");
    }

    (
        StatusCode::ACCEPTED,
        Json(IntentResponse {
            status: "accepted",
            reason: None,
        }),
    )
}
```

Add small helpers:

```rust
fn rejected(status: StatusCode, reason: &'static str) -> (StatusCode, Json<IntentResponse>) { ... }
fn status_for_admission_error(err: AdmissionError) -> StatusCode { ... }
fn reason_for_admission_error(err: AdmissionError) -> &'static str { ... }
```

Use non-blocking `try_send`, not `send(...).await`, so the HTTP request cannot hang indefinitely when the bounded engine channel is full.

**Step 4: Register module in main**

Modify `green-order-cardano-agent/src/main.rs`:

```rust
mod http_intent_source;
```

Do not start the server yet in this task.

**Step 5: Run tests to verify they pass**

Run:

```bash
cargo test -p green-order-cardano-agent http_intent_source -- --nocapture
```

Expected: PASS.

**Step 6: Commit**

```bash
git add green-order-cardano-agent/src/http_intent_source.rs green-order-cardano-agent/src/main.rs
git commit -m "Add HTTP green intent handler"
```

---

## Task 3: Add HTTP Config And Startup

**Files:**
- Modify: `green-order-cardano-agent/src/intent_source.rs`
- Modify: `green-order-cardano-agent/src/config.rs`
- Modify: `green-order-cardano-agent/src/main.rs`
- Modify: `green-order-cardano-agent/resources/preprod.config.json`
- Modify: `README.md`

**Step 1: Write the failing config test**

Add test in `green-order-cardano-agent/src/intent_source.rs`:

```rust
#[test]
fn parses_http_intent_source_config() {
    let config: GreenOrdersConfig = serde_json::from_value(serde_json::json!({
        "allowPartial": false,
        "intentSource": {
            "listenAddr": null,
            "httpListenAddr": "127.0.0.1:9031"
        }
    }))
    .unwrap();

    assert_eq!("127.0.0.1:9031".parse().ok(), config.intent_source.http_listen_addr);
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p green-order-cardano-agent intent_source::tests::parses_http_intent_source_config -- --nocapture
```

Expected: FAIL because `http_listen_addr` does not exist.

**Step 3: Add config field**

Modify `IntentSourceConfig`:

```rust
pub struct IntentSourceConfig {
    pub listen_addr: Option<SocketAddr>,
    pub http_listen_addr: Option<SocketAddr>,
}
```

Update manual deserialize `Repr`:

```rust
#[serde(default)]
listen_addr: Option<SocketAddr>,
#[serde(default)]
http_listen_addr: Option<SocketAddr>,
```

Keep `listenAddr` for the current TCP source. Use `httpListenAddr` for the new HTTP source.

**Step 4: Reject non-loopback HTTP binding while unauthenticated**

Extend `CheckIntegrity for AppConfig` in `green-order-cardano-agent/src/config.rs`:

```rust
if let Some(addr) = self.green_orders.intent_source.http_listen_addr {
    if !addr.ip().is_loopback() {
        violations
            .0
            .push("Green HTTP intent source must bind to a loopback address".to_string());
    }
}
```

Add a focused unit test for this rule. If constructing a full `AppConfig` fixture is noisy, put the check in a small helper and test the helper:

```rust
#[test]
fn rejects_non_loopback_http_intent_source() {
    assert!(http_intent_source_bind_violation(Some("0.0.0.0:9031".parse().unwrap())).is_some());
    assert!(http_intent_source_bind_violation(Some("127.0.0.1:9031".parse().unwrap())).is_none());
}
```

**Step 5: Start HTTP source in main**

In `green-order-cardano-agent/src/main.rs`, import:

```rust
use crate::http_intent_source::{router as http_intent_router, HttpIntentState};
```

After the existing TCP intent source startup, add:

Before adding HTTP startup, update the existing TCP source call to clone the sender:

```rust
tokio::spawn(run_tcp_intent_source(
    intent_listen_addr,
    Arc::clone(&account_index),
    GreenScriptHashes::from(&green_deployment),
    config.green_orders,
    intent_pair_upd_snd.clone(),
));
```

Then add:

```rust
if let Some(http_listen_addr) = config.green_orders.intent_source.http_listen_addr {
    info!("Green HTTP intent source listening on {}", http_listen_addr);
    let app = http_intent_router(HttpIntentState {
        account_index: Arc::clone(&account_index),
        ctx: GreenScriptHashes::from(&green_deployment),
        config: config.green_orders,
        events: intent_pair_upd_snd.clone(),
    });
    tokio::spawn(async move {
        axum::Server::bind(&http_listen_addr)
            .serve(app.into_make_service())
            .await
            .expect("green HTTP intent source failed");
    });
}
```

If this repository uses Axum 0.7 instead of 0.6, use:

```rust
let listener = tokio::net::TcpListener::bind(http_listen_addr).await.unwrap();
axum::serve(listener, app).await.unwrap();
```

Choose the variant that matches `Cargo.lock`.

**Step 6: Update config**

Modify `green-order-cardano-agent/resources/preprod.config.json`:

```json
"greenOrders": {
  "allowPartial": false,
  "intentSource": {
    "listenAddr": null,
    "httpListenAddr": "127.0.0.1:9031"
  }
}
```

This makes HTTP the default local ingress and disables TCP by default.

**Step 7: Update README**

Replace the TCP JSON-lines section with:

```markdown
External signed intents enter through a local HTTP endpoint configured under
`greenOrders.intentSource.httpListenAddr`.

POST /intents accepts one JSON intent and returns 202 when admitted into the
execution stream.
```

Mention that the old `listenAddr` TCP source still exists for local testing but is not the default.

**Step 8: Run tests/checks**

Run:

```bash
cargo fmt
cargo test -p green-order-cardano-agent intent_source::tests::parses_http_intent_source_config -- --nocapture
cargo test -p green-order-cardano-agent http_intent_source -- --nocapture
cargo test -p green-order-cardano-agent config -- --nocapture
cargo check -p green-order-cardano-agent
```

Expected: all PASS / exit 0.

**Step 9: Commit**

```bash
git add green-order-cardano-agent/src/intent_source.rs green-order-cardano-agent/src/config.rs green-order-cardano-agent/src/main.rs green-order-cardano-agent/resources/preprod.config.json README.md
git commit -m "Wire HTTP green intent ingress"
```

---

## Task 4: Add One Manual Smoke Command To Docs

**Files:**
- Modify: `README.md`

**Step 1: Add curl example**

Add:

```bash
curl -sS -X POST http://127.0.0.1:9031/intents \
  -H 'content-type: application/json' \
  --data '{"accountId":"...","originalIntentDigest":"...","inputAsset":"00","outputAsset":"...","leavingAmount":1000000,"expectedArrivingAmount":900000,"feeLovelace":100000,"targetNonceSlot":0,"targetNonceValue":42,"operatorKeyHash":"...","auth":{"type":"sig","prefix":"","postfix":"","signature":"...","updateProof":""}}'
```

Expected accepted response:

```json
{"status":"accepted","reason":null}
```

**Step 2: Verify README still matches code names**

Run:

```bash
rg "httpListenAddr|POST /intents|listenAddr" README.md green-order-cardano-agent/src
```

Expected: README references `httpListenAddr`, `POST /intents`, and describes `listenAddr` as legacy TCP/local testing.

**Step 3: Commit**

```bash
git add README.md
git commit -m "Document HTTP green intent ingress"
```

---

## Final Verification

Run the full focused verification set:

```bash
cargo fmt
cargo check -p green-order-cardano-agent
cargo test -p green-order-cardano-agent -- --nocapture
cargo test -p bloom-offchain-cardano green -- --nocapture
```

Expected:

- `green-order-cardano-agent` compiles.
- All green-order-agent tests pass.
- Existing Bloom/Cardano green-order tests pass.
- No live node or mempool E2E is required for this plan.

---

## Security Notes

This endpoint must bind to `127.0.0.1` by default. Without authentication, binding to `0.0.0.0` would let external callers push arbitrary syntactically valid intents into the local execution stream. Admission checks still protect account state and non-ADA/partial constraints, but they are not a network access-control layer.
