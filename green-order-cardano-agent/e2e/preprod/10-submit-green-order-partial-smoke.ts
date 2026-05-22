import { credentialToAddress } from "npm:@lucid-evolution/utils@0.1.65";
import { parseAgentAcceptedResponse, submitIntent } from "./src/agent.ts";
import { deriveCompressedPublicKey, signIntentDigest } from "./src/aleph.ts";
import { parseAccountDatum } from "./src/account_datum.ts";
import { loadConfig } from "./src/config.ts";
import { adaAsset, alephIntentionDigest, buildIntentPayload, nativeAsset } from "./src/intent.ts";
import { koiosUtxoByRef, koiosUtxosAt, SimpleUtxo } from "./src/koios.ts";
import { computePartialFillPlan, PartialFillPlan } from "./src/partial_preflight.ts";
import { loadState, saveState } from "./src/state.ts";
import { waitFor } from "./src/wait.ts";

const dryRun = Deno.args.includes("--dry-run");
const config = await loadConfig();
const state = await loadState(config.statePath);

if (!state.pool) throw new Error("Run 01-create-royalty-v1-pool.ts first");
if (!state.account) throw new Error("Run 03-create-aleph-account-and-bind.ts first");
if (deriveCompressedPublicKey(state.account.hotPrivateKeyHex) !== state.account.mainKeyHex) {
  throw new Error("state.account.hotPrivateKeyHex does not derive state.account.mainKeyHex");
}

const outputAsset = nativeAsset(state.pool.assetY.slice(0, 56), state.pool.assetY.slice(56));
const alephAbi = state.account.alephAbi ?? "current";
const targetNonceValue = alephAbi === "legacy" ? 0n : 1n;
const digestHex = alephIntentionDigest({
  alephAbi,
  targetNonceSlot: 0,
  targetNonceValue,
  inputAsset: adaAsset(),
  leavingAmount: config.partialLeavingLovelace,
  outputAsset,
  expectedArrivingAmount: config.partialExpectedTokenAmount,
  feeLovelace: config.partialFeeLovelace,
  operatorKeyHash: config.operatorKeyHashHex,
});
const signature = signIntentDigest(state.account.hotPrivateKeyHex, digestHex);

const payload = buildIntentPayload({
  accountId: state.account.accountId,
  originalIntentDigest: digestHex,
  inputAsset: adaAsset(),
  outputAsset,
  leavingAmount: config.partialLeavingLovelace,
  expectedArrivingAmount: config.partialExpectedTokenAmount,
  feeLovelace: config.partialFeeLovelace,
  targetNonceSlot: 0,
  targetNonceValue,
  operatorKeyHash: config.operatorKeyHashHex,
  auth: { type: "sig", prefix: "", postfix: "", signature, updateProof: "" },
});

console.log(JSON.stringify(payload, null, 2));
if (dryRun) Deno.exit(0);

const accountAddress = credentialToAddress("Preprod", {
  type: "Script",
  hash: config.deployment.alephAccount.hash,
});
const poolAddress = credentialToAddress("Preprod", {
  type: "Script",
  hash: config.deployment[state.pool.validator].hash,
});

const oldAccount = await koiosUtxoByRef(state.account.outputRef);
if (!oldAccount) throw new Error("current account output is not available on preprod");
if (!oldAccount.datum) {
  throw new Error(`current account output ${refToString(state.account.outputRef)} has no inline datum`);
}
const oldPool = await koiosUtxoByRef(state.pool.outputRef);
if (!oldPool) throw new Error("current pool output is not available on preprod");

const preflight = await computePartialFillPlan({
  poolLovelaceReserve: oldPool.assets.lovelace ?? 0n,
  poolTokenReserve: oldPool.assets[state.pool.assetY] ?? 0n,
  poolAssetY: state.pool.assetY,
  leavingLovelace: config.partialLeavingLovelace,
  expectedTokenAmount: config.partialExpectedTokenAmount,
  feeLovelace: config.partialFeeLovelace,
});
console.log(`partial preflight: ${JSON.stringify(stringifyPlan(preflight))}`);

const response = await submitIntent(config.agentUrl, payload);
parseAgentAcceptedResponse(response);

