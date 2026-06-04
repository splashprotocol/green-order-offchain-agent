import { assert, assertStringIncludes } from "https://deno.land/std@0.224.0/assert/mod.ts";

const runner = await Deno.readTextFile("run-preprod-e2e.sh");
const bindAccountScript = await Deno.readTextFile("03-create-aleph-account-and-bind.ts");
const walletScript = await Deno.readTextFile("00-prepare-preprod-wallets.ts");
const operatorFundingScript = await Deno.readTextFile("07-prepare-operator-funding.ts");

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

Deno.test("run-preprod-e2e prepares wallet env before operator funding", () => {
  const walletIndex = runner.indexOf("prepare_preprod_wallet_env");
  const fundingIndex = runner.indexOf("07-prepare-operator-funding.ts");

  assertStringIncludes(runner, "00-prepare-preprod-wallets.ts");
  assertStringIncludes(runner, "00-wait-for-preprod-wallet-funding.ts");
  assertStringIncludes(runner, "FUNDED_WALLET_SEED");
  assertStringIncludes(runner, "BATCHER_ADDRESS");
  assertStringIncludes(runner, "Send at least ${E2E_BATCHER_REQUESTED_ADA:-400} tADA to:");
  assert(!runner.includes("then rerun"), "fresh wallet path should wait instead of asking for a rerun");
  assert(!runner.includes("exit 2"), "fresh wallet path should continue after funding is observed");
  assert(walletIndex >= 0, "runner should prepare wallet env");
  assert(fundingIndex >= 0, "runner should prepare operator funding");
  assert(walletIndex < fundingIndex, "wallet env must be available before operator funding");
});

Deno.test("run-preprod-e2e asks for Blockfrost before falling back to Koios", () => {
  const providerIndex = runner.indexOf("prepare_preprod_provider_env");
  const walletIndex = runner.indexOf("prepare_preprod_wallet_env");

  assertStringIncludes(runner, "has_blockfrost_project_id");
  assertStringIncludes(runner, "BLOCKFROST_PROJECT_ID");
  assertStringIncludes(runner, "Enter Blockfrost preprod project id");
  assertStringIncludes(runner, "using Koios provider");
  assert(providerIndex >= 0, "runner should prepare provider env");
  assert(walletIndex >= 0, "runner should prepare wallet env");
  assert(providerIndex < walletIndex, "provider choice should be available before wallet funding checks");
});

Deno.test("wallet preparation requests enough batcher tADA for auditor runs", () => {
  assertStringIncludes(walletScript, 'E2E_BATCHER_REQUESTED_ADA")?.trim() || "400"');
});

Deno.test("operator funding script retries provider UTxO lookups", () => {
  assertStringIncludes(operatorFundingScript, 'import { withRetries } from "./src/retry.ts";');
  assertStringIncludes(operatorFundingScript, "getUtxosWithRetries");
  assertStringIncludes(operatorFundingScript, "OPERATOR_FUNDING_PROVIDER_RETRY_ATTEMPTS");
  assertStringIncludes(operatorFundingScript, "operator funding observation failed");
});

Deno.test("run-preprod-e2e keeps agent cleanup trap active in force mode", () => {
  assert(!runner.includes("trap - EXIT"), "runner must not clear the agent cleanup trap");
});

Deno.test("run-preprod-e2e passes an absolute generated config path to the agent", () => {
  assertStringIncludes(runner, "absolute_e2e_path");
  assertStringIncludes(runner, 'AGENT_RUN_CONFIG_PATH="$(absolute_e2e_path');
  assertStringIncludes(runner, '--config-path "$AGENT_RUN_CONFIG_PATH"');
});

Deno.test("run-preprod-e2e cleans forced agent chain-sync state before launch", () => {
  const configIndex = runner.indexOf("prepare_green_order_agent_config");
  const cleanIndex = runner.indexOf("clean_forced_green_order_agent_state");
  const buildIndex = runner.indexOf("agent: building binary");

  assertStringIncludes(runner, "FORCE_E2E_STATE");
  assertStringIncludes(runner, 'rm -rf "$db_path" "$db_path.green-account-stores.json"');
  assertStringIncludes(runner, "ALLOW_FORCE_CLEAN_EXTERNAL_AGENT_DB");
  assert(configIndex >= 0, "runner should prepare generated agent config");
  assert(cleanIndex >= 0, "runner should clean generated agent state");
  assert(buildIndex >= 0, "runner should build the agent");
  assert(configIndex < cleanIndex, "cleanup should read the generated config path");
  assert(cleanIndex < buildIndex, "cleanup should happen before the agent starts");
});

Deno.test("run-preprod-e2e queries SDK monitoring after smoke execution", () => {
  const smokeIndex = runner.indexOf('"${smoke_script}"');
  const queryIndex = runner.indexOf("11-query-agent-via-sdk.ts");

  assert(smokeIndex >= 0, "runner should execute the smoke script");
  assert(queryIndex >= 0, "runner should query SDK monitoring");
  assert(smokeIndex < queryIndex, "SDK monitoring query must run after smoke execution");
});

Deno.test("account binding script logs agent observation progress", () => {
  assertStringIncludes(bindAccountScript, "waitingForAgentObservation");
  assertStringIncludes(bindAccountScript, "ACCOUNT_BIND_PROGRESS_EVERY");
  assertStringIncludes(bindAccountScript, "getMonitoringSummary");
});
