import { loadConfig } from "./src/config.ts";
import { koiosUtxoByRef } from "./src/koios.ts";
import { computePartialFillPlan, partialPlanToJson } from "./src/partial_preflight.ts";
import { loadState } from "./src/state.ts";

const shellEnv = Deno.args.includes("--shell-env");
const config = await loadConfig();
const state = await loadState(config.statePath);

if (!state.pool) throw new Error("Run 01-create-royalty-v1-pool.ts first");

const pool = await koiosUtxoByRef(state.pool.outputRef);
if (!pool) throw new Error("current pool output is not available on preprod");

const plan = await computePartialFillPlan({
  poolLovelaceReserve: pool.assets.lovelace ?? 0n,
  poolTokenReserve: pool.assets[state.pool.assetY] ?? 0n,
  poolAssetY: state.pool.assetY,
  leavingLovelace: config.partialLeavingLovelace,
  expectedTokenAmount: config.partialExpectedTokenAmount,
  feeLovelace: config.partialFeeLovelace,
});

if (shellEnv) {
  for (const [key, value] of Object.entries(partialPlanToJson(plan))) {
    console.log(`export PARTIAL_PREFLIGHT_${camelToEnv(key)}=${quoteShell(value)}`);
  }
} else {
  console.log(JSON.stringify(partialPlanToJson(plan), null, 2));
}

function camelToEnv(value: string): string {
  return value.replace(/[A-Z]/g, (char) => `_${char}`).toUpperCase();
}

function quoteShell(value: string): string {
  return `'${value.replaceAll("'", "'\\''")}'`;
}
