import { assertEquals, assertRejects } from "https://deno.land/std@0.224.0/assert/mod.ts";
import { loadConfig } from "../src/config.ts";

const ENV_KEYS = [
  "AGENT_CONFIG_PATH",
  "FUNDED_WALLET_SEED",
  "WALLET_ENV_PATH",
  "PARTIAL_LEAVING_LOVELACE",
  "PARTIAL_EXPECTED_TOKEN_AMOUNT",
  "PARTIAL_FEE_LOVELACE",
  "PARTIAL_EXECUTION_TIMEOUT_MS",
] as const;

const TEST_OPERATOR_KEY =
  "xprv1zzfs2eqknq4z730pg84782l2mja8vskq5zph99r37vgplpa7razfmqsey2mg08l75refatytt504gh2cl6ld52qe5k47pp5n4zaukxzc7c4cnasd825qvpm3lss3k8vjuv8gus9qpue0ulgf5uk7gr2dwy4en3va";

Deno.test("loadConfig reads partial smoke env overrides", async () => {
  const previous = snapshotEnv(ENV_KEYS);
  const agentConfigPath = await writeTempAgentConfig();
  try {
    Deno.env.set("AGENT_CONFIG_PATH", agentConfigPath);
    Deno.env.set("PARTIAL_LEAVING_LOVELACE", "20000000");
    Deno.env.set("PARTIAL_EXPECTED_TOKEN_AMOUNT", "14900000");
    Deno.env.set("PARTIAL_FEE_LOVELACE", "2000000");
    Deno.env.set("PARTIAL_EXECUTION_TIMEOUT_MS", "900000");

    const config = await loadConfig();

    assertEquals(config.smokeExpectedTokenAmount, 900_000n);
    assertEquals(config.partialLeavingLovelace, 20_000_000n);
    assertEquals(config.partialExpectedTokenAmount, 14_900_000n);
    assertEquals(config.partialFeeLovelace, 2_000_000n);
    assertEquals(config.partialExecutionTimeoutMs, 900_000);
  } finally {
    await Deno.remove(agentConfigPath).catch(() => {});
    restoreEnv(previous);
  }
});

Deno.test("loadConfig reads generated wallet env from WALLET_ENV_PATH", async () => {
  const previous = snapshotEnv(ENV_KEYS);
  const agentConfigPath = await writeTempAgentConfig();
  const walletEnvPath = await Deno.makeTempFile({ prefix: "wallets-", suffix: ".env" });
  try {
    Deno.env.set("AGENT_CONFIG_PATH", agentConfigPath);
    Deno.env.delete("FUNDED_WALLET_SEED");
    Deno.env.set("WALLET_ENV_PATH", walletEnvPath);
    await Deno.writeTextFile(walletEnvPath, "FUNDED_WALLET_SEED=fresh generated seed phrase\n");

    const config = await loadConfig();

    assertEquals(config.fundedWalletSeed, "fresh generated seed phrase");
  } finally {
    await Deno.remove(agentConfigPath).catch(() => {});
    await Deno.remove(walletEnvPath).catch(() => {});
    restoreEnv(previous);
  }
});

Deno.test("loadConfig rejects invalid partial execution timeout", async () => {
  const previous = snapshotEnv(ENV_KEYS);
  const agentConfigPath = await writeTempAgentConfig();
  try {
    Deno.env.set("AGENT_CONFIG_PATH", agentConfigPath);
    Deno.env.set("PARTIAL_EXECUTION_TIMEOUT_MS", "bad");
    await assertRejects(() => loadConfig(), Error, "PARTIAL_EXECUTION_TIMEOUT_MS");
  } finally {
    await Deno.remove(agentConfigPath).catch(() => {});
    restoreEnv(previous);
  }
});

async function writeTempAgentConfig(): Promise<string> {
  const path = await Deno.makeTempFile({ prefix: "agent-config-", suffix: ".json" });
  await Deno.writeTextFile(path, `${JSON.stringify({ operatorKey: TEST_OPERATOR_KEY }, null, 2)}\n`);
  return path;
}

function snapshotEnv(keys: readonly string[]): Map<string, string | undefined> {
  return new Map(keys.map((key) => [key, Deno.env.get(key)]));
}

function restoreEnv(snapshot: Map<string, string | undefined>): void {
  for (const [key, value] of snapshot) {
    if (value === undefined) Deno.env.delete(key);
    else Deno.env.set(key, value);
  }
}