await waitFor("old Aleph account output to be spent", async () => {
  const utxo = await koiosUtxoByRef(state.account!.outputRef);
  return utxo ? undefined : true;
}, { timeoutMs: config.partialExecutionTimeoutMs });
await waitFor("old Royalty V1 pool output to be spent", async () => {
  const utxo = await koiosUtxoByRef(state.pool!.outputRef);
  return utxo ? undefined : true;
}, { timeoutMs: config.partialExecutionTimeoutMs });
const newPool = await waitFor("advanced Royalty V1 pool output", async () => {
  const utxos = await koiosUtxosAt(poolAddress);
  return utxos.find((utxo) =>
    utxo.txHash !== state.pool!.outputRef.txHash && utxo.assets[state.pool!.poolNft] === 1n
  );
}, { timeoutMs: config.partialExecutionTimeoutMs });
const newAccount = await waitFor("new Aleph account output from partial execution tx", async () => {
  const utxos = await koiosUtxosAt(accountAddress);
  return utxos.find((utxo) => utxo.txHash === newPool.txHash);
}, { timeoutMs: config.partialExecutionTimeoutMs });

const oldParsed = parseAccountDatum(oldAccount.datum, alephAbi);
const newParsed = assertPartialAccountAdvanced(
  oldAccount,
  newAccount,
  state.pool.assetY,
  preflight,
  state.account.mainKeyHex,
  alephAbi,
);
assertPoolAdvanced(oldPool, newPool, state.pool.assetY, preflight);

const storePath = await accountStorePath(config.agentConfigPath);
await waitFor("agent to persist partial continuation store", async () => {
  assertPersistedPartialStore(storePath, {
    accountId: state.account!.accountId,
    outputRef: { txHash: newAccount.txHash, outputIndex: newAccount.outputIndex },
    rootHex: newParsed.storeRootHex,
    remainingDigestHex: remainingIntentDigestHex(preflight),
    preflight,
  });
  return true;
}, { timeoutMs: config.partialExecutionTimeoutMs });

await saveState(config.statePath, {
  ...state,
  account: {
    ...state.account,
    outputRef: { txHash: newAccount.txHash, outputIndex: newAccount.outputIndex },
  },
  pool: {
    ...state.pool,
    outputRef: { txHash: newPool.txHash, outputIndex: newPool.outputIndex },
  },
  partialSmoke: {
    submittedIntentDigest: digestHex,
    executionTxHash: newAccount.txHash,
    oldStoreRootHex: oldParsed.storeRootHex,
    newStoreRootHex: newParsed.storeRootHex,
    receivedAmount: preflight.receivedOutput.toString(),
    storePath,
  },
});
console.log(`Submitted partial green order smoke intent in tx ${newAccount.txHash}`);

type ParsedCurrentAccount = ReturnType<typeof parseAccountDatum> & { storeRootHex: string };
type OutputRefLike = { txHash: string; outputIndex: number };

function assertPartialAccountAdvanced(
  oldAccount: SimpleUtxo,
  newAccount: SimpleUtxo,
  receivedAsset: string,
  preflight: PartialFillPlan,
  expectedMainKeyHex: string,
  alephAbi: "current" | "legacy",
): ParsedCurrentAccount {
  if (!newAccount.datum) throw new Error("successor account output must have inline datum");
  const parsed = parseAccountDatum(newAccount.datum, alephAbi);
  if (!parsed.storeRootHex) {
    throw new Error("partial smoke requires current Aleph account ABI with store root");
  }
  if (parsed.mainKeyHex !== expectedMainKeyHex) {
    throw new Error("successor account main_key does not match bound account");
  }
  if (parsed.nonce0 < 1n) throw new Error("successor account nonce[0] did not advance");
  const oldRoot = parseAccountDatum(oldAccount.datum!, alephAbi).storeRootHex;
  if (!oldRoot) throw new Error("partial smoke requires current Aleph account ABI with old store root");
  if (oldRoot === parsed.storeRootHex) {
    throw new Error("partial execution did not mutate Aleph MPF store root");
  }
  const receivedDelta = (newAccount.assets[receivedAsset] ?? 0n) - (oldAccount.assets[receivedAsset] ?? 0n);
  if (receivedDelta !== preflight.receivedOutput) {
    throw new Error(`successor account received ${receivedDelta}, expected ${preflight.receivedOutput}`);
  }
  const lovelaceDelta = (oldAccount.assets.lovelace ?? 0n) - (newAccount.assets.lovelace ?? 0n);
  if (lovelaceDelta !== preflight.consumedLeaving) {
    throw new Error(
      `successor account lovelace decreased by ${lovelaceDelta}, expected ${preflight.consumedLeaving}`,
    );
  }
  return parsed as ParsedCurrentAccount;
}

