import { assertEquals, assertThrows } from "https://deno.land/std@0.224.0/assert/mod.ts";
import { assertWritableAgentConfig, resolveAgentConfigPath } from "../src/operator_config.ts";

Deno.test("operator funding refuses default preprod config during partial e2e", () => {
  assertThrows(
    () => assertWritableAgentConfig("../../resources/preprod.config.json", { partialE2e: true }),
    Error,
    "must not update default preprod config during partial E2E",
  );
  assertThrows(
    () => assertWritableAgentConfig("/repo/green-order-cardano-agent/resources/preprod.config.json", {
      partialE2e: true,
    }),
    Error,
    "must not update default preprod config during partial E2E",
  );
});

Deno.test("operator funding accepts explicit temp config during partial e2e", () => {
  assertEquals(resolveAgentConfigPath("/tmp/partial.json"), "/tmp/partial.json");
  assertWritableAgentConfig("/tmp/partial.json", { partialE2e: true });
});
