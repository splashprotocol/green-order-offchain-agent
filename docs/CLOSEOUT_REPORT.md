New Proof of Achievement for Milestone 5

Milestone title: Project close-out

Project repository: https://github.com/splashprotocol/green-order-offchain-agent

Status: Draft close-out package pending final public video URL.

Close-out video: Pending publication. Replace this line with the final YouTube or Vimeo URL before Catalyst submission.

Milestone Output 1 — Completed development and testing

Description: The project delivered a Green Order offchain agent for Cardano preprod. The agent accepts signed Aleph Green Order intents, binds observed Aleph accounts, indexes account state, plans execution against Royalty V1 pools, handles default full-fill flows and partial-fill flows under the generated partial-enabled demo config, persists continuation state, and exposes SDK-accessible HTTP endpoints for intent submission, account binding, and monitoring.

The delivered repository contains the Rust agent, TypeScript SDK, preprod E2E scripts, Catalyst demo wrapper, tests, and auditor documentation.

Evidence:
- Public milestone branch: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api
- Agent entry point: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/main.rs
- HTTP intent and account API: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/http_intent_source.rs
- Intent admission and conversion: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/intent_source.rs
- Account indexing: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/account_index.rs
- Account-store and MPF planning: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/account_store.rs
- Deployment loader: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/deployment.rs
- TypeScript SDK: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/sdk/typescript
- Preprod E2E scripts: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/green-order-cardano-agent/e2e/preprod
- Catalyst demo wrapper: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/demo
- Rust account-index and account-store tests embedded in implementation files:
  - https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/account_index.rs
  - https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/account_store.rs
- HTTP intent-source tests: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/http_intent_source.rs
- SDK tests: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/sdk/typescript/test
- Preprod script tests: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/green-order-cardano-agent/e2e/preprod/tests
- Demo wrapper tests: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/demo/tests

How the Acceptance Criteria can be verified:
- Verify that the public branch opens at https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api.
- Inspect the linked Rust agent files to confirm the Green Order agent entry point, HTTP API, intent admission, account indexing, account-store planning, and deployment loading are present.
- Inspect `sdk/typescript` to confirm the SDK is present and exposes client, account, intent, and HMAC helpers.
- Inspect the preprod E2E and demo directories to confirm the auditor-facing verification scripts are present.
- Run the local checks listed in `docs/TESTING.md`:

```sh
cargo test -q -p green-order-cardano-agent account_index
cargo test -q -p green-order-cardano-agent account_store
cargo test -q -p green-order-cardano-agent http_intent_source
npm --prefix sdk/typescript test
bash demo/tests/run-tests.sh
```

- Run the preprod checks listed in `docs/TESTING.md` when a preprod node/socket and funding are available.

Milestone Output 2 — Comprehensive documentation published as Markdown

Description: The close-out package includes Markdown documentation for the agent architecture, testing and verification, close-out demo recording, and the final project completion report. The documentation explains how the agent works, how SDK communication is exercised, how to run local and live preprod checks, and where auditors can inspect the delivered code.

Evidence:
- Architecture documentation: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/ARCHITECTURE.md
- Testing and verification documentation: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/TESTING.md
- Close-out video demo script: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/DEMO.md
- Russian close-out video recording guide: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/VIDEO_GUIDE_RU.md
- Project completion report: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/CLOSEOUT_REPORT.md
- Repository README: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/README.md
- Catalyst demo README: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/demo/README.md
- Catalyst auditor guide: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/demo/AUDITOR.md
- Demo script reference: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/demo/SCRIPT_REFERENCE.md

How the Acceptance Criteria can be verified:
- Verify that each documentation link above opens from the public `sdk-api` branch.
- Verify that `docs/ARCHITECTURE.md` explains the agent runtime, account indexing, SDK communication, and demo wrapper.
- Verify that `docs/TESTING.md` lists local Rust, SDK, Deno/preprod, and demo verification commands.
- Verify that `docs/DEMO.md` provides a video recording flow covering repository links, agent files, SDK communication, verification commands, and the live preprod demo.
- Verify that the README links the close-out documentation from the repository entry point.

Milestone Output 3 — Project completion report

Description: This document is the GitHub-hosted completion report. It summarizes the delivered project, maps milestone outputs to evidence, explains how auditors can verify acceptance criteria, links to source code and tests, and states the close-out video publication status.

Evidence:
- Completion report: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/CLOSEOUT_REPORT.md
- Public repository: https://github.com/splashprotocol/green-order-offchain-agent
- Public milestone branch: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api

How the Acceptance Criteria can be verified:
- Verify that this report is accessible from the public repository at the link above.
- Verify that each milestone output in this report includes a description, evidence links, and concrete verification steps.
- Verify that all repository links in this report point to the public repository at https://github.com/splashprotocol/green-order-offchain-agent.
- Verify that this report contains no wallet seeds, private keys, Blockfrost API keys, HMAC secrets, local filesystem paths, private node socket paths, or generated run state.
- Verify that the video URL status is explicitly marked as pending until the final public YouTube or Vimeo URL is available.

Milestone Output 4 — Video demonstration

Description: The close-out package includes a Markdown recording script for the final video demonstration. The video should show the repository, explain the delivered Green Order agent, demonstrate SDK/API communication, run or replay verification commands, show the live preprod demo flow, and end with the completion report. The final Catalyst submission is not complete until the video is published and the pending video line in this report is replaced with the public URL.

Evidence:
- Close-out video demo script: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/DEMO.md
- Russian close-out video recording guide: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/VIDEO_GUIDE_RU.md
- Repository README close-out entry point: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/README.md
- Catalyst demo wrapper: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/demo/catalyst-demo.sh
- Catalyst demo README: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/demo/README.md
- Catalyst auditor guide: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/demo/AUDITOR.md

How the Acceptance Criteria can be verified:
- Verify that `docs/DEMO.md` gives a concrete recording script and identifies the files and commands to show.
- Verify that the script covers the agent, SDK communication, local verification, and live preprod demo.
- Verify that `demo/catalyst-demo.sh` exists and is linked from the evidence above.
- Before final Catalyst submission, replace the pending video line at the top of this report with the final public video URL and verify that the URL is accessible without private permissions.

SDK and off-chain communication evidence:
- SDK client: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/client.ts
- SDK intent helpers: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/intent.ts
- SDK account helpers: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/account.ts
- SDK HMAC helpers: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/hmac.ts
- SDK monitoring query script: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/11-query-agent-via-sdk.ts
- Negative HMAC script proving bad credentials are rejected: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/12-query-agent-with-bad-hmac.ts

Final link and sensitivity verification:
- All GitHub links in this report point to the public repository at https://github.com/splashprotocol/green-order-offchain-agent.
- Public source links target the `sdk-api` branch.
- This Proof of Achievement contains public repository links, public source-code references, test instructions, and public run instructions.
- This Proof of Achievement does not include wallet seeds, private keys, Blockfrost API keys, HMAC secrets, local filesystem paths, private node socket paths, or generated run state.