function assertPoolAdvanced(
  oldPool: SimpleUtxo,
  newPool: SimpleUtxo,
  paidAsset: string,
  preflight: PartialFillPlan,
): void {
  const lovelaceDelta = (newPool.assets.lovelace ?? 0n) - (oldPool.assets.lovelace ?? 0n);
  if (lovelaceDelta !== preflight.consumedLeaving) {
    throw new Error(
      `pool lovelace reserve increased by ${lovelaceDelta}, expected ${preflight.consumedLeaving}`,
    );
  }
  const tokenDelta = (oldPool.assets[paidAsset] ?? 0n) - (newPool.assets[paidAsset] ?? 0n);
  if (tokenDelta !== preflight.receivedOutput) {
    throw new Error(`pool token reserve decreased by ${tokenDelta}, expected ${preflight.receivedOutput}`);
  }
}

async function accountStorePath(agentConfigPath: string): Promise<string> {
  const raw = JSON.parse(await Deno.readTextFile(agentConfigPath));
  const dbPath = String(raw?.chainSync?.dbPath ?? "").trim();
  if (!dbPath) throw new Error(`${agentConfigPath}.chainSync.dbPath is missing`);
  return `${dbPath}.green-account-stores.json`;
}

function assertPersistedPartialStore(
  path: string,
  expected: {
    accountId: string;
    outputRef: OutputRefLike;
    rootHex: string;
    remainingDigestHex: string;
    preflight: PartialFillPlan;
  },
): void {
  const raw = JSON.parse(Deno.readTextFileSync(path));
  const stores = Array.isArray(raw?.stores) ? raw.stores : undefined;
  if (!stores) throw new Error(`${path} has unknown account store snapshot shape`);
  const store = stores.find((item: any) =>
    hexish(item.account_id) === expected.accountId &&
    outputRefEquals(item.output_ref, expected.outputRef)
  );
  if (!store) throw new Error(`persisted account store for ${refToString(expected.outputRef)} not found`);
  if (hexish(store.root) !== expected.rootHex) {
    throw new Error(`persisted store root ${hexish(store.root)} does not match datum ${expected.rootHex}`);
  }
  const leaves = Array.isArray(store.leaves) ? store.leaves : undefined;
  if (!leaves) throw new Error("persisted account store leaves are missing");
  const leaf = leaves.find((item: any) =>
    item.status === "Pending" && hexish(item.digest) === expected.remainingDigestHex
  );
  if (!leaf) throw new Error("pending continuation leaf for remaining intent digest not found");
  const intent = leaf.intent;
  if (!intent || typeof intent !== "object") throw new Error("pending continuation leaf intent is missing");
  assertBigIntField(intent, "leaving_amount", expected.preflight.remainingLeaving);
  assertBigIntField(intent, "expected_arriving_amount", expected.preflight.remainingExpectedOutput);
  assertBigIntField(intent, "fee_lovelace", expected.preflight.remainingFee);
}

function remainingIntentDigestHex(preflight: PartialFillPlan): string {
  return alephIntentionDigest({
    alephAbi,
    targetNonceSlot: 0,
    targetNonceValue,
    inputAsset: adaAsset(),
    leavingAmount: preflight.remainingLeaving,
    outputAsset,
    expectedArrivingAmount: preflight.remainingExpectedOutput,
    feeLovelace: preflight.remainingFee,
    operatorKeyHash: config.operatorKeyHashHex,
  });
}

function assertBigIntField(object: any, field: string, expected: bigint): void {
  const actual = BigInt(String(object[field]));
  if (actual !== expected) throw new Error(`pending continuation ${field}=${actual}, expected ${expected}`);
}

function outputRefEquals(actual: any, expected: OutputRefLike): boolean {
  if (typeof actual === "string") return actual === refToString(expected);
  return hexish(actual?.tx_hash) === expected.txHash &&
    Number(actual?.index ?? actual?.outputIndex) === expected.outputIndex;
}

function hexish(value: unknown): string {
  if (typeof value === "string") return value;
  if (Array.isArray(value)) return value.map((byte) => Number(byte).toString(16).padStart(2, "0")).join("");
  throw new Error(`cannot convert ${JSON.stringify(value)} to hex`);
}

function refToString(ref: OutputRefLike): string {
  return `${ref.txHash}#${ref.outputIndex}`;
}

function stringifyPlan(plan: PartialFillPlan): Record<string, string> {
  return {
    consumedLeaving: plan.consumedLeaving.toString(),
    receivedOutput: plan.receivedOutput.toString(),
    remainingLeaving: plan.remainingLeaving.toString(),
    remainingExpectedOutput: plan.remainingExpectedOutput.toString(),
    remainingFee: plan.remainingFee.toString(),
  };
}
