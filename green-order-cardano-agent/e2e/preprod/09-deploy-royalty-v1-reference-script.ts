import {
  Data,
  getAddressDetails,
  Lucid,
  Script,
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

const deploymentPath = Deno.env.get("DEPLOYMENT_PATH")?.trim() ?? "../../resources/preprod.deployment.json";
const splashBlueprintPath =
  Deno.env.get("SPLASH_BLUEPRINT_PATH")?.trim() ?? "../../../splash-testing-cardano/plutus.json";

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

const deployment = JSON.parse(await Deno.readTextFile(deploymentPath)) as Deployment;
const deploymentKey = "royaltyPoolLedgerFixed";
const royaltyPool = deployment[deploymentKey];
if (!royaltyPool) throw new Error(`${deploymentPath} is missing ${deploymentKey}`);

const splashBlueprint = JSON.parse(await Deno.readTextFile(splashBlueprintPath)) as {
  validators: BlueprintValidator[];
};
const royaltyPoolValidator = findValidator(
  splashBlueprint.validators,
  "royalty_pool/royalty_pool_pool.validate_pool",
);
const royaltyPoolScript: Script = {
  type: "PlutusV2",
  script: royaltyPoolValidator.compiledCode,
};
const royaltyPoolHash = validatorToScriptHash(royaltyPoolScript);
if (royaltyPoolHash !== royaltyPool.hash || royaltyPoolValidator.hash !== royaltyPool.hash) {
  throw new Error(
    `${deploymentKey} hash mismatch: deployment ${royaltyPool.hash}, blueprint ${royaltyPoolValidator.hash}, computed ${royaltyPoolHash}`,
  );
}
if (await currentReferenceUtxoIsLive(royaltyPool, walletAddress, royaltyPoolHash)) {
  console.log(`${deploymentKey} reference UTxO is already live`);
  console.log(`${deploymentKey}=${royaltyPool.referenceUtxo.txHash}#${royaltyPool.referenceUtxo.outputIndex}`);
  Deno.exit(0);
}

const utxos = await provider.getUtxos(walletAddress);
const balance = utxos.reduce((sum, utxo) => sum + (utxo.assets.lovelace ?? 0n), 0n);
if (balance < 8_000_000n) {
  throw new Error(`deployment wallet needs at least 8 tADA, found ${balance} lovelace`);
}

const markerDatum = Data.void();
const tx = await lucid
  .newTx()
  .pay.ToAddressWithData(
    walletAddress,
    { kind: "inline", value: markerDatum },
    { lovelace: 5_000_000n },
    royaltyPoolScript,
  )
  .addSignerKey(walletKeyHash)
  .complete();

const signed = await tx.sign.withWallet().complete();
if (Deno.env.get("DRY_RUN_DEPLOYMENT_TX") === "1") {
  console.log(await inspectDeploymentTx(signed.toCBOR()));
  Deno.exit(0);
}

const txHash = await submitSignedTx(signed);
console.log(`submitted_royalty_pool_ref_script_tx=${txHash}`);

await waitForKoiosTxOutput(txHash, walletAddress, royaltyPoolHash);
deployment[deploymentKey] = {
  ...royaltyPool,
  hash: royaltyPoolHash,
  referenceUtxo: { txHash, outputIndex: 0 },
};

await Deno.writeTextFile(deploymentPath, `${JSON.stringify(deployment, null, 2)}\n`);
console.log(`updated_deployment=${deploymentPath}`);
console.log(`${deploymentKey}=${txHash}#0`);

function requiredEnv(name: string): string {
  const value = Deno.env.get(name)?.trim();
  if (!value) throw new Error(`Set ${name}`);
  return value;
}

function findValidator(validators: BlueprintValidator[], title: string): BlueprintValidator {
  const validator = validators.find((item) => item.title === title);
  if (!validator) throw new Error(`${splashBlueprintPath} is missing ${title}`);
  return validator;
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

async function waitForKoiosTxOutput(
  txHash: string,
  expectedAddress: string,
  royaltyPoolHash: string,
): Promise<void> {
  const koiosUrl = Deno.env.get("KOIOS_PREPROD_URL")?.trim() ?? "https://preprod.koios.rest/api/v1";
  for (let attempt = 0; attempt < 30; attempt++) {
    try {
      const response = await fetch(`${koiosUrl}/tx_info`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ _tx_hashes: [txHash] }),
      });
      if (response.ok) {
        const txs = await response.json() as Array<{ outputs?: KoiosOutput[] }>;
        const output = (txs[0]?.outputs ?? []).find((row) =>
          row.tx_index === 0 && row.payment_addr?.bech32 === expectedAddress
        );
        if (isExpectedReferenceScript(output, royaltyPoolHash)) return;
      }
    } catch (error) {
      console.warn(`Koios tx_info attempt ${attempt + 1} failed: ${error}`);
    }
    await new Promise((resolve) => setTimeout(resolve, 5_000));
  }
  throw new Error(`could not confirm Royalty V1 reference script output for tx ${txHash}`);
}

type KoiosOutput = {
  tx_index: number;
  payment_addr?: { bech32?: string };
  value?: string;
  reference_script?: { hash?: string | null; type?: string | null };
  inline_datum?: { bytes?: string | null; value?: unknown };
};

async function currentReferenceUtxoIsLive(
  validator: DeploymentValidator,
  expectedAddress: string,
  expectedHash: string,
): Promise<boolean> {
  const ref = validator.referenceUtxo;
  if (!ref) return false;
  const koiosUrl = Deno.env.get("KOIOS_PREPROD_URL")?.trim() ?? "https://preprod.koios.rest/api/v1";
  try {
    const response = await fetch(`${koiosUrl}/utxo_info`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ _utxo_refs: [`${ref.txHash}#${ref.outputIndex}`], _extended: true }),
    });
    if (!response.ok) return false;
    const [output] = await response.json() as Array<KoiosOutput>;
    return Boolean(
      output &&
        output.tx_index === ref.outputIndex &&
        output.payment_addr?.bech32 === expectedAddress &&
        isExpectedReferenceScript(output, expectedHash),
    );
  } catch {
    return false;
  }
}

function isExpectedReferenceScript(output: KoiosOutput | undefined, expectedHash: string): boolean {
  return Boolean(
    output?.reference_script?.hash === expectedHash &&
      output.reference_script.type === "PlutusV2" &&
      output.inline_datum?.bytes,
  );
}
