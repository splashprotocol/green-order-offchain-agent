export type AssetClassWire = { kind: "ada" } | { kind: "native"; policyId: string; assetName: string };

export type SigAuthWire = {
  type: "sig";
  prefix: string;
  postfix: string;
  signature: string;
  updateProof: string;
};

export type IntentPayloadInput = {
  alephAbi?: "current" | "legacy";
  accountId: string;
  originalIntentDigest: string;
  inputAsset: AssetClassWire;
  outputAsset: AssetClassWire;
  leavingAmount: bigint;
  expectedArrivingAmount: bigint;
  feeLovelace: bigint;
  targetNonceSlot: number;
  targetNonceValue: bigint;
  operatorKeyHash: string;
  auth: SigAuthWire;
};

export function adaAsset(): AssetClassWire {
  return { kind: "ada" };
}

export function nativeAsset(policyId: string, assetName: string): AssetClassWire {
  assertHex("policyId", policyId, 56);
  assertHex("assetName", assetName);
  return { kind: "native", policyId, assetName };
}

export function encodeAssetClass(asset: AssetClassWire): string {
  return asset.kind === "ada" ? "00" : `${asset.policyId}${asset.assetName}`;
}

export function buildIntentPayload(input: IntentPayloadInput) {
  assertHex("accountId", input.accountId, 64);
  assertHex("originalIntentDigest", input.originalIntentDigest, 64);
  assertHex("operatorKeyHash", input.operatorKeyHash, 56);
  assertHex("auth.signature", input.auth.signature, 128);
  return {
    accountId: input.accountId,
    originalIntentDigest: input.originalIntentDigest,
    inputAsset: encodeAssetClass(input.inputAsset),
    outputAsset: encodeAssetClass(input.outputAsset),
    leavingAmount: Number(input.leavingAmount),
    expectedArrivingAmount: Number(input.expectedArrivingAmount),
    feeLovelace: Number(input.feeLovelace),
    targetNonceSlot: input.targetNonceSlot,
    targetNonceValue: Number(input.targetNonceValue),
    operatorKeyHash: input.operatorKeyHash,
    auth: input.auth,
  };
}

export function alephIntentionCbor(input: {
  alephAbi?: "current" | "legacy";
  targetNonceSlot: number;
  targetNonceValue: bigint;
  inputAsset: AssetClassWire;
  leavingAmount: bigint;
  outputAsset: AssetClassWire;
  expectedArrivingAmount: bigint;
  feeLovelace: bigint;
  operatorKeyHash: string;
}): string {
  assertHex("operatorKeyHash", input.operatorKeyHash, 56);
  const nonce = input.alephAbi === "legacy"
    ? aikenUintCbor(input.targetNonceValue)
    : aikenTupleCbor([aikenUintCbor(BigInt(input.targetNonceSlot)), aikenUintCbor(input.targetNonceValue)]);
  return bytesToHex(aikenConstr0Cbor([
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

function assetClassAikenCbor(asset: AssetClassWire): Uint8Array {
  if (asset.kind === "ada") {
    return aikenTupleCbor([aikenBytesCbor(new Uint8Array()), aikenBytesCbor(new Uint8Array())]);
  }
  return aikenTupleCbor([
    aikenBytesCbor(hexToBytes(asset.policyId)),
    aikenBytesCbor(hexToBytes(asset.assetName)),
  ]);
}

function aikenConstr0Cbor(fields: Uint8Array[]): Uint8Array {
  return concatBytes(new Uint8Array([0xd8, 0x79, 0x9f]), ...fields, new Uint8Array([0xff]));
}

function aikenTupleCbor(fields: Uint8Array[]): Uint8Array {
  return concatBytes(new Uint8Array([0x9f]), ...fields, new Uint8Array([0xff]));
}

function aikenUintCbor(value: bigint): Uint8Array {
  if (value < 0n) throw new Error("Aiken intention fields must be non-negative");
  if (value <= 23n) return new Uint8Array([Number(value)]);
  if (value <= 0xffn) return new Uint8Array([0x18, Number(value)]);
  if (value <= 0xffffn) return uintWithHeader(0x19, value, 2);
  if (value <= 0xffffffffn) return uintWithHeader(0x1a, value, 4);
  if (value <= 0xffffffffffffffffn) return uintWithHeader(0x1b, value, 8);
  throw new Error("Aiken intention integer exceeds uint64");
}

function aikenBytesCbor(bytes: Uint8Array): Uint8Array {
  const len = BigInt(bytes.length);
  let header: Uint8Array;
  if (len <= 23n) header = new Uint8Array([0x40 | Number(len)]);
  else if (len <= 0xffn) header = new Uint8Array([0x58, Number(len)]);
  else if (len <= 0xffffn) header = uintWithHeader(0x59, len, 2);
  else if (len <= 0xffffffffn) header = uintWithHeader(0x5a, len, 4);
  else header = uintWithHeader(0x5b, len, 8);
  return concatBytes(header, bytes);
}

function uintWithHeader(header: number, value: bigint, byteLength: number): Uint8Array {
  const out = new Uint8Array(1 + byteLength);
  out[0] = header;
  for (let i = byteLength; i > 0; i--) {
    out[i] = Number(value & 0xffn);
    value >>= 8n;
  }
  return out;
}

function concatBytes(...chunks: Uint8Array[]): Uint8Array {
  const total = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    out.set(chunk, offset);
    offset += chunk.length;
  }
  return out;
}

export function assertHex(name: string, value: string, length?: number): void {
  if (!/^[0-9a-fA-F]*$/.test(value)) throw new Error(`${name} must be hex`);
  if (length !== undefined && value.length !== length) throw new Error(`${name} must be ${length} hex chars`);
}
import { blake2b } from "npm:@noble/hashes@1.7.1/blake2b";
import { bytesToHex, hexToBytes } from "./aleph.ts";
