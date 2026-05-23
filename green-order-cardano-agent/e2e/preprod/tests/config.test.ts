import { assertEquals, assertRejects } from "https://deno.land/std@0.224.0/assert/mod.ts";
import { loadConfig } from "../src/config.ts";

const ENV_KEYS = [
  "FUNDED_WALLET_SEED",
  "WALLET_ENV_PATH",
  "PARTIAL_LEAVING_LOVELACE",
  "PARTIAL_EXPECTED_TOKEN_AMOUNT",
  "PARTIAL_FEE_LOVELACE",
  "PARTIAL_EXECUTION_TIMEOUT_MS",
] as const;

Deno.test("loadConfig reads partial smoke env overrides", async () => {
  const previous = snapshotEnv(ENV_KEYS);
  try {
    Deno.env.set("PARTIAL_LEAVING_LOVELACE", "20000000");
    Deno.env.set("PARTIAL_EXPECTED_TOKEN_AMOUNT", "14900000");
    Deno.env.set("PARTIAL_FEE_LOVELACE", "2000000");
    Deno.env.set("PARTIAL_EXECUTION_TIMEOUT_MS", "900000");

    const config = await loadConfig();

    assertEquals(config.partialLeavingLovelace, 20_000_000n);
    assertEquals(config.partialExpectedTokenAmount, 14_900_000n);
    assertEquals(config.partialFeeLovelace, 2_000_000n);
    assertEquals(config.partialExecutionTimeoutMs, 900_000);
  } finally {
    restoreEnv(previous);
  }
});

Deno.test("loadConfig reads generated wallet env from WALLET_ENV_PATH", async () => {
  const previous = snapshotEnv(ENV_KEYS);
  const walletEnvPath = await Deno.makeTempFile({ prefix: "wallets-", suffix: ".env" });
  try {
    Deno.env.delete("FUNDED_WALLET_SEED");
    Deno.env.set("WALLET_ENV_PATH", walletEnvPath);
    await Deno.writeTextFile(walletEnvPath, "FUNDED_WALLET_SEED=fresh generated seed phrase\n");

    const config = await loadConfig();

    assertEquals(config.fundedWalletSeed, "fresh generated seed phrase");
  } finally {
    await Deno.remove(walletEnvPath).catch(() => {});
    restoreEnv(previous);
  }
});

Deno.test("loadConfig rejects invalid partial execution timeout", async () => {
  const previous = snapshotEnv(ENV_KEYS);
  try {
    Deno.env.set("PARTIAL_EXECUTION_TIMEOUT_MS", "bad");
    await assertRejects(() => loadConfig(), Error, "PARTIAL_EXECUTION_TIMEOUT_MS");
  } finally {
    restoreEnv(previous);
  }
});

function snapshotEnv(keys: readonly string[]): Map<string, string | undefined> {
  return new Map(keys.map((key) => [key, Deno.env.get(key)]));
}

function restoreEnv(snapshot: Map<string, string | undefined>): void {
  for (const [key, value] of snapshot) {
    if (value === undefined) Deno.env.delete(key);
    else Deno.env.set(key, value);
  }
}
