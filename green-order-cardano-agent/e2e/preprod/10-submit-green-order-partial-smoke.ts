import { credentialToAddress } from "npm:@lucid-evolution/utils@0.1.65";
import { parseAgentAcceptedResponse, submitIntent } from "./src/agent.ts";
import { attachAccountOutputRefToEntitlement, deriveCompressedPublicKey, signIntentDigest } from "./src/aleph.ts";
import { parseAccountDatum } from "./src/account_datum.ts";
import { loadConfig } from "./src/config.ts";
import { adaAsset, alephIntentionDigest, buildIntentPayload, nativeAsset } from "./src/intent.ts";
import { koiosUtxoByRef, koiosUtxoByRefWithSpent, koiosUtxosAt, SimpleUtxo } from "./src/koios.ts";
import { computePartialFillPlan, PartialFillPlan } from "./src/partial_preflight.ts";
import { loadState, saveState } from "./src/state.ts";
import { waitFor } from "./src/wait.ts";

const dryRun = Deno.args.includes("--dry-run");
const verifyExecutionTx = Deno.args.find((arg) => arg.startsWith("--verify-execution-tx="))
  ?.slice("--verify-execution-tx=".length);
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
const oldAccountForVerification = verifyExecutionTx
  ? await koiosUtxoByRefWithSpent(state.account.outputRef, true)
  : oldAccount;
if (!oldAccountForVerification) throw new Error("current account output is not available on preprod");
if (!oldAccountForVerification.datum) {
  throw new Error(`current account output ${refToString(state.account.outputRef)} has no inline datum`);
}
const oldPool = await koiosUtxoByRef(state.pool.outputRef);
const oldPoolForVerification = verifyExecutionTx
  ? await koiosUtxoByRefWithSpent(state.pool.outputRef, true)
  : oldPool;
if (!oldPoolForVerification) throw new Error("current pool output is not available on preprod");

const preflight = await computePartialFillPlan({
  poolLovelaceReserve: oldPoolForVerification.assets.lovelace ?? 0n,
  poolTokenReserve: oldPoolForVerification.assets[state.pool.assetY] ?? 0n,
  poolAssetY: state.pool.assetY,
  leavingLovelace: config.partialLeavingLovelace,
  expectedTokenAmount: config.partialExpectedTokenAmount,
  feeLovelace: config.partialFeeLovelace,
});
console.log(`partial preflight: ${JSON.stringify(stringifyPlan(preflight))}`);

if (!verifyExecutionTx) {
  if (!oldAccount) throw new Error("current account output is not available on preprod");
  if (!oldPool) throw new Error("current pool output is not available on preprod");
  const response = await submitIntent(config.agentUrl, payload);
  parseAgentAcceptedResponse(response);
  console.log("partial_intent_status=accepted");
}

if (!verifyExecutionTx) {
  await waitFor("old Aleph account output to be spent", async () => {
    const utxo = await koiosUtxoByRef(state.account!.outputRef);
    return utxo ? undefined : true;
  }, { timeoutMs: config.partialExecutionTimeoutMs });
  await waitFor("old Royalty V1 pool output to be spent", async () => {
    const utxo = await koiosUtxoByRef(state.pool!.outputRef);
    return utxo ? undefined : true;
  }, { timeoutMs: config.partialExecutionTimeoutMs });
}
const newPool = verifyExecutionTx
  ? await requireUtxoByRef({ txHash: verifyExecutionTx, outputIndex: state.pool.outputRef.outputIndex })
  : await waitFor("advanced Royalty V1 pool output", async () => {
    const utxos = await koiosUtxosAt(poolAddress);
    return utxos.find((utxo) =>
      utxo.txHash !== state.pool!.outputRef.txHash && utxo.assets[state.pool!.poolNft] === 1n
    );
  }, { timeoutMs: config.partialExecutionTimeoutMs });
const newAccount = verifyExecutionTx
  ? await requireUtxoByRef({ txHash: verifyExecutionTx, outputIndex: state.account.outputRef.outputIndex })
  : await waitFor("new Aleph account output from partial execution tx", async () => {
    const utxos = await koiosUtxosAt(accountAddress);
    return utxos.find((utxo) => utxo.txHash === newPool.txHash);
  }, { timeoutMs: config.partialExecutionTimeoutMs });

