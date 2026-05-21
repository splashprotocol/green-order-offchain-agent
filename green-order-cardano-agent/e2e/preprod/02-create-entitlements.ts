import { loadConfig } from "./src/config.ts";
import { batchWitnessRewardAddress, createEntitlementState } from "./src/aleph.ts";
import { getLucid, submitSignedTx } from "./src/lucid.ts";
import { preprodProvider } from "./src/provider.ts";
import { loadState, saveState } from "./src/state.ts";

const dryRun = Deno.args.includes("--dry-run");
const force = Deno.args.includes("--force");

const config = await loadConfig();
const state = await loadState(config.statePath);

const batchWitnessHash = config.deployment.alephBatchWitness.hash;
const rewardAddress = batchWitnessRewardAddress(batchWitnessHash);
const existing = state.entitlements?.find((item) => item.kind === "alephBatchWitnessAllowlist");
if (existing && existing.hash !== batchWitnessHash && !force) {
  throw new Error("Stored alephBatchWitness entitlement does not match deployment");
}

let entitlement = {
  ...createEntitlementState(batchWitnessHash)[0],
  ...(existing?.hash === batchWitnessHash ? existing : {}),
  rewardAddress,
};

console.log(`Aleph batch witness allowlist entitlement present: ${batchWitnessHash}`);
console.log(`Aleph batch witness reward address: ${rewardAddress}`);

if (entitlement.registrationTxHash && !force) {
  console.log(
    `Aleph batch witness reward credential already registered by ${entitlement.registrationTxHash}`,
  );
  Deno.exit(0);
}

if (!dryRun) {
  const lucid = await getLucid(config);
  const provider = preprodProvider();
  if (await isRewardRegistered(provider, rewardAddress)) {
    console.log("Aleph batch witness reward credential is already registered on-chain");
  } else {
    const tx = await lucid.newTx().registerStake(rewardAddress).complete();
    const txHash = await submitSignedTx(await tx.sign.withWallet().complete());
    const confirmed = await lucid.awaitTx(txHash);
    if (!confirmed) {
      throw new Error(`batch witness reward credential registration ${txHash} was not confirmed`);
    }
    entitlement = { ...entitlement, registrationTxHash: txHash };
    console.log(`Registered Aleph batch witness reward credential: ${txHash}`);
  }
  await saveState(config.statePath, { ...state, entitlements: [entitlement] });
}

async function isRewardRegistered(
  provider: ReturnType<typeof preprodProvider>,
  rewardAddress: string,
): Promise<boolean> {
  try {
    await provider.getDelegation(rewardAddress);
    return true;
  } catch {
    return false;
  }
}
