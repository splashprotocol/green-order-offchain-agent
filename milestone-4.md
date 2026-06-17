New Proof of Achievement for Milestone 4

Milestone title: SDK development

Milestone Output 1 — SDK codebase with necessary functionalities for integration

Description: The delivered TypeScript SDK is the developer-facing integration layer for Green Orders. It provides helpers to build wire-compatible Green Order intent payloads, compute the Aleph-compatible intention digest, sign intents through an injected wallet/signer, submit signed intents to a Green Order agent, bind Aleph accounts, query account status, fetch account summaries, query readiness, and monitor the agent. The milestone branch also includes the Green Order agent HTTP routes required by the SDK, including submit, bind, account status, account summary, and monitoring endpoints.

Evidence:
- Public milestone branch: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api
- TypeScript SDK package: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/sdk/typescript
- SDK package manifest and scripts: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/package.json
- SDK public exports: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/index.ts
- Agent HTTP client with submit, bind, status, account, readiness, and monitoring methods: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/client.ts
- Green Order intent building, Aleph intention CBOR/digest, and signing helpers: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/intent.ts
- HMAC request signing helpers for operator HTTP API protection: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/hmac.ts
- Account binding and account-status payload helpers: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/account.ts
- Agent HTTP routes for `POST /intents`, `POST /accounts/bind`, `GET /accounts/status`, `GET /accounts/:account_id`, `GET /monitoring/summary`, and `GET /monitoring/readiness`: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/http_intent_source.rs
- Pull request submitted to the open GitHub repository: https://github.com/splashprotocol/green-order-offchain-agent/pull/1

How the Acceptance Criteria can be verified:
- Acceptance criterion 1 says the code must be written and submitted to an open GitHub repository. Verify that the public branch opens at https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api and that the SDK source files above are visible in `sdk/typescript/src`.
- Verify the SDK API surface by inspecting `sdk/typescript/src/index.ts`, `client.ts`, `intent.ts`, `account.ts`, and `hmac.ts`.
- Verify the agent routes consumed by the SDK by inspecting `green-order-cardano-agent/src/http_intent_source.rs`.
- Verify the current branch and pull request head directly from GitHub PR #1; the evidence links in this report use the public `sdk-api` branch so they resolve to the latest pushed milestone evidence.
- Verify the submitted pull request at https://github.com/splashprotocol/green-order-offchain-agent/pull/1 is public, uses the `sdk-api` branch, and contains the SDK source, agent API route, test, E2E, and documentation changes referenced in this report.

SDK-based preprod integration evidence:
- SDK-based preprod E2E runner: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/run-preprod-e2e.sh
- SDK-based account binding script: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/03-create-aleph-account-and-bind.ts
- SDK-based full-fill submit script: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/04-submit-green-order-smoke.ts
- SDK monitoring query script that calls `getMonitoringReadiness`, `getMonitoringSummary`, `getAccount`, and `getAccountStatus`: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/11-query-agent-via-sdk.ts
- SDK negative HMAC probe that intentionally signs with the wrong secret and requires `403 Forbidden`: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/12-query-agent-with-bad-hmac.ts

Preprod SDK execution evidence from the latest local SDK E2E run:
- Network: Cardano preprod
- Run mode: forced fresh E2E state with the funded preprod batcher wallet reused
- HMAC mode: enabled for SDK requests to the Green Order agent
- Account id: `eda3e61ab5b686850f512170b0957f5f0480c74bf1cb59694d3919e79f823288`
- Initial Aleph account creation transaction: `05c7cd253f69989a01e3ad09c567c434c073ed37334a040272153b3b86fe0561`
- Initial Aleph account output observed by the agent: `05c7cd253f69989a01e3ad09c567c434c073ed37334a040272153b3b86fe0561#0`
- Operator funding transaction: `daabeb39eb621ca9b93ac1794bd48d15f2aece4da5ee989ca3b84ade44548426`
- Royalty V1 pool creation transaction: `b5dbe13f0cae051b9a3a91f7f4d608bd6bd01fba58f33e816835485e6222075b`
- Separator funding transaction: `294316acb7662c5ae9df3134ea180ca9830b8211b85af6dbb68cc75a84cd00f5`
- Full-fill submitted intent digest: `45990670d8e2dc9ec8d37b039358819276cd497a524929b8bce14aec3d4e018b`
- Full-fill execution transaction built by the agent: `07103b7658edaa1853b8ee0ca6484e495d455c8509502d1c64d4c26dd5508b26`
- Successor Aleph account output after execution: `07103b7658edaa1853b8ee0ca6484e495d455c8509502d1c64d4c26dd5508b26#0`
- Successor Royalty V1 pool output after execution: `07103b7658edaa1853b8ee0ca6484e495d455c8509502d1c64d4c26dd5508b26#2`
- SDK monitoring query result: readiness `status: ok`, account status `current: true`, `pending: false`, `persisted: false`, `predicted: false`, `unbound: false`
- Negative HMAC probe result: `bad_hmac_check=passed`, `bad_hmac_status=403`, `bad_hmac_reason=invalidHmacSignature`
- Public explorer links:
  - https://preprod.cexplorer.io/tx/05c7cd253f69989a01e3ad09c567c434c073ed37334a040272153b3b86fe0561
  - https://preprod.cexplorer.io/tx/daabeb39eb621ca9b93ac1794bd48d15f2aece4da5ee989ca3b84ade44548426
  - https://preprod.cexplorer.io/tx/b5dbe13f0cae051b9a3a91f7f4d608bd6bd01fba58f33e816835485e6222075b
  - https://preprod.cexplorer.io/tx/294316acb7662c5ae9df3134ea180ca9830b8211b85af6dbb68cc75a84cd00f5
  - https://preprod.cexplorer.io/tx/07103b7658edaa1853b8ee0ca6484e495d455c8509502d1c64d4c26dd5508b26

