# Project Completion Report

Project repository: https://github.com/splashprotocol/green-order-offchain-agent

Status: Draft close-out package pending final public video URL.

Close-out video: Pending publication. Replace this line with the final YouTube or Vimeo URL before Catalyst submission.

## Summary

The project delivered a Green Order offchain agent for Cardano preprod. The agent accepts signed Aleph Green Order intents, binds observed Aleph accounts, indexes account state, plans execution against Royalty V1 pools, handles default full-fill flows and partial-fill flows under the generated partial-enabled demo config, persists continuation state, and exposes SDK-accessible HTTP endpoints for intent submission, account binding, and monitoring.

The repository contains Rust agent code, TypeScript SDK code, preprod E2E scripts, Catalyst demo scripts, tests, and Markdown documentation for auditors and operators.

## Milestone 5 Outputs

### 1. Completed Development And Testing

Completed components:

- Agent entry point: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/main.rs
- HTTP intent/account API: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/http_intent_source.rs
- Intent admission/conversion: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/intent_source.rs
- Account indexing: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/account_index.rs
- Account-store and MPF planning: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/account_store.rs
- Deployment loader: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/deployment.rs
- TypeScript SDK: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/sdk/typescript
- Preprod E2E scripts: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/green-order-cardano-agent/e2e/preprod
- Catalyst demo wrapper: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/demo

Testing evidence:

- Rust account-index and account-store tests are embedded in the implementation files:
  https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/account_index.rs
  https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/account_store.rs
- HTTP intent-source tests:
  https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/http_intent_source.rs
- SDK tests:
  https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/sdk/typescript/test
- Preprod script tests:
  https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/green-order-cardano-agent/e2e/preprod/tests
- Demo wrapper tests:
  https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/demo/tests

Auditors can reproduce the local verification commands listed in:

https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/TESTING.md

### 2. Comprehensive Documentation Published As Markdown

Documentation:

- Architecture: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/ARCHITECTURE.md
- Testing and verification: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/TESTING.md
- Close-out video demo script: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/DEMO.md
- Project completion report: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/CLOSEOUT_REPORT.md
- Repository README: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/README.md
- Catalyst demo README: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/demo/README.md
- Catalyst auditor guide: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/demo/AUDITOR.md
- Demo script reference: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/demo/SCRIPT_REFERENCE.md

### 3. Project Completion Report

This document is the GitHub-hosted completion report draft. It summarizes the delivered code, explains how the system works, links to tests and documentation, and gives auditors reproduction instructions.

### 4. Video Demonstration

The close-out video should follow:

https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/DEMO.md

The video must be published on an accessible platform such as YouTube or Vimeo. This draft is not ready for final Catalyst submission until the pending line at the top of this report is replaced with the final public URL.

## How The System Works

The Rust agent observes Cardano preprod chain data and maintains local views of Aleph account UTxOs and Royalty V1 pool UTxOs. Before accepting an intent for an externally known `accountId`, the agent binds that account id to an observed Aleph account output through the loopback-only account binding endpoint.

Signed Green Order intents enter through the local HTTP API or the TypeScript SDK. The agent validates the request, checks account binding and operator configuration, converts the wire intent into the runtime model, builds the required transaction inputs/outputs, and submits execution against a Royalty V1 pool. For partial fills under the generated partial-enabled demo config, the residual intent and store-root transition are persisted and verified.

The Catalyst demo wrapper combines the full flow into one reviewer-facing command. It creates fresh wallets, waits for preprod funding, prepares on-chain setup, starts the agent, submits full-fill smoke tests and partial-fill smoke tests under the generated partial-enabled demo config, verifies execution, and writes a report with transaction hashes and checkpoints.

## SDK And Off-Chain Communication

The TypeScript SDK communicates with the loopback agent API. It can:

1. Query monitoring readiness and summary.
2. Query account status.
3. Bind an observed Aleph account output to an `accountId`.
4. Submit a signed Green Order intent.
5. Use HMAC authentication when configured.

The SDK query script demonstrates read-only communication:

https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/11-query-agent-via-sdk.ts

The negative HMAC script proves bad credentials are rejected:

https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/12-query-agent-with-bad-hmac.ts

## Acceptance Criteria Mapping

1. Project completion report meets funding rules and requirements:
   This draft provides the project summary, delivered components, documentation links, test instructions, SDK explanation, and video placeholder. It must receive the final public video URL before submission.

2. Video demonstration is published on an accessible platform:
   The recording script is provided in `docs/DEMO.md`. This acceptance criterion remains pending until the video is published and the final URL is inserted.

3. All provided links to repositories and documents are working:
   The report links only to files in the public repository
   [splashprotocol/green-order-offchain-agent](https://github.com/splashprotocol/green-order-offchain-agent).

## Security And Privacy

The report and documentation contain public repository links, public source-code references, and public run instructions. They do not include wallet seeds, private keys, API keys, local filesystem paths, private node socket paths, or generated run state.
