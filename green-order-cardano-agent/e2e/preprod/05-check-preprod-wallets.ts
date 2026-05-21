import { preprodProvider, loadPreprodEnv, lovelaceToAda } from "./src/provider.ts";

await loadPreprodEnv();

const batcherAddress = requiredEnv("BATCHER_ADDRESS");
const deploymentAddress = requiredEnv("DEPLOYMENT_ADDRESS");
if (!batcherAddress.startsWith("addr_test1") || !deploymentAddress.startsWith("addr_test1")) {
  throw new Error("wallet addresses are not testnet addresses");
}

const provider = preprodProvider();
for (const [role, address] of [
  ["batcher", batcherAddress],
  ["deployment", deploymentAddress],
] as const) {
  const utxos = await provider.getUtxos(address);
  const lovelace = utxos.reduce((sum, utxo) => sum + (utxo.assets.lovelace ?? 0n), 0n);
  console.log(`${role}: ${lovelaceToAda(lovelace)} tADA across ${utxos.length} UTxOs`);
  console.log(`${role}_address=${address}`);
}

function requiredEnv(name: string): string {
  const value = Deno.env.get(name)?.trim();
  if (!value) throw new Error(`Set ${name}`);
  return value;
}
