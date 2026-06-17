import test from "node:test";
import assert from "node:assert/strict";

import { GreenOrderClient, GreenOrderSdkError } from "../src/index.js";

test("submits intents to the agent JSON endpoint", async () => {
  const calls: Array<{ url: string; init: RequestInit }> = [];
  const client = new GreenOrderClient({
    baseUrl: "http://127.0.0.1:9031",
    fetch: async (url, init) => {
      calls.push({ url: String(url), init: init ?? {} });
      return new Response(JSON.stringify({ status: "accepted", reason: null }), { status: 202 });
    },
  });

  const response = await client.submitIntent({ hello: "intent" });

  assert.deepEqual(response, { status: "accepted", reason: null });
  assert.equal(calls[0].url, "http://127.0.0.1:9031/intents");
  assert.equal(calls[0].init.method, "POST");
  assert.equal(calls[0].init.body, "{\"hello\":\"intent\"}");
});

test("turns rejected agent responses into typed SDK errors", async () => {
  const client = new GreenOrderClient({
    baseUrl: "http://127.0.0.1:9031",
    fetch: async () =>
      new Response(JSON.stringify({ status: "rejected", reason: "missingAccount" }), { status: 409 }),
  });

  await assert.rejects(
    () => client.submitIntent({}),
    (err) => err instanceof GreenOrderSdkError && err.status === 409 && err.reason === "missingAccount",
  );
});

test("queries account status using the existing agent route", async () => {
  const client = new GreenOrderClient({
    baseUrl: "http://127.0.0.1:9031/",
    fetch: async (url) => {
      assert.equal(
        String(url),
        "http://127.0.0.1:9031/accounts/status?accountId=aa&txHash=bb&outputIndex=0",
      );
      return new Response(JSON.stringify({ status: "observed" }), { status: 200 });
    },
  });

  assert.deepEqual(await client.getAccountStatus({ accountId: "aa", txHash: "bb", outputIndex: 0 }), {
    status: "observed",
  });
});

test("binds accounts through the existing agent route", async () => {
  const client = new GreenOrderClient({
    baseUrl: "http://127.0.0.1:9031",
    fetch: async (url, init) => {
      assert.equal(String(url), "http://127.0.0.1:9031/accounts/bind");
      assert.equal(init?.method, "POST");
      assert.equal(init?.body, "{\"accountId\":\"aa\",\"txHash\":\"bb\",\"outputIndex\":0}");
      return new Response(JSON.stringify({ status: "bound", reason: null }), { status: 200 });
    },
  });

  assert.deepEqual(await client.bindAccount({ accountId: "aa", txHash: "bb", outputIndex: 0 }), {
    status: "bound",
    reason: null,
  });
});

test("queries agent readiness through the health endpoint", async () => {
  const client = new GreenOrderClient({
    baseUrl: "http://127.0.0.1:9024",
    fetch: async (url, init) => {
      assert.equal(String(url), "http://127.0.0.1:9024/health");
      assert.equal(init?.method, "GET");
      return new Response(JSON.stringify({ status: "ok" }), { status: 200 });
    },
  });

  assert.deepEqual(await client.getReadiness(), { status: "ok" });
});

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

  assert.throws(() => client.getAccount("aa"), /accountId must be 64 hex chars/);
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

test("queries green-order monitoring readiness", async () => {
  const client = new GreenOrderClient({
    baseUrl: "http://127.0.0.1:9031",
    fetch: async (url) => {
      assert.equal(String(url), "http://127.0.0.1:9031/monitoring/readiness");
      return new Response(JSON.stringify({ status: "ok", service: "green-order-agent" }), { status: 200 });
    },
  });

  assert.deepEqual(await client.getMonitoringReadiness(), {
    status: "ok",
    service: "green-order-agent",
  });
});

test("adds hmac headers to every sdk request when configured", async () => {
  const seen: Array<{ url: string; init: RequestInit }> = [];
  const client = new GreenOrderClient({
    baseUrl: "http://127.0.0.1:9031",
    hmac: { secret: "test-secret", keyId: "preprod" },
    fetch: async (url, init) => {
      seen.push({ url: String(url), init: init ?? {} });
      return new Response(JSON.stringify({ status: "ok" }), { status: 200 });
    },
  });

  await client.submitIntent({});
  await client.bindAccount({ accountId: "aa", txHash: "bb", outputIndex: 0 });
  await client.getAccountStatus({ accountId: "aa", txHash: "bb", outputIndex: 0 });
  await client.getAccount("aa".repeat(32));
  await client.getReadiness();
  await client.getMonitoringSummary();
  await client.getMonitoringReadiness();

  assert.equal(seen.length, 7);
  for (const call of seen) {
    const headers = call.init.headers as Record<string, string>;
    assert.equal(headers["x-go-key-id"], "preprod", call.url);
    assert.match(headers["x-go-timestamp"], /^[0-9]+$/, call.url);
    assert.match(headers["x-go-nonce"], /.+/, call.url);
    assert.match(headers["x-go-body-sha256"], /^[0-9a-f]{64}$/, call.url);
    assert.match(headers["x-go-signature"], /^hmac-sha256=[0-9a-f]{64}$/, call.url);
  }
});
