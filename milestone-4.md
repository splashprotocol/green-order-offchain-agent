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
- Public PR creation URL for this branch: https://github.com/splashprotocol/green-order-offchain-agent/pull/new/sdk-api

How the Acceptance Criteria can be verified:
- Acceptance criterion 1 says the code must be written and submitted to an open GitHub repository. Verify that the public branch opens at https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api and that the SDK source files above are visible in `sdk/typescript/src`.
- Verify the SDK API surface by inspecting `sdk/typescript/src/index.ts`, `client.ts`, `intent.ts`, `account.ts`, and `hmac.ts`.
- Verify the agent routes consumed by the SDK by inspecting `green-order-cardano-agent/src/http_intent_source.rs`.
- Verify the branch head used for this report: `2b57e051f0f44e1380203d829aa7b1624b6f4cef`.
- Note on PR verification: the branch is pushed and publicly observable. Creating the PR through `gh pr create` was attempted locally, but the GitHub CLI token returned `HTTP 401: Bad credentials`. The PR creation URL above returns HTTP 200 and can be used to open the final public PR; after PR creation, replace it with the final PR URL.

SDK-based preprod integration evidence:
- SDK-based preprod E2E runner: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/run-preprod-e2e.sh
- SDK-based account binding script: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/03-create-aleph-account-and-bind.ts
- SDK-based full-fill submit script: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/04-submit-green-order-smoke.ts
- SDK monitoring query script that calls `getMonitoringReadiness`, `getMonitoringSummary`, `getAccount`, and `getAccountStatus`: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/11-query-agent-via-sdk.ts

Preprod SDK execution evidence from the current local SDK E2E run:
- Network: Cardano preprod
- Account id: `c1310acb16606e95128872420461c212087e6c268aa8228017073ee68c25903e`
- Initial Aleph account output observed by the agent: `b07aba444720dcfb9ce6e7ba0359607e34faba64e9579b97c12676009a9cf303#0`
- Operator funding transaction: `daabeb39eb621ca9b93ac1794bd48d15f2aece4da5ee989ca3b84ade44548426`
- Full-fill execution transaction built by the agent: `f42c3c3c3421662eab6157ad43df8c5d7aa0a7f02e1babd8270eb8e5a6e5235f`
- Successor Aleph account output after execution: `f42c3c3c3421662eab6157ad43df8c5d7aa0a7f02e1babd8270eb8e5a6e5235f#0`
- Full-fill submitted intent digest: `da5f8568be3d34c7ab782486fae335c8609dc3cd2333e4be33766cdd24907e00`
- Public explorer links:
  - https://preprod.cexplorer.io/tx/b07aba444720dcfb9ce6e7ba0359607e34faba64e9579b97c12676009a9cf303
  - https://preprod.cexplorer.io/tx/daabeb39eb621ca9b93ac1794bd48d15f2aece4da5ee989ca3b84ade44548426
  - https://preprod.cexplorer.io/tx/f42c3c3c3421662eab6157ad43df8c5d7aa0a7f02e1babd8270eb8e5a6e5235f

Auditor-style command to verify the SDK-based preprod flow:

```sh
cd green-order-cardano-agent/e2e/preprod
CARDANO_NODE_SOCKET_PATH=/absolute/path/to/preprod/node.socket FORCE_E2E_STATE=1 bash ./run-preprod-e2e.sh
```

The runner asks for a preprod Blockfrost project id when one is not already configured. Pressing Enter uses Koios. Secrets are read from the environment or generated ignored local files; the scripts do not hardcode API keys, wallet seeds, or private keys.

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

Milestone Output 3 — SDK documentation published on GitHub and GitBook

Description: The SDK is documented in Markdown files available in the open GitHub repository. The documentation explains the TypeScript SDK package, installation and test commands, build/sign/submit usage, account monitoring, HMAC request signing, the SDK HTTP API contract, test coverage, and concrete examples. The `docs/sdk` directory is the source material intended for GitBook publication.

Evidence:
- SDK README: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/README.md
- SDK documentation directory intended as the GitBook source: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/docs/sdk
- TypeScript SDK API documentation: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/sdk/typescript.md
- Agent API contract used by the SDK: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/sdk/api-contract.md
- SDK test coverage documentation: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/sdk/test-coverage.md
- Full-fill intent example: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/sdk/examples/full-fill-intent.md
- Account monitoring example: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/sdk/examples/monitor-account.md
- Preprod SDK E2E README with auditor run instructions, Blockfrost/Koios provider behavior, forced fresh wallet state, HMAC env behavior, and SDK monitoring query instructions: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/README.md
- Repository README entry point: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/README.md

How the Acceptance Criteria can be verified:
- Acceptance criterion 3 says the codebase must be documented and accessible as Markdown files through the open GitHub repository. Verify each documentation link above opens from the public `sdk-api` branch.
- Verify that `docs/sdk/typescript.md` documents the SDK APIs and signer model.
- Verify that `docs/sdk/api-contract.md` documents the agent HTTP routes wrapped by the SDK.
- Verify that `docs/sdk/examples/full-fill-intent.md` and `docs/sdk/examples/monitor-account.md` provide practical usage examples.
- Verify that `sdk/typescript/README.md` includes install, build/sign/submit, monitoring, HMAC request signing, and test instructions.

Final link and sensitivity verification:
- All GitHub and public preprod explorer links in this report were checked and returned HTTP `200`.
- This Proof of Achievement contains public repository links, public branch/commit identifiers, public preprod transaction hashes, public preprod account/pool identifiers, public asset ids, and test command output.
- This Proof of Achievement does not include wallet seeds, private keys, Blockfrost API keys, HMAC secrets, local user paths, or raw local state files that contain secrets.
