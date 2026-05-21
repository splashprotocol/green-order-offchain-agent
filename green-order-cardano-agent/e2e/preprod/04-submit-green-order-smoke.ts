import { credentialToAddress } from "npm:@lucid-evolution/utils@0.1.65";
import { Constr, Data } from "npm:@lucid-evolution/lucid@0.3.53";
import { parseAgentAcceptedResponse, submitIntent } from "./src/agent.ts";
import { deriveCompressedPublicKey, signIntentDigest } from "./src/aleph.ts";
import { loadConfig } from "./src/config.ts";
import { adaAsset, alephIntentionDigest, buildIntentPayload, nativeAsset } from "./src/intent.ts";
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
  leavingAmount: config.smokeLeavingLovelace,
  outputAsset,
  expectedArrivingAmount: config.smokeExpectedTokenAmount,
  feeLovelace: config.smokeFeeLovelace,
  operatorKeyHash: config.operatorKeyHashHex,
});
const signature = signIntentDigest(state.account.hotPrivateKeyHex, digestHex);

const payload = buildIntentPayload({
  accountId: state.account.accountId,
  originalIntentDigest: digestHex,
  inputAsset: adaAsset(),
  outputAsset,
  leavingAmount: config.smokeLeavingLovelace,
  expectedArrivingAmount: config.smokeExpectedTokenAmount,
  feeLovelace: config.smokeFeeLovelace,
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
  throw new Error(
    `current account output ${state.account.outputRef.txHash}#${state.account.outputRef.outputIndex} has no inline datum; recreate it with 03-create-aleph-account-and-bind.ts --force`,
  );
}
const oldPool = await koiosUtxoByRef(state.pool.outputRef);
if (!oldPool) throw new Error("current pool output is not available on preprod");

const response = await submitIntent(config.agentUrl, payload);
parseAgentAcceptedResponse(response);

await waitFor("old Aleph account output to be spent", async () => {
  const utxo = await koiosUtxoByRef(state.account!.outputRef);
  return utxo ? undefined : true;
});
await waitFor("old Royalty V1 pool output to be spent", async () => {
  const utxo = await koiosUtxoByRef(state.pool!.outputRef);
  return utxo ? undefined : true;
});
const newPool = await waitFor("advanced Royalty V1 pool output", async () => {
  const utxos = await koiosUtxosAt(poolAddress);
  return utxos.find((utxo) =>
    utxo.txHash !== state.pool!.outputRef.txHash && utxo.assets[state.pool!.poolNft] === 1n
  );
});
const newAccount = await waitFor("new Aleph account output from execution tx", async () => {
  const utxos = await koiosUtxosAt(accountAddress);
  return utxos.find((utxo) => utxo.txHash === newPool.txHash);
});

assertAccountAdvanced(
  oldAccount,
  newAccount,
  state.pool.assetY,
  config.smokeExpectedTokenAmount,
  config.smokeLeavingLovelace,
  config.smokeFeeLovelace,
  state.account.mainKeyHex,
  alephAbi,
);
assertPoolAdvanced(oldPool, newPool, state.pool.assetY);

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
  smoke: { submittedIntentDigest: digestHex, executionTxHash: newAccount.txHash },
});
console.log("Submitted green order smoke intent");

type SimpleUtxo = {
  txHash: string;
  outputIndex: number;
  datum?: string | null;
  assets: Record<string, bigint>;
};

type OutputRef = { txHash: string; outputIndex: number };

async function koiosUtxoByRef(ref: OutputRef): Promise<SimpleUtxo | undefined> {
  const rows = await koiosPost("utxo_info", {
    _utxo_refs: [`${ref.txHash}#${ref.outputIndex}`],
    _extended: true,
  });
  return rows.map(parseKoiosUtxo)[0];
}

async function koiosUtxosAt(address: string): Promise<SimpleUtxo[]> {
  const rows = await koiosPost("address_utxos", {
    _addresses: [address],
    _extended: true,
  });
  return rows.map(parseKoiosUtxo);
}

