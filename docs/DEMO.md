# Close-Out Video Demo Script

Repository: https://github.com/splashprotocol/green-order-offchain-agent

Use this script when recording the milestone 5 close-out video. The video should show the code, explain how it works, demonstrate SDK communication, and run the verifier/demo flow.

## 1. Show Repository Entry Point

Open:

- Repository: https://github.com/splashprotocol/green-order-offchain-agent
- README: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/README.md
- Architecture docs: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/ARCHITECTURE.md
- Testing docs: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/TESTING.md
- Completion report: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/CLOSEOUT_REPORT.md

## 2. Explain The Agent

Open:

- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/main.rs
- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/http_intent_source.rs
- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/account_index.rs
- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/account_store.rs

Explain that the agent observes preprod chain state, binds Aleph accounts, accepts signed Green Order intents through the loopback HTTP API, plans full-fill execution by default, or partial-fill execution when the run-local partial-enabled demo config is used against Royalty V1 pools, and verifies successor account/store state.

## 3. Explain SDK Communication

Open:

- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/client.ts
- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/11-query-agent-via-sdk.ts
- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/12-query-agent-with-bad-hmac.ts

Show that the SDK calls monitoring, account, account-status, bind, and intent endpoints. Mention that the preprod harness uses a run-local HMAC secret and includes a negative bad-HMAC check.

## 4. Run Local Verification

Run:

```sh
cargo test -q -p green-order-cardano-agent account_index
cargo test -q -p green-order-cardano-agent account_store
cargo test -q -p green-order-cardano-agent http_intent_source
npm --prefix sdk/typescript test
bash demo/tests/run-tests.sh
```

If time is limited, run the first Rust command, the SDK tests, and the demo wrapper tests on camera, then show `docs/TESTING.md` for the full command list.

## 5. Run Or Explain Live Preprod Flow

Open:

- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/demo/README.md
- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/demo/AUDITOR.md
- https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/demo/SCRIPT_REFERENCE.md

Run, or show a completed run report:

```sh
./demo/catalyst-demo.sh
```

Explain:

1. Fresh wallets are generated.
2. Reviewer funds the generated preprod address.
3. The wrapper prepares operator/pool/account/separator prerequisites.
4. The agent starts with run-local config.
5. The full-fill intent is submitted and verified.
6. The partial-fill intent is submitted under the generated partial-enabled config and continuation state is verified.
7. The generated `demo-report.md` contains transaction hashes and checkpoints.

## 6. Close With Completion Report

Open:

https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/CLOSEOUT_REPORT.md

After the video is published, replace the pending video line in the report with the final YouTube or Vimeo URL.
