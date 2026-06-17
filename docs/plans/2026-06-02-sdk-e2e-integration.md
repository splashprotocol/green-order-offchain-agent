# SDK E2E Integration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Prove the TypeScript SDK integrates with the Green Order agent by using it from the existing preprod E2E scripts for agent HTTP calls and endpoint monitoring.

**Architecture:** Keep the existing Deno preprod E2E harness for wallet, Koios, Lucid, and chain-state validation. Build the local Node/TypeScript SDK before smoke execution, then load the compiled SDK from Deno through a small adapter that wraps `GreenOrderClient`, normalizes the existing response checks, and optionally sends HMAC headers when configured. Add a read-only endpoint demo script that queries every non-mutating SDK endpoint, while existing account bind and smoke scripts exercise the mutating SDK calls.

**Tech Stack:** Deno preprod E2E scripts, local `sdk/typescript` package, Node-built SDK `dist`, Green Order Rust HTTP agent, existing Koios/Lucid validation.

---

### Task 1: Add SDK E2E Adapter Tests

**Files:**
- Create: `green-order-cardano-agent/e2e/preprod/tests/sdk_agent.test.ts`
- Modify later: `green-order-cardano-agent/e2e/preprod/src/sdk_agent.ts`

**Step 1: Write failing tests**

Create tests for adapter behavior without loading the real SDK:

```ts
import {
  buildSdkClientOptions,
  parseSdkAcceptedResponse,
  parseSdkBoundResponse,
  parseSdkMonitoringReadinessResponse,
  parseSdkAccountFoundResponse,
  parseSdkAccountStatusResponse,
} from "../src/sdk_agent.ts";
import { assertEquals, assertRejects, assertThrows } from "jsr:@std/assert@1";

Deno.test("buildSdkClientOptions includes optional HMAC settings", () => {
  const options = buildSdkClientOptions("http://127.0.0.1:9031/", {
    secret: "test-secret",
    keyId: "preprod",
  });

  assertEquals(options.baseUrl, "http://127.0.0.1:9031/");
  assertEquals(options.hmac, { secret: "test-secret", keyId: "preprod" });
});

Deno.test("buildSdkClientOptions omits HMAC when secret is absent", () => {
  const options = buildSdkClientOptions("http://127.0.0.1:9031", {});

  assertEquals(options.baseUrl, "http://127.0.0.1:9031");
  assertEquals("hmac" in options, false);
});

Deno.test("SDK response parsers keep existing accepted and bound checks", () => {
  parseSdkAcceptedResponse({ status: "accepted", reason: null });
  parseSdkBoundResponse({ status: "bound", reason: null });

  assertThrows(() => parseSdkAcceptedResponse({ status: "rejected", reason: "x" }));
  assertThrows(() => parseSdkBoundResponse({ status: "rejected", reason: "x" }));
});

Deno.test("SDK response parsers validate readiness and account summary shapes", () => {
  parseSdkMonitoringReadinessResponse({ status: "ok", accountIndex: "available" });
  parseSdkAccountFoundResponse({
    status: "found",
    reason: null,
    account: { accountId: "11".repeat(32) },
  }, "11".repeat(32));

  assertThrows(() => parseSdkMonitoringReadinessResponse({ status: "error" }));
  assertThrows(() =>
    parseSdkAccountFoundResponse({ status: "found", reason: null, account: { accountId: "22".repeat(32) } }, "11".repeat(32))
  );
});

Deno.test("SDK response parser validates account status shapes", () => {
  parseSdkAccountStatusResponse({ status: { current: true, pending: false, predicted: false } });

  assertThrows(() => parseSdkAccountStatusResponse({ status: "observed" }));
  assertThrows(() => parseSdkAccountStatusResponse({ status: { current: true } }));
});
```

**Step 2: Run test to verify it fails**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno test --no-lock --allow-read --allow-env tests/sdk_agent.test.ts
```

Expected: FAIL because `src/sdk_agent.ts` does not exist.

---

### Task 2: Implement the SDK Adapter

**Files:**
- Create: `green-order-cardano-agent/e2e/preprod/src/sdk_agent.ts`

**Step 1: Add adapter**

Implement:

```ts
import { OutputRef } from "./state.ts";