Auditor-style command to verify the SDK-based preprod flow:

```sh
cd green-order-cardano-agent/e2e/preprod
CARDANO_NODE_SOCKET_PATH=/absolute/path/to/preprod/node.socket \
E2E_CHECK_BAD_HMAC=1 \
FORCE_E2E_STATE=1 \
bash ./run-preprod-e2e.sh
```

The runner asks for a preprod Blockfrost project id when one is not already configured. Pressing Enter uses Koios. Secrets are read from the environment or generated ignored local files; the scripts do not hardcode API keys, wallet seeds, private keys, or HMAC secrets. For the run-local agent path, the runner generates a fresh HMAC secret for each run, stores it only in ignored `.state/preprod-hmac.env`, writes HMAC auth into the ignored local agent config with the fixed non-secret key id `preprod-e2e`, signs SDK requests, and verifies that an intentionally bad SDK HMAC request is rejected with `403 Forbidden`.

Milestone Output 2 — Unit test coverage for the SDK

Description: The SDK includes unit tests covering account route payloads, account status query construction, agent client request construction, rejected response handling, account summary and monitoring routes, HMAC canonicalization and headers, ADA/native asset wire encoding, Aleph-compatible intention digest generation, intent JSON payload generation, and signer injection.

Evidence:
- SDK unit test directory: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/sdk/typescript/test
- Account helper tests: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/test/account.test.ts
- Agent client tests: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/test/client.test.ts
- HMAC request signing tests: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/test/hmac.test.ts
- Intent encoding, digest, payload, and signer tests: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/test/intent.test.ts
- SDK test coverage documentation: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/sdk/test-coverage.md
- E2E tests proving the runner builds the SDK before SDK use, starts/stops the agent, queries SDK monitoring after smoke execution, prompts for Blockfrost without hardcoding it, and isolates forced fresh wallet state: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/tests/runner_agent_setup.test.ts
- SDK response parser tests for monitoring and account status shapes: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/tests/sdk_agent.test.ts
- Agent route tests proving valid HMAC headers are accepted and unsigned, mismatched body-hash, and invalid-signature SDK requests are rejected with `403 Forbidden`: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/http_intent_source.rs

How the Acceptance Criteria can be verified:
- Acceptance criterion 2 says the code must be covered with unit tests accessible through the open GitHub repository. Verify that the SDK test files above are visible in the public repository.
- Run the SDK unit tests from the repository root:

```sh
npm --prefix sdk/typescript test
```

Expected result from local verification on branch `sdk-api`:
- `17` SDK tests passed.
- `0` SDK tests failed.

- Run the preprod TypeScript/Deno support checks:

```sh
cd green-order-cardano-agent/e2e/preprod
deno task check
deno test --no-lock --allow-run --allow-read --allow-write --allow-env tests
```

Expected result from local verification on branch `sdk-api`:
- Preprod TypeScript/Deno check passed.
- Preprod E2E support tests passed: `47` passed, `0` failed.

Milestone Output 3 — SDK documentation published as Markdown on GitHub

Description: The SDK is documented in Markdown files available in the open GitHub repository. The documentation explains the TypeScript SDK package, installation and test commands, build/sign/submit usage, account monitoring, HMAC request signing, the SDK HTTP API contract, test coverage, and concrete examples. There is no separate GitBook publication for this milestone; the audit evidence is the Markdown documentation in the public repository.

Evidence:
- SDK README: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/README.md
- SDK documentation directory: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/docs/sdk
- TypeScript SDK API documentation: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/sdk/typescript.md
- Agent API contract used by the SDK: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/sdk/api-contract.md
- SDK test coverage documentation: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/sdk/test-coverage.md
- Full-fill intent example: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/sdk/examples/full-fill-intent.md
- Account monitoring example: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/sdk/examples/monitor-account.md
- Preprod SDK E2E README with auditor run instructions, Blockfrost/Koios provider behavior, forced fresh wallet state, HMAC env behavior, and SDK monitoring query instructions: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/README.md
- Repository README entry point: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/README.md

How the Acceptance Criteria can be verified:
- Acceptance criterion 3 says the codebase must be documented and accessible as Markdown files through the open GitHub repository. Verify each documentation link above opens from the public `sdk-api` branch.
- Note: no separate GitBook URL is provided because the SDK documentation is currently published in the GitHub repository as Markdown.
- Verify that `docs/sdk/typescript.md` documents the SDK APIs and signer model.
- Verify that `docs/sdk/api-contract.md` documents the agent HTTP routes wrapped by the SDK.
- Verify that `docs/sdk/examples/full-fill-intent.md` and `docs/sdk/examples/monitor-account.md` provide practical usage examples.
- Verify that `sdk/typescript/README.md` includes install, build/sign/submit, monitoring, HMAC request signing, and test instructions.

Final link and sensitivity verification:
- The public preprod explorer links in this report were checked and returned HTTP `200`.
- The GitHub source links target the public `sdk-api` branch and PR #1. The current local `sdk-api` branch includes the latest HMAC validation commits; push the branch before submitting this report so the new source links, including the bad-HMAC E2E script, are observable by auditors.
- This Proof of Achievement contains public repository links, public branch/commit identifiers, public preprod transaction hashes, public preprod account/pool identifiers, public asset ids, and test command output.
- This Proof of Achievement does not include wallet seeds, private keys, Blockfrost API keys, HMAC secrets, local user paths, or raw local state files that contain secrets.
