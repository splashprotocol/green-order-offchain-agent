# Green Order Offchain Agent Architecture

Repository: https://github.com/splashprotocol/green-order-offchain-agent

The Green Order offchain agent is a Rust/Cardano service that accepts signed Aleph Green Order intents, observes Aleph account and Royalty V1 pool state, builds execution plans, and submits Cardano preprod transactions. The repository also contains a TypeScript SDK, preprod E2E scripts, and a one-command Catalyst demo wrapper.

## Components

- Agent entry point: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/main.rs
- Runtime config model: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/config.rs
- Deployment references: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/deployment.rs
- HTTP intent and account binding API: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/http_intent_source.rs
- Green Order admission and conversion: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/intent_source.rs
- Aleph account indexing: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/account_index.rs
- Account-store and continuation planning: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/account_store.rs
- MPF helpers: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/mpf.rs
- TypeScript SDK: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/sdk/typescript
- Preprod E2E scripts: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/green-order-cardano-agent/e2e/preprod
- Catalyst demo wrapper: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/demo

## Runtime Flow

1. The agent starts from `green-order-cardano-agent/src/main.rs` and loads the deployment/config files from `green-order-cardano-agent/resources`.
2. Ledger and mempool events are processed through the existing Spectrum/Cardano infrastructure.
3. Aleph account outputs are indexed by `account_index.rs`.
4. Royalty V1 pool outputs and separator/funding state are observed by the execution layer.
5. External Green Order intents are submitted to the loopback HTTP API.
6. The agent validates the intent wire fields, account binding, assets, nonce target, operator key, and signature payload.
7. The planner constructs full-fill execution by default. When the run-local partial-enabled demo config is used, it also constructs partial-fill execution using the observed Aleph account and Royalty V1 pool state.
8. For partial fills under that generated config, the continuation state is persisted through the Aleph account store-root transition.
9. The transaction is submitted and later observed back through chain sync.

## HTTP And SDK Communication

Auditors can see the public reviewer-facing API in the README:

https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/README.md

The local HTTP API is loopback-only by design. It exposes:

- `POST /accounts/bind`: bind an externally known Aleph `accountId` to an observed account UTxO.
- `POST /intents`: submit one signed Green Order intent.
- monitoring/readiness/account endpoints used by the SDK and demo scripts.

The TypeScript SDK wraps these calls:

- SDK client: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/client.ts
- SDK intent helpers: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/intent.ts
- SDK account helpers: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/account.ts
- SDK HMAC helpers: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/hmac.ts

The preprod script `11-query-agent-via-sdk.ts` demonstrates read-only SDK communication after an account is bound:

https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/11-query-agent-via-sdk.ts

The bad-HMAC script verifies authenticated SDK protection:

https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/12-query-agent-with-bad-hmac.ts

## Demo And Evidence Flow

The close-out video should use the Catalyst wrapper:

https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/demo/catalyst-demo.sh

The wrapper creates fresh preprod wallets, asks the reviewer to fund one generated address with tADA, prepares account/pool/separator prerequisites, starts the agent, submits full-fill Green Orders and partial-fill Green Orders under the generated partial-enabled config, verifies execution, and writes a run-local auditor report.