async function koiosPost(endpoint: string, body: unknown): Promise<any[]> {
  const baseUrl = Deno.env.get("KOIOS_PREPROD_URL")?.trim() ?? "https://preprod.koios.rest/api/v1";
  const response = await fetch(`${baseUrl}/${endpoint}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    throw new Error(`Koios ${endpoint} failed ${response.status}: ${await response.text()}`);
  }
  return await response.json();
}

function parseKoiosUtxo(row: any): SimpleUtxo {
  const assets: Record<string, bigint> = { lovelace: BigInt(row.value) };
  for (const asset of row.asset_list ?? []) {
    const unit = `${asset.policy_id}${asset.asset_name ?? ""}`;
    assets[unit] = BigInt(asset.quantity);
  }
  return {
    txHash: row.tx_hash,
    outputIndex: Number(row.tx_index),
    datum: row.inline_datum?.bytes ?? null,
    assets,
  };
}

function assertAccountAdvanced(
  oldAccount: { assets: Record<string, bigint> },
  newAccount: { datum?: string | null; assets: Record<string, bigint> },
  receivedAsset: string,
  expectedAmount: bigint,
  leavingAmount: bigint,
  feeLovelace: bigint,
  expectedMainKeyHex: string,
  alephAbi: "current" | "legacy",
): void {
  if (!newAccount.datum) throw new Error("successor account output must have inline datum");
  const parsed = parseAccountDatum(newAccount.datum, alephAbi);
  if (parsed.mainKeyHex !== expectedMainKeyHex) {
    throw new Error("successor account main_key does not match bound account");
  }
  if (parsed.nonce0 < 1n) {
    throw new Error("successor account nonce[0] did not advance");
  }
  const oldLovelace = oldAccount.assets.lovelace ?? 0n;
  const newLovelace = newAccount.assets.lovelace ?? 0n;
  const lovelaceDelta = newLovelace - oldLovelace;
  if (lovelaceDelta >= 0n || lovelaceDelta < -(leavingAmount + feeLovelace + 5_000_000n)) {
    throw new Error("successor account ADA delta is outside expected spend range");
  }
  const receivedDelta = (newAccount.assets[receivedAsset] ?? 0n) - (oldAccount.assets[receivedAsset] ?? 0n);
  if (receivedDelta < expectedAmount) {
    throw new Error("successor account did not receive expected output asset");
  }
}

function parseAccountDatum(
  datumCbor: string,
  alephAbi: "current" | "legacy",
): { nonce0: bigint; mainKeyHex: string } {
  const datum = Data.from(datumCbor) as any;
  if (alephAbi === "legacy") {
    if (!(datum instanceof Constr) || datum.index !== 0 || datum.fields.length !== 4) {
      throw new Error("successor legacy account datum has unexpected shape");
    }
    if (typeof datum.fields[1] !== "bigint" || typeof datum.fields[2] !== "string") {
      throw new Error("successor legacy account nonce or main_key is malformed");
    }
    return { nonce0: datum.fields[1], mainKeyHex: datum.fields[2] };
  }
  if (!(datum instanceof Constr) || datum.index !== 0 || datum.fields.length !== 6) {
    throw new Error("successor account datum has unexpected shape");
  }
  const nonce = datum.fields[2];
  if (!Array.isArray(nonce) || typeof nonce[0] !== "bigint") {
    throw new Error("successor account nonce[0] is missing or malformed");
  }
  const hotCred = datum.fields[3];
  if (!Array.isArray(hotCred) || typeof hotCred[0] !== "string") {
    throw new Error("successor account main_key is missing or malformed");
  }
  return { nonce0: nonce[0], mainKeyHex: hotCred[0] };
}

function assertPoolAdvanced(
  oldPool: { assets: Record<string, bigint> },
  newPool: { assets: Record<string, bigint> },
  paidAsset: string,
): void {
  if ((newPool.assets.lovelace ?? 0n) <= (oldPool.assets.lovelace ?? 0n)) {
    throw new Error("successor pool lovelace reserve did not increase");
  }
  if ((newPool.assets[paidAsset] ?? 0n) >= (oldPool.assets[paidAsset] ?? 0n)) {
    throw new Error("successor pool token reserve did not decrease");
  }
}
