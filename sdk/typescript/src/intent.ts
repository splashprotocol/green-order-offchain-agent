import { blake2b } from "@noble/hashes/blake2b";
import { AssetClass, encodeAssetClass } from "./assets.js";
import { aikenBytesCbor, aikenConstr0Cbor, aikenTupleCbor, aikenUintCbor, cborHex } from "./cbor.js";
import { assertHex, bytesToHex, hexToBytes } from "./hex.js";

export type AlephAbi = "current" | "legacy";

export type SigAuthWire = {
  type: "sig";
  prefix: string;
  postfix: string;
  signature: string;
  updateProof: string;
};

export type IntentPayloadInput = {
  alephAbi?: AlephAbi;
  accountId: string;
  originalIntentDigest: string;
  inputAsset: AssetClass;
  outputAsset: AssetClass;
  leavingAmount: bigint;
  expectedArrivingAmount: bigint;
  feeLovelace: bigint;
  targetNonceSlot: number;
  targetNonceValue: bigint;
  operatorKeyHash: string;
  auth: SigAuthWire;
};

export type GreenOrderIntentInput = Omit<IntentPayloadInput, "originalIntentDigest" | "auth"> & {
  signerPrefix?: string;
  signerPostfix?: string;
};

export type GreenOrderSigner = {
  sign(messageHex: string): Promise<string>;
};

export type WireGreenIntent = ReturnType<typeof buildIntentPayload>;

export function buildIntentPayload(input: IntentPayloadInput) {
  assertHex("accountId", input.accountId, 64);
  assertHex("originalIntentDigest", input.originalIntentDigest, 64);
  assertHex("operatorKeyHash", input.operatorKeyHash, 56);
  assertHex("auth.signature", input.auth.signature, 128);
  assertSafeNumber("leavingAmount", input.leavingAmount);
  assertSafeNumber("expectedArrivingAmount", input.expectedArrivingAmount);
  assertSafeNumber("feeLovelace", input.feeLovelace);
  assertSafeNumber("targetNonceValue", input.targetNonceValue);
  return {
    accountId: input.accountId.toLowerCase(),
    originalIntentDigest: input.originalIntentDigest.toLowerCase(),
    inputAsset: encodeAssetClass(input.inputAsset),
    outputAsset: encodeAssetClass(input.outputAsset),
    leavingAmount: Number(input.leavingAmount),
    expectedArrivingAmount: Number(input.expectedArrivingAmount),
    feeLovelace: Number(input.feeLovelace),
    targetNonceSlot: input.targetNonceSlot,
    targetNonceValue: Number(input.targetNonceValue),
    operatorKeyHash: input.operatorKeyHash.toLowerCase(),
    auth: input.auth,
  };
}

export function alephIntentionCbor(input: {
  alephAbi?: AlephAbi;
  targetNonceSlot: number;
  targetNonceValue: bigint;
  inputAsset: AssetClass;
  leavingAmount: bigint;
  outputAsset: AssetClass;
  expectedArrivingAmount: bigint;
  feeLovelace: bigint;
  operatorKeyHash: string;
}): string {
  assertHex("operatorKeyHash", input.operatorKeyHash, 56);
  const nonce = input.alephAbi === "legacy"
    ? aikenUintCbor(input.targetNonceValue)
    : aikenTupleCbor([aikenUintCbor(BigInt(input.targetNonceSlot)), aikenUintCbor(input.targetNonceValue)]);
  return cborHex(aikenConstr0Cbor([
    nonce,
    assetClassAikenCbor(input.inputAsset),
    aikenUintCbor(input.leavingAmount),
    assetClassAikenCbor(input.outputAsset),
    aikenUintCbor(input.expectedArrivingAmount),
    aikenUintCbor(input.feeLovelace),
    aikenBytesCbor(hexToBytes(input.operatorKeyHash)),
  ]));
}

export function alephIntentionDigest(input: Parameters<typeof alephIntentionCbor>[0]): string {
  return bytesToHex(blake2b(hexToBytes(alephIntentionCbor(input)), { dkLen: 32 }));
}

export async function signGreenOrderIntent(
  input: GreenOrderIntentInput,
  signer: GreenOrderSigner,
): Promise<WireGreenIntent> {
  const originalIntentDigest = alephIntentionDigest(input);
  const signature = await signer.sign(`${input.signerPrefix ?? ""}${originalIntentDigest}${input.signerPostfix ?? ""}`);
  return buildIntentPayload({
    ...input,
    originalIntentDigest,
    auth: {
      type: "sig",
      prefix: input.signerPrefix ?? "",
      postfix: input.signerPostfix ?? "",
      signature,
      updateProof: "",
    },
  });
}

function assetClassAikenCbor(asset: AssetClass): Uint8Array {
  if (asset.kind === "ada") {
    return aikenTupleCbor([aikenBytesCbor(new Uint8Array()), aikenBytesCbor(new Uint8Array())]);
  }
  return aikenTupleCbor([
    aikenBytesCbor(hexToBytes(asset.policyId)),
    aikenBytesCbor(hexToBytes(asset.assetName)),
  ]);
}

function assertSafeNumber(name: string, value: bigint): void {
  if (value < 0n) throw new Error(`${name} must be non-negative`);
  if (value > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error(`${name} exceeds Number.MAX_SAFE_INTEGER`);
}