export type AgentHmacConfig = { secret?: string; keyId?: string };
export type SdkClientOptions = { baseUrl: string; hmac?: { secret: string; keyId?: string } };
export type GreenOrderSdkClient = {
  bindAccount(payload: { accountId: string; txHash: string; outputIndex: number }): Promise<unknown>;
  submitIntent(payload: unknown): Promise<unknown>;
  getReadiness(): Promise<unknown>;
  getMonitoringReadiness(): Promise<unknown>;
  getMonitoringSummary(): Promise<unknown>;
  getAccount(accountId: string): Promise<unknown>;
  getAccountStatus(input: { accountId: string; txHash: string; outputIndex: number }): Promise<unknown>;
};

export function buildSdkClientOptions(agentUrl: string, hmac: AgentHmacConfig): SdkClientOptions {
  return hmac.secret ? { baseUrl: agentUrl, hmac: { secret: hmac.secret, keyId: hmac.keyId } } : { baseUrl: agentUrl };
}

export async function loadGreenOrderSdkClient(agentUrl: string, hmac: AgentHmacConfig): Promise<GreenOrderSdkClient> {
  const sdkPath = new URL("../../../../sdk/typescript/dist/src/index.js", import.meta.url);
  try {
    await Deno.stat(sdkPath);
  } catch (error) {
    if (error instanceof Deno.errors.NotFound) {
      throw new Error("Build the TypeScript SDK first: npm --prefix ../../../sdk/typescript run build; expected sdk/typescript/dist/src/index.js");
    }
    throw error;
  }
  const sdk = await import(sdkPath.href);
  return new sdk.GreenOrderClient(buildSdkClientOptions(agentUrl, hmac));
}

export async function bindAccountViaSdk(client: GreenOrderSdkClient, accountId: string, outputRef: OutputRef): Promise<unknown> {
  return client.bindAccount({ accountId, txHash: outputRef.txHash, outputIndex: outputRef.outputIndex });
}

export async function submitIntentViaSdk(client: GreenOrderSdkClient, payload: unknown): Promise<unknown> {
  return client.submitIntent(payload);
}

export function parseSdkBoundResponse(body: unknown): void {
  assertStatus(body, "bound");
}

export function parseSdkAcceptedResponse(body: unknown): void {
  assertStatus(body, "accepted");
}
```

Also add parser helpers for monitoring readiness, monitoring summary, account summary, and account status. Keep them intentionally small and shape-based. The E2E agent API client should not call SDK `getReadiness()` because `/health` lives on `agentHealthUrl`, not `agentUrl`.

**Step 2: Run adapter tests**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno test --no-lock --allow-read --allow-env tests/sdk_agent.test.ts
```

Expected: PASS.

---

### Task 3: Wire Existing E2E Scripts Through the SDK

**Files:**
- Modify: `green-order-cardano-agent/e2e/preprod/src/config.ts`
- Modify: `green-order-cardano-agent/e2e/preprod/03-create-aleph-account-and-bind.ts`
- Modify: `green-order-cardano-agent/e2e/preprod/04-submit-green-order-smoke.ts`
- Modify: `green-order-cardano-agent/e2e/preprod/10-submit-green-order-partial-smoke.ts`
- Modify: `green-order-cardano-agent/e2e/preprod/run-preprod-e2e.sh`

**Step 1: Add optional HMAC config**

In `PreprodE2eConfig`, add:

```ts
agentHmacSecret?: string;
agentHmacKeyId?: string;
```

In `loadConfig()`, read:

```ts
agentHmacSecret: optionalEnv("AGENT_HMAC_SECRET"),
agentHmacKeyId: optionalEnv("AGENT_HMAC_KEY_ID"),
```

Do not require these values. Do not hardcode secrets.

**Step 2: Build SDK before scripts that load it**

In `run-preprod-e2e.sh`, immediately after `cd "$(dirname "$0")"`, add:

```bash
npm --prefix ../../../sdk/typescript run build
```

This ensures Deno can dynamically import `sdk/typescript/dist/src/index.js` before `03-create-aleph-account-and-bind.ts` loads the SDK.

**Step 3: Replace direct agent helpers with SDK adapter**

In `03-create-aleph-account-and-bind.ts`, replace `bindAccount` import and call with:

```ts
import { bindAccountViaSdk, loadGreenOrderSdkClient } from "./src/sdk_agent.ts";

const sdkClient = await loadGreenOrderSdkClient(config.agentUrl, {
  secret: config.agentHmacSecret,
  keyId: config.agentHmacKeyId,
});

const body = await bindAccountViaSdk(sdkClient, pending.accountId, pending.outputRef as OutputRef)
```

In `04-submit-green-order-smoke.ts` and `10-submit-green-order-partial-smoke.ts`, replace `submitIntent` and `parseAgentAcceptedResponse` with:

```ts
import { loadGreenOrderSdkClient, parseSdkAcceptedResponse, submitIntentViaSdk } from "./src/sdk_agent.ts";

const sdkClient = await loadGreenOrderSdkClient(config.agentUrl, {
  secret: config.agentHmacSecret,
  keyId: config.agentHmacKeyId,
});
const response = await submitIntentViaSdk(sdkClient, payload);
parseSdkAcceptedResponse(response);
```

**Step 4: Run Deno check**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
npm --prefix ../../../sdk/typescript run build
deno task check
```

Expected: PASS.

---

### Task 4: Add SDK Endpoint Demo Script

**Files:**
- Create: `green-order-cardano-agent/e2e/preprod/11-query-agent-via-sdk.ts`
- Modify: `green-order-cardano-agent/e2e/preprod/deno.json`
- Modify: `green-order-cardano-agent/e2e/preprod/README.md`

**Step 1: Write script**

Create a read-only script that loads config/state, creates the SDK client, and prints JSON results for:

- `client.getMonitoringReadiness()`
- `client.getMonitoringSummary()`
- `client.getAccount(state.account.accountId)`
- `client.getAccountStatus({ accountId, txHash, outputIndex })`

It should throw if `state.account` does not exist, with the existing message style:

```ts
if (!state.account) throw new Error("Run 03-create-aleph-account-and-bind.ts first");
```

Use parser helpers from `src/sdk_agent.ts` before printing results.

Do not call `client.getReadiness()` in this script. The SDK method is still documented for clients whose health endpoint shares the same base URL, but this preprod harness has a separate `agentHealthUrl` and the Green Order API port does not expose `/health`.

**Step 2: Include in Deno check task**

Add `11-query-agent-via-sdk.ts` to the `deno.json` `check` task.

**Step 3: Update README**

Document:

- The E2E now uses the local TypeScript SDK for agent HTTP calls.
- `run-preprod-e2e.sh` builds the SDK before smoke execution.
- Optional `AGENT_HMAC_SECRET` and `AGENT_HMAC_KEY_ID` are read from env and sent by the SDK if present.
- `11-query-agent-via-sdk.ts` demonstrates all read-only SDK endpoints available on `AGENT_URL`: account summary, output-ref account status, monitoring summary, and monitoring readiness.
- The legacy SDK `getReadiness()` method targets `/health` on the client base URL; the preprod harness keeps health checks on `AGENT_HEALTH_URL`.
- Mutating SDK endpoint coverage is in `03-create-aleph-account-and-bind.ts` (`bindAccount`) and `04`/`10` smoke scripts (`submitIntent`).

**Step 4: Run script in dry-compatible mode**

Because the query script needs a running agent and state, do not require a live run in unit verification. Use `deno check` for static coverage and adapter tests for parser behavior.

---

### Task 5: Full Verification and Commit

**Files:**
- All files above.

**Step 1: Run SDK tests**

Run:

```bash
cd sdk/typescript
npm run test:coverage
```

Expected: 17 SDK tests pass.

**Step 2: Run E2E Deno tests**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
deno test --no-lock --allow-run --allow-read --allow-write --allow-env tests
```

Expected: all Deno E2E unit tests pass.

**Step 3: Run Deno check**

Run:

```bash
cd green-order-cardano-agent/e2e/preprod
npm --prefix ../../../sdk/typescript run build
deno task check
```

Expected: PASS.

**Step 4: Run reviewer loop**

Ask a subreviewer to inspect:

- `docs/plans/2026-06-02-sdk-e2e-integration.md`
- `green-order-cardano-agent/e2e/preprod/src/sdk_agent.ts`
- updated `03`, `04`, `10`, `11` E2E scripts
- `src/config.ts`
- `run-preprod-e2e.sh`
- `run-preprod-partial-e2e.sh`; partial-fill coverage is inherited because it sets `E2E_SMOKE_SCRIPT=10-submit-green-order-partial-smoke.ts` and then `exec bash run-preprod-e2e.sh`
- `README.md`

Reviewer must return `OK` or required fixes. Apply fixes and repeat until `OK`.

**Step 5: Commit**

Run:

```bash
git add docs/plans/2026-06-02-sdk-e2e-integration.md green-order-cardano-agent/e2e/preprod
git commit -m "test: exercise green order sdk in preprod e2e"
```

Do not add `.state/`, `demo/.state/`, `milestone-3.md`, or generated SDK `dist/`.
