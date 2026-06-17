import { Constr, Data } from "npm:@lucid-evolution/lucid@0.3.53";
import { credentialToAddress } from "npm:@lucid-evolution/utils@0.1.65";
import {
  accountIdFromMainKey,
  ALEPH_ACCOUNT_MAGIC_HEX,
  ALEPH_EMPTY_MPF_ROOT_HEX,
  attachAccountOutputRefToEntitlement,
  deriveCompressedPublicKey,
  randomHotPrivateKeyHex,
  requireBatchWitnessEntitlement,
} from "./src/aleph.ts";
import { loadConfig } from "./src/config.ts";
import { getLucid, submitSignedTx } from "./src/lucid.ts";
import { bindAccountViaSdk, loadGreenOrderSdkClient } from "./src/sdk_agent.ts";
import { loadState, OutputRef, PreprodE2eState, saveState } from "./src/state.ts";
import { waitFor } from "./src/wait.ts";

const dryRun = Deno.args.includes("--dry-run");
const force = Deno.args.includes("--force");
const createOnly = Deno.args.includes("--create-only");
const bindOnly = Deno.args.includes("--bind-only");

if (createOnly && bindOnly) {
  throw new Error("--create-only and --bind-only are mutually exclusive");
}

const config = await loadConfig();
let state = await loadState(config.statePath);
const entitlement = requireBatchWitnessEntitlement(state, config.deployment.alephBatchWitness.hash);

if (state.account && !force) {
  const account = state.account;
  state = { ...state, pendingAccount: account };
  state = attachAccountOutputRefToEntitlement(state, account.outputRef);
  delete state.account;
}

let pending = force ? undefined : state.pendingAccount;
if (bindOnly && !pending) {
  throw new Error("No pending account to bind; run without --bind-only first");
}
if (!pending) {
  const hotPrivateKeyHex = force
    ? randomHotPrivateKeyHex()
    : config.accountHotPrivateKeyHex ?? randomHotPrivateKeyHex();
  const mainKeyHex = deriveCompressedPublicKey(hotPrivateKeyHex);
  pending = {
    alephAbi: "current",
    accountId: accountIdFromMainKey(mainKeyHex),
    hotPrivateKeyHex,
    mainKeyHex,
    coldKeyHash: config.operatorKeyHashHex,
    initialStoreRoot: ALEPH_EMPTY_MPF_ROOT_HEX,
    batchWitnessHash: entitlement.hash,
  };
  if (!force) {
    state = { ...state, pendingAccount: pending };
    if (!dryRun) await saveState(config.statePath, state);
  }
}

console.log(`Aleph account script hash: ${config.deployment.alephAccount.hash}`);
console.log(`Aleph account ABI: ${pending.alephAbi ?? "current"}`);
console.log(`Aleph batch witness hash: ${pending.batchWitnessHash}`);
console.log(`Account id: ${pending.accountId}`);
console.log(`Initial MPF root: ${pending.initialStoreRoot}`);
console.log(`Account lovelace: ${config.accountInitialLovelace}`);

if (dryRun) Deno.exit(0);

const lucid = await getLucid(config);
console.log("Lucid provider initialized");
const accountAddress = credentialToAddress("Preprod", {
  type: "Script",
  hash: config.deployment.alephAccount.hash,
});

if (!pending.outputRef) {
  const datum = buildAlephAccountDatum(pending);
  console.log("Building Aleph account creation transaction");
  const tx = await lucid
    .newTx()
    .pay.ToAddressWithData(accountAddress, { kind: "inline", value: datum }, {
      lovelace: config.accountInitialLovelace,
    })
    .complete();
  console.log("Signing and submitting Aleph account creation transaction");
  const txHash = await submitSignedTx(await tx.sign.withWallet().complete());
  console.log(`Submitted Aleph account creation transaction ${txHash}`);
  const outputRef = await waitFor("created Aleph account output", async () => {
    const utxos = await lucid.utxosAt(accountAddress);
    const found = utxos.find((utxo) => utxo.txHash === txHash);
    return found ? { txHash: found.txHash, outputIndex: found.outputIndex } : undefined;
  });
  console.log(`Observed Aleph account output ${outputRef.txHash}#${outputRef.outputIndex}`);
  pending = { ...pending, outputRef };
  if (!force || createOnly) {
    state = attachAccountOutputRefToEntitlement({ ...state, pendingAccount: pending }, outputRef);
    await saveState(config.statePath, state);
  }
}

if (createOnly) {
  console.log(
    `Created Aleph account ${pending.accountId} at ${(pending.outputRef as OutputRef).txHash}#${
      (pending.outputRef as OutputRef).outputIndex
    }`,
  );
  Deno.exit(0);
}

console.log(`Binding Aleph account ${pending.accountId} to agent`);
const sdkClient = await loadGreenOrderSdkClient(config.agentUrl, {
  secret: config.agentHmacSecret,
  keyId: config.agentHmacKeyId,
});
let bindAttempts = 0;
const bindProgressEvery = Number(Deno.env.get("ACCOUNT_BIND_PROGRESS_EVERY") ?? "10");
await waitFor("agent account binding", async () => {
  const body = await bindAccountViaSdk(sdkClient, pending.accountId, pending.outputRef as OutputRef).catch(
    async (error) => {
      if (String(error).includes("outputNotObserved")) {
        bindAttempts += 1;
        if (bindProgressEvery > 0 && bindAttempts % bindProgressEvery === 0) {
          const monitoring = await sdkClient.getMonitoringSummary().catch((monitoringError) => ({
            status: "unavailable",
            reason: String(monitoringError),
          }));
          console.log(JSON.stringify(
            {
              accountBinding: {
                status: "waitingForAgentObservation",
                attempts: bindAttempts,
                accountId: pending.accountId,
                outputRef: pending.outputRef,
                monitoring,
              },
            },
            null,
            2,
          ));
        }
        return undefined;
      }
      if (force && String(error).includes("alreadyBound")) {
        throw new Error(`forced account bind was rejected as alreadyBound for ${pending.accountId}`);
      }
      if (String(error).includes("alreadyBound")) return { status: "alreadyBound" };
      throw error;
    },
  );
  if (!body) return undefined;
  const status = (body as { status?: string }).status;
  if (status === "bound" || status === "alreadyBound") return true;
  return undefined;
}, { timeoutMs: Number(Deno.env.get("ACCOUNT_BIND_TIMEOUT_MS") ?? "900000") });

const account = { ...pending, outputRef: pending.outputRef as OutputRef };
let finalState: PreprodE2eState = { ...state, account };
finalState = attachAccountOutputRefToEntitlement(finalState, account.outputRef);
delete finalState.pendingAccount;
await saveState(config.statePath, finalState);
console.log(
  `Bound account ${account.accountId} at ${account.outputRef.txHash}#${account.outputRef.outputIndex}`,
);

function buildAlephAccountDatum(account: {
  alephAbi?: "current" | "legacy";
  mainKeyHex: string;
  coldKeyHash: string;
  batchWitnessHash: string;
  initialStoreRoot: string;
}): string {
  if (account.alephAbi === "legacy") {
    return Data.to(
      new Constr(0, [
        ALEPH_ACCOUNT_MAGIC_HEX,
        0n,
        account.mainKeyHex,
        account.initialStoreRoot,
      ] as any) as any,
    );
  }
  return Data.to(
    new Constr(0, [
      ALEPH_ACCOUNT_MAGIC_HEX,
      [account.batchWitnessHash],
      [0n],
      [account.mainKeyHex, []],
      account.coldKeyHash,
      account.initialStoreRoot,
    ] as any) as any,
  );
}
