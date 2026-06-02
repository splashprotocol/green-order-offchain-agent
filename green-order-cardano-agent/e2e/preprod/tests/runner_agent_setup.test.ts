import { assert, assertStringIncludes } from "https://deno.land/std@0.224.0/assert/mod.ts";

const runner = await Deno.readTextFile("run-preprod-e2e.sh");

Deno.test("run-preprod-e2e can launch and stop a local green-order agent", () => {
  assertStringIncludes(runner, "START_GREEN_ORDER_AGENT");
  assertStringIncludes(runner, "start_green_order_agent");
  assertStringIncludes(runner, "stop_green_order_agent");
  assertStringIncludes(runner, "trap stop_green_order_agent EXIT");
  assertStringIncludes(runner, "./target/debug/green-order-cardano-agent");
});

Deno.test("run-preprod-e2e builds SDK before account binding can load it", () => {
  const buildIndex = runner.indexOf("npm --prefix ../../../sdk/typescript run build");
  const bindIndex = runner.indexOf("03-create-aleph-account-and-bind.ts");

  assert(buildIndex >= 0, "runner should build the SDK");
  assert(bindIndex >= 0, "runner should run account binding");
  assert(buildIndex < bindIndex, "SDK build must happen before account binding");
});

Deno.test("run-preprod-e2e keeps agent cleanup trap active in force mode", () => {
  assert(!runner.includes("trap - EXIT"), "runner must not clear the agent cleanup trap");
});

Deno.test("run-preprod-e2e passes an absolute generated config path to the agent", () => {
  assertStringIncludes(runner, "absolute_e2e_path");
  assertStringIncludes(runner, 'AGENT_RUN_CONFIG_PATH="$(absolute_e2e_path');
  assertStringIncludes(runner, '--config-path "$AGENT_RUN_CONFIG_PATH"');
});

Deno.test("run-preprod-e2e queries SDK monitoring after smoke execution", () => {
  const smokeIndex = runner.indexOf('"${smoke_script}"');
  const queryIndex = runner.indexOf("11-query-agent-via-sdk.ts");

  assert(smokeIndex >= 0, "runner should execute the smoke script");
  assert(queryIndex >= 0, "runner should query SDK monitoring");
  assert(smokeIndex < queryIndex, "SDK monitoring query must run after smoke execution");
});
