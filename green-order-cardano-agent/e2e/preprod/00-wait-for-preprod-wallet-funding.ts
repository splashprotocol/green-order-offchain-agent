import { loadPreprodEnv, lovelaceToAda, preprodProvider } from "./src/provider.ts";

await loadPreprodEnv();

const address = requiredEnv("BATCHER_ADDRESS");
const requiredLovelace = BigInt(Deno.env.get("E2E_BATCHER_REQUIRED_LOVELACE")?.trim() || "350000000");
const intervalMs = Number(Deno.env.get("E2E_BATCHER_FUNDING_POLL_MS")?.trim() || "15000");
const timeoutMs = Number(Deno.env.get("E2E_BATCHER_FUNDING_TIMEOUT_MS")?.trim() || "3600000");
const startedAt = Date.now();
const provider = preprodProvider();

console.log(`Waiting for at least ${lovelaceToAda(requiredLovelace)} tADA at batcher address:`);
console.log(address);

while (Date.now() - startedAt < timeoutMs) {
  const utxos = await provider.getUtxos(address);
  const lovelace = utxos.reduce((sum, utxo) => sum + (utxo.assets.lovelace ?? 0n), 0n);
  console.log(`batcher funding observed: ${lovelaceToAda(lovelace)} tADA across ${utxos.length} UTxOs`);
  if (lovelace >= requiredLovelace) Deno.exit(0);
  await new Promise((resolve) => setTimeout(resolve, intervalMs));
}

throw new Error(
  `Timed out waiting for ${lovelaceToAda(requiredLovelace)} tADA at ${address}`,
);

function requiredEnv(name: string): string {
  const value = Deno.env.get(name)?.trim();
  if (!value) throw new Error(`Set ${name}`);
  return value;
}
