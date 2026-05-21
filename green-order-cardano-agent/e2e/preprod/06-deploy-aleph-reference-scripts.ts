import {
  Data,
  getAddressDetails,
  Lucid,
  validatorToScriptHash,
} from "npm:@lucid-evolution/lucid@0.3.53";
import { preprodProvider, loadPreprodEnv } from "./src/provider.ts";

type BlueprintValidator = {
  title: string;
  compiledCode: string;
  hash: string;
};

type DeploymentValidator = {
  hash: string;
  referenceUtxo: { txHash: string; outputIndex: number };
  cost?: { mem: number; steps: number };
  marginalCost?: { mem: number; steps: number };
};

type Deployment = Record<string, DeploymentValidator>;

const blueprintPath = Deno.env.get("ALEPH_BLUEPRINT_PATH")?.trim() ?? "/Users/aleksandr/RustroverProjects/aleph/plutus.json";
const deploymentPath = Deno.env.get("DEPLOYMENT_PATH")?.trim() ?? "../../resources/preprod.deployment.json";

await loadPreprodEnv();

const seed = requiredEnv("DEPLOYMENT_WALLET_SEED");
const expectedAddress = requiredEnv("DEPLOYMENT_ADDRESS");
const provider = preprodProvider();
const lucid = await Lucid(provider, "Preprod");
lucid.selectWallet.fromSeed(seed, { addressType: "Enterprise" });

const walletAddress = await lucid.wallet().address();
if (walletAddress !== expectedAddress) {
  throw new Error(`deployment seed derives ${walletAddress}, expected ${expectedAddress}`);
}
if (!walletAddress.startsWith("addr_test1")) {
  throw new Error("deployment wallet is not a testnet address");
}

const walletKeyHash = getAddressDetails(walletAddress).paymentCredential?.hash;
if (!walletKeyHash) throw new Error("failed to derive deployment wallet payment key hash");

const blueprint = JSON.parse(await Deno.readTextFile(blueprintPath)) as { validators: BlueprintValidator[] };
const account = findValidator(blueprint.validators, "account.account.else");
const batchWitness =
  findValidatorOrNull(blueprint.validators, "intent.batch_witness.else") ??
  findValidator(blueprint.validators, "witness.batch_witness.else");

const accountScript = { type: "PlutusV3" as const, script: account.compiledCode };
const batchWitnessScript = { type: "PlutusV3" as const, script: batchWitness.compiledCode };

assertHash("alephAccount", account.hash, validatorToScriptHash(accountScript));
assertHash("alephBatchWitness", batchWitness.hash, validatorToScriptHash(batchWitnessScript));

const utxos = await provider.getUtxos(walletAddress);
const balance = utxos.reduce((sum, utxo) => sum + (utxo.assets.lovelace ?? 0n), 0n);
if (balance < 15_000_000n) {
  throw new Error(`deployment wallet needs at least 15 tADA, found ${balance} lovelace`);
}

const markerDatum = Data.void();
const tx = await lucid
  .newTx()
  .pay.ToAddressWithData(walletAddress, { kind: "inline", value: markerDatum }, { lovelace: 5_000_000n }, accountScript)
  .pay.ToAddressWithData(
    walletAddress,
    { kind: "inline", value: markerDatum },
    { lovelace: 5_000_000n },
    batchWitnessScript,
  )
  .addSignerKey(walletKeyHash)
  .complete();

const signed = await tx.sign.withWallet().complete();
if (Deno.env.get("DRY_RUN_DEPLOYMENT_TX") === "1") {
  console.log(await inspectDeploymentTx(signed.toCBOR()));
  Deno.exit(0);
}
const txHash = await submitSignedTx(signed);
console.log(`submitted_ref_script_tx=${txHash}`);

await waitForKoiosTxOutputs(txHash, walletAddress, account.hash, batchWitness.hash);
const refs = {
  alephAccount: { txHash, outputIndex: 0 },
  alephBatchWitness: { txHash, outputIndex: 1 },
};
const deployment = JSON.parse(await Deno.readTextFile(deploymentPath)) as Deployment;
deployment.alephAccount = {
  ...deployment.alephAccount,
  hash: account.hash,
  referenceUtxo: refs.alephAccount,
};
deployment.alephBatchWitness = {
  ...deployment.alephBatchWitness,
  hash: batchWitness.hash,
  referenceUtxo: refs.alephBatchWitness,
};

