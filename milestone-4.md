New Proof of Achievement for Milestone 4

A. Output: TypeScript SDK codebase for Green Order integration

Acceptance criteria: The SDK code is written and available in the open GitHub repository. It provides developer-facing functionality to build Green Order intent payloads, compute Aleph-compatible intention digests, sign intents through an injected wallet/signer, submit intents to a Green Order agent, bind Aleph accounts, query account status, and monitor the agent.

Evidence:
- Public milestone branch: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api
- TypeScript SDK package: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/sdk/typescript
- SDK package manifest and scripts: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/package.json
- SDK public exports: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/index.ts
- Agent HTTP client with submit, bind, status, account, readiness, and monitoring methods: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/client.ts
- Green Order intent building, Aleph intention CBOR/digest, and signing helpers: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/intent.ts
- ADA/native asset helpers for the wire format: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/assets.ts
- HMAC request signing helpers for operator HTTP API protection: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/hmac.ts
- Account binding and account-status payload helpers: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/src/account.ts

B. Output: Green Order agent API routes required by the SDK

Acceptance criteria: The agent exposes SDK-friendly HTTP routes for submit, bind, and monitor operations, including account lookup and monitoring endpoints. These routes are available in the open repository and are exercised by SDK unit tests and the preprod E2E harness.

Evidence:
- Agent HTTP routes for `POST /intents`, `POST /accounts/bind`, `GET /accounts/status`, `GET /accounts/:account_id`, `GET /monitoring/summary`, and `GET /monitoring/readiness`: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/http_intent_source.rs
- Agent configuration support used by the SDK/E2E flow: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/src/config.rs
- SDK client route tests for submit, bind, status, account, readiness, and monitoring calls: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/test/client.test.ts
- Preprod SDK adapter and response validators used by E2E scripts: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/src/sdk_agent.ts

C. Output: Unit test coverage for the SDK

Acceptance criteria: The SDK is covered by unit tests in the open GitHub repository. The tests cover account route payloads, agent client behavior, rejected responses, HMAC canonicalization/signature headers, asset encoding, Aleph-compatible intention digest generation, wire payload generation, and signer injection.

Evidence:
- SDK unit test directory: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api/sdk/typescript/test
- Account helper tests: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/test/account.test.ts
- Agent client tests: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/test/client.test.ts
- HMAC request signing tests: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/test/hmac.test.ts
- Intent encoding, digest, payload, and signer tests: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/sdk/typescript/test/intent.test.ts

Verification command executed locally against branch `sdk-api` at commit `3aaac4ce735355f7ae678563df10f71c08d78b60`:

```sh
npm --prefix sdk/typescript test
```

Result:
- `17` SDK tests passed.
- `0` SDK tests failed.

D. Output: Markdown SDK documentation

Acceptance criteria: The SDK codebase is documented in Markdown through files available in the open GitHub repository. The documentation explains installation, build/sign/submit usage, account monitoring, HMAC request signing, and test commands.

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

E. Output: SDK-based preprod integration and monitoring evidence

Acceptance criteria: The SDK can be used by integration scripts to bind accounts, submit Green Order intents, and query monitoring/account endpoints. The preprod E2E harness builds the SDK, starts the Green Order agent, prepares a fresh forced state, runs SDK-based account binding and intent submission, and queries monitoring endpoints after smoke execution.

Evidence:
- SDK-based preprod E2E runner: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/run-preprod-e2e.sh
- SDK-based account binding script: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/03-create-aleph-account-and-bind.ts
- SDK-based full-fill submit script: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/04-submit-green-order-smoke.ts
- SDK monitoring query script that calls `getMonitoringReadiness`, `getMonitoringSummary`, `getAccount`, and `getAccountStatus`: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/11-query-agent-via-sdk.ts
- E2E tests proving the runner builds the SDK before SDK use, starts/stops the agent, queries SDK monitoring after smoke execution, prompts for Blockfrost without hardcoding it, and isolates forced fresh wallet state: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/tests/runner_agent_setup.test.ts
- SDK response parser tests for monitoring and account status shapes: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/green-order-cardano-agent/e2e/preprod/tests/sdk_agent.test.ts

Verification commands executed locally against branch `sdk-api` at commit `3aaac4ce735355f7ae678563df10f71c08d78b60`:

```sh
deno task check
deno test --no-lock --allow-run --allow-read --allow-write --allow-env tests
```

