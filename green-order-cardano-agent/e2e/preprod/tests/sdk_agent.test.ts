import { assertEquals, assertThrows } from "https://deno.land/std@0.224.0/assert/mod.ts";
import {
  buildSdkClientOptions,
  parseSdkAcceptedResponse,
  parseSdkAccountFoundResponse,
  parseSdkAccountStatusResponse,
  parseSdkBoundResponse,
  parseSdkMonitoringReadinessResponse,
} from "../src/sdk_agent.ts";

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

Deno.test("SDK response parsers validate monitoring readiness and account summary shapes", () => {
  parseSdkMonitoringReadinessResponse({ status: "ok", accountIndex: "available" });
  parseSdkAccountFoundResponse({
    status: "found",
    reason: null,
    account: { accountId: "11".repeat(32) },
  }, "11".repeat(32));

  assertThrows(() => parseSdkMonitoringReadinessResponse({ status: "error" }));
  assertThrows(() =>
    parseSdkAccountFoundResponse({
      status: "found",
      reason: null,
      account: { accountId: "22".repeat(32) },
    }, "11".repeat(32))
  );
});

Deno.test("SDK response parser validates account status shapes", () => {
  parseSdkAccountStatusResponse({ status: { current: true, pending: false, predicted: false } });

  assertThrows(() => parseSdkAccountStatusResponse({ status: "observed" }));
  assertThrows(() => parseSdkAccountStatusResponse({ status: { current: true } }));
});