const oldParsed = parseAccountDatum(oldAccountForVerification.datum, alephAbi);
const newParsed = assertPartialAccountAdvanced(
  oldAccountForVerification,
  newAccount,
  state.pool.assetY,
  config.partialLeavingLovelace,
  state.account.mainKeyHex,
  alephAbi,
);
const actualFill = actualPartialFill(oldAccountForVerification, newAccount, state.pool.assetY);
assertPoolAdvanced(oldPoolForVerification, newPool, state.pool.assetY, actualFill);
const actualPlan = actualPartialFillPlan(preflight, actualFill);

let storePath: string | undefined;
if (!verifyExecutionTx) {
  storePath = await accountStorePath(config.agentConfigPath);
  await waitFor("agent to persist partial continuation store", async () => {
    assertPersistedPartialStore(storePath!, {
      accountId: state.account!.accountId,
      outputRef: { txHash: newAccount.txHash, outputIndex: newAccount.outputIndex },
      rootHex: newParsed.storeRootHex,
      remainingDigestHex: remainingIntentDigestHex(actualPlan),
      preflight: actualPlan,
    });
    return true;
  }, { timeoutMs: config.partialExecutionTimeoutMs });
}

if (verifyExecutionTx) {
  await savePartialState({
    state,
    accountRef: { txHash: newAccount.txHash, outputIndex: newAccount.outputIndex },
    poolRef: { txHash: newPool.txHash, outputIndex: newPool.outputIndex },
    submittedIntentDigest: digestHex,
    executionTxHash: newAccount.txHash,
    oldStoreRootHex: oldParsed.storeRootHex,
    newStoreRootHex: newParsed.storeRootHex,
    receivedAmount: actualPlan.receivedOutput.toString(),
    storePath: await verifiedStorePath(config.agentConfigPath),
  });
  console.log(`Verified partial green order smoke intent in tx ${newAccount.txHash}`);
  Deno.exit(0);
}

await savePartialState({
  state,
  accountRef: { txHash: newAccount.txHash, outputIndex: newAccount.outputIndex },
  poolRef: { txHash: newPool.txHash, outputIndex: newPool.outputIndex },
  submittedIntentDigest: digestHex,
  executionTxHash: newAccount.txHash,
  oldStoreRootHex: oldParsed.storeRootHex,
  newStoreRootHex: newParsed.storeRootHex,
  receivedAmount: actualPlan.receivedOutput.toString(),
  storePath: storePath ?? "",
});
console.log(
  `${
    verifyExecutionTx ? "Verified" : "Submitted"
  } partial green order smoke intent in tx ${newAccount.txHash}`,
);

type ParsedCurrentAccount = ReturnType<typeof parseAccountDatum> & { storeRootHex: string };
type OutputRefLike = { txHash: string; outputIndex: number };

function assertPartialAccountAdvanced(
  oldAccount: SimpleUtxo,
  newAccount: SimpleUtxo,
  receivedAsset: string,
  originalLeavingLovelace: bigint,
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
  if (receivedDelta <= 0n) {
    throw new Error(`successor account received non-positive delta ${receivedDelta}`);
  }
  const lovelaceDelta = (oldAccount.assets.lovelace ?? 0n) - (newAccount.assets.lovelace ?? 0n);
  if (lovelaceDelta <= 0n || lovelaceDelta >= originalLeavingLovelace) {
    throw new Error(`successor account lovelace decreased by non-partial amount ${lovelaceDelta}`);
  }
  return parsed as ParsedCurrentAccount;
}

function actualPartialFill(
  oldAccount: SimpleUtxo,
  newAccount: SimpleUtxo,
  receivedAsset: string,
): Pick<PartialFillPlan, "consumedLeaving" | "receivedOutput"> {
  return {
    consumedLeaving: (oldAccount.assets.lovelace ?? 0n) - (newAccount.assets.lovelace ?? 0n),
    receivedOutput: (newAccount.assets[receivedAsset] ?? 0n) - (oldAccount.assets[receivedAsset] ?? 0n),
  };
}