Result:
- Preprod TypeScript/Deno check passed.
- Preprod E2E support tests passed: `47` passed, `0` failed.

Auditor-style command for a fresh SDK E2E run:

```sh
cd green-order-cardano-agent/e2e/preprod
CARDANO_NODE_SOCKET_PATH=/absolute/path/to/preprod/node.socket FORCE_E2E_STATE=1 bash ./run-preprod-e2e.sh
```

The runner asks for a preprod Blockfrost project id when one is not already configured. Pressing Enter uses Koios. Secrets are read from the environment or generated ignored local files; the scripts do not hardcode API keys, wallet seeds, or private keys.

F. Pull request evidence

Acceptance criteria: Code is written and a pull request is submitted to the open GitHub repository.

Evidence:
- Public branch prepared for PR: https://github.com/splashprotocol/green-order-offchain-agent/tree/sdk-api
- GitHub PR creation URL for this branch: https://github.com/splashprotocol/green-order-offchain-agent/pull/new/sdk-api
- Current branch head used for this report: `3aaac4ce735355f7ae678563df10f71c08d78b60`

Note: The branch is pushed and publicly observable. Creating the PR through `gh pr create` was attempted, but the local GitHub CLI token returned `HTTP 401: Bad credentials`. Replace the PR creation URL above with the final PR URL after authenticating and opening the PR in GitHub.

Preprod SDK execution evidence from the current local SDK E2E run:
- Network: Cardano preprod
- Branch/commit: `sdk-api` at `3aaac4ce735355f7ae678563df10f71c08d78b60`
- Account id: `c1310acb16606e95128872420461c212087e6c268aa8228017073ee68c25903e`
- Initial Aleph account output observed by the agent: `b07aba444720dcfb9ce6e7ba0359607e34faba64e9579b97c12676009a9cf303#0`
- Operator funding transaction: `daabeb39eb621ca9b93ac1794bd48d15f2aece4da5ee989ca3b84ade44548426`
- Full-fill execution transaction built by the agent: `f42c3c3c3421662eab6157ad43df8c5d7aa0a7f02e1babd8270eb8e5a6e5235f`
- Successor Aleph account output after execution: `f42c3c3c3421662eab6157ad43df8c5d7aa0a7f02e1babd8270eb8e5a6e5235f#0`
- Royalty V1 pool output after execution: `f42c3c3c3421662eab6157ad43df8c5d7aa0a7f02e1babd8270eb8e5a6e5235f#2`
- Pool asset X: `00`
- Pool asset Y: `017e404c3e81f68a1074e7f05a20da6700a6b85da1ea1e1d6e348376677265656ecf17061bb6820227`
- Pool NFT: `017e404c3e81f68a1074e7f05a20da6700a6b85da1ea1e1d6e3483766e6674cf17061bb6820227`
- Pool LQ asset: `017e404c3e81f68a1074e7f05a20da6700a6b85da1ea1e1d6e3483766c71cf17061bb6820227`
- Full-fill submitted intent digest: `da5f8568be3d34c7ab782486fae335c8609dc3cd2333e4be33766cdd24907e00`
- Agent log evidence: the local SDK E2E log records `Built execution tx f42c3c3c3421662eab6157ad43df8c5d7aa0a7f02e1babd8270eb8e5a6e5235f` with account, operator-funding, and pool inputs, and later records the successor Aleph account output at `f42c3c3c3421662eab6157ad43df8c5d7aa0a7f02e1babd8270eb8e5a6e5235f#0`.

Public explorer links:
- https://preprod.cexplorer.io/tx/b07aba444720dcfb9ce6e7ba0359607e34faba64e9579b97c12676009a9cf303
- https://preprod.cexplorer.io/tx/daabeb39eb621ca9b93ac1794bd48d15f2aece4da5ee989ca3b84ade44548426
- https://preprod.cexplorer.io/tx/f42c3c3c3421662eab6157ad43df8c5d7aa0a7f02e1babd8270eb8e5a6e5235f

Sensitive-information confirmation: this Proof of Achievement contains public repository links, public branch/commit identifiers, public preprod transaction hashes, public preprod account/pool identifiers, public asset ids, and test command output. It does not include wallet seeds, private keys, Blockfrost API keys, HMAC secrets, local node socket paths, or raw local state files that contain secrets.