await Deno.writeTextFile(deploymentPath, `${JSON.stringify(deployment, null, 2)}\n`);
console.log(`updated_deployment=${deploymentPath}`);
console.log(`alephAccount=${refs.alephAccount.txHash}#${refs.alephAccount.outputIndex}`);
console.log(`alephBatchWitness=${refs.alephBatchWitness.txHash}#${refs.alephBatchWitness.outputIndex}`);

function requiredEnv(name: string): string {
  const value = Deno.env.get(name)?.trim();
  if (!value) throw new Error(`Set ${name}`);
  return value;
}

function findValidator(validators: BlueprintValidator[], title: string): BlueprintValidator {
  const validator = validators.find((item) => item.title === title);
  if (!validator) throw new Error(`blueprint is missing ${title}`);
  return validator;
}

function findValidatorOrNull(validators: BlueprintValidator[], title: string): BlueprintValidator | null {
  return validators.find((item) => item.title === title) ?? null;
}

function assertHash(name: string, expected: string, actual: string): void {
  if (expected !== actual) {
    throw new Error(`${name} script hash mismatch: blueprint ${expected}, computed ${actual}`);
  }
}

async function submitSignedTx(signed: { submit: () => Promise<string>; toCBOR: () => string; toHash: () => string }): Promise<string> {
  try {
    return await signed.submit();
  } catch (error) {
    const koiosUrl = Deno.env.get("KOIOS_PREPROD_URL")?.trim() ?? "https://preprod.koios.rest/api/v1";
    const response = await fetch(`${koiosUrl}/submittx`, {
      method: "POST",
      headers: { "content-type": "application/cbor" },
      body: hexToBytes(signed.toCBOR()),
    });
    if (!response.ok) {
      throw new Error(`provider submit failed (${error}); direct Koios submit failed ${response.status}: ${await response.text()}`);
    }
    const body = (await response.text()).trim().replace(/^"|"$/g, "");
    const expectedHash = signed.toHash();
    if (body && body !== expectedHash) {
      throw new Error(`direct Koios submit returned tx hash ${body}, expected ${expectedHash}`);
    }
    return expectedHash;
  }
}

function hexToBytes(hex: string): Uint8Array {
  if (hex.length % 2 !== 0) throw new Error("hex string has odd length");
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return bytes;
}

async function inspectDeploymentTx(cbor: string): Promise<string> {
  const { CML } = await import("npm:@lucid-evolution/lucid@0.3.53");
  const tx = CML.Transaction.from_cbor_hex(cbor);
  const body = tx.body();
  const lines = [`outputs=${body.outputs().len()}`];
  for (let i = 0; i < body.outputs().len(); i++) {
    const output = body.outputs().get(i);
    lines.push(
      `${i}: datum=${output.datum()?.kind() ?? "none"} script_ref=${
        output.script_ref()?.kind() ?? "none"
      }`,
    );
  }
  return lines.join("\n");
}

async function waitForKoiosTxOutputs(
  txHash: string,
  expectedAddress: string,
  accountHash: string,
  batchWitnessHash: string,
): Promise<void> {
  const koiosUrl = Deno.env.get("KOIOS_PREPROD_URL")?.trim() ?? "https://preprod.koios.rest/api/v1";
  for (let attempt = 0; attempt < 30; attempt++) {
    const response = await fetch(`${koiosUrl}/tx_info`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ _tx_hashes: [txHash] }),
    });
    if (response.ok) {
      const txs = await response.json() as Array<{
        outputs?: Array<{
          tx_index: number;
          payment_addr?: { bech32?: string };
          value?: string;
          reference_script?: { hash?: string | null; type?: string | null };
          inline_datum?: { bytes?: string | null; value?: unknown };
        }>;
      }>;
      const outputs = txs[0]?.outputs ?? [];
      const accountOutput = outputs.find((output) =>
        output.tx_index === 0 && output.payment_addr?.bech32 === expectedAddress && output.value === "5000000"
      );
      const batchWitnessOutput = outputs.find((output) =>
        output.tx_index === 1 && output.payment_addr?.bech32 === expectedAddress && output.value === "5000000"
      );
      if (
        accountOutput?.reference_script?.hash === accountHash &&
        accountOutput.reference_script.type === "PlutusV3" &&
        accountOutput.inline_datum?.bytes &&
        batchWitnessOutput?.reference_script?.hash === batchWitnessHash &&
        batchWitnessOutput.reference_script.type === "PlutusV3" &&
        batchWitnessOutput.inline_datum?.bytes
      ) return;
    }
    await new Promise((resolve) => setTimeout(resolve, 5_000));
  }
  throw new Error(`could not confirm Aleph reference script outputs for tx ${txHash}`);
}