function actualPartialFillPlan(
  preflight: PartialFillPlan,
  actualFill: Pick<PartialFillPlan, "consumedLeaving" | "receivedOutput">,
): PartialFillPlan {
  if (actualFill.consumedLeaving <= 0n || actualFill.consumedLeaving >= config.partialLeavingLovelace) {
    throw new Error(`actual consumed amount is not a strict partial fill: ${actualFill.consumedLeaving}`);
  }
  if (actualFill.receivedOutput <= 0n || actualFill.receivedOutput >= config.partialExpectedTokenAmount) {
    throw new Error(`actual received amount is not a strict partial fill: ${actualFill.receivedOutput}`);
  }
  const consumedDrift = absDiff(actualFill.consumedLeaving, preflight.consumedLeaving);
  const outputDrift = absDiff(actualFill.receivedOutput, preflight.receivedOutput);
  if (consumedDrift > 10n || outputDrift > 10n) {
    throw new Error(
      `actual partial fill drift is too large: consumed ${actualFill.consumedLeaving} vs ${preflight.consumedLeaving}, ` +
        `output ${actualFill.receivedOutput} vs ${preflight.receivedOutput}`,
    );
  }
  const remainingLeaving = config.partialLeavingLovelace - actualFill.consumedLeaving;
  return {
    consumedLeaving: actualFill.consumedLeaving,
    receivedOutput: actualFill.receivedOutput,
    remainingLeaving,
    remainingExpectedOutput: config.partialExpectedTokenAmount - actualFill.receivedOutput,
    remainingFee: remainingLeaving * config.partialFeeLovelace / config.partialLeavingLovelace,
  };
}

function assertPoolAdvanced(
  oldPool: SimpleUtxo,
  newPool: SimpleUtxo,
  paidAsset: string,
  expected: Pick<PartialFillPlan, "consumedLeaving" | "receivedOutput">,
): void {
  const lovelaceDelta = (newPool.assets.lovelace ?? 0n) - (oldPool.assets.lovelace ?? 0n);
  if (lovelaceDelta !== expected.consumedLeaving) {
    throw new Error(
      `pool lovelace reserve increased by ${lovelaceDelta}, expected ${expected.consumedLeaving}`,
    );
  }
  const tokenDelta = (oldPool.assets[paidAsset] ?? 0n) - (newPool.assets[paidAsset] ?? 0n);
  if (tokenDelta !== expected.receivedOutput) {
    throw new Error(`pool token reserve decreased by ${tokenDelta}, expected ${expected.receivedOutput}`);
  }
}

async function accountStorePath(agentConfigPath: string): Promise<string> {
  const raw = JSON.parse(await Deno.readTextFile(agentConfigPath));
  const dbPath = String(raw?.chainSync?.dbPath ?? "").trim();
  if (!dbPath) throw new Error(`${agentConfigPath}.chainSync.dbPath is missing`);
  return `${dbPath}.green-account-stores.json`;
}

async function verifiedStorePath(agentConfigPath: string): Promise<string> {
  try {
    return await accountStorePath(agentConfigPath);
  } catch {
    return "verified-existing-execution-tx";
  }
}

async function savePartialState(args: {
  state: typeof state;
  accountRef: OutputRefLike;
  poolRef: OutputRefLike;
  submittedIntentDigest: string;
  executionTxHash: string;
  oldStoreRootHex?: string;
  newStoreRootHex?: string;
  receivedAmount: string;
  storePath: string;
}): Promise<void> {
  const nextState = attachAccountOutputRefToEntitlement({
    ...args.state,
    account: {
      ...args.state.account!,
      outputRef: args.accountRef,
    },
    pool: {
      ...args.state.pool!,
      outputRef: args.poolRef,
    },
    partialSmoke: {
      submittedIntentDigest: args.submittedIntentDigest,
      executionTxHash: args.executionTxHash,
      oldStoreRootHex: args.oldStoreRootHex,
      newStoreRootHex: args.newStoreRootHex,
      receivedAmount: args.receivedAmount,
      storePath: args.storePath,
    },
  }, args.accountRef);
  await saveState(config.statePath, nextState);
}

async function requireUtxoByRef(ref: OutputRefLike): Promise<SimpleUtxo> {
  const utxo = await koiosUtxoByRef({ txHash: ref.txHash, outputIndex: ref.outputIndex });
  if (!utxo) throw new Error(`expected live output ${refToString(ref)}`);
  return utxo;
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

function absDiff(left: bigint, right: bigint): bigint {
  return left > right ? left - right : right - left;
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
