import { Entitlement, OutputRef, PreprodE2eState, validateHex } from "./state.ts";
import { credentialToRewardAddress } from "npm:@lucid-evolution/lucid@0.3.53";
import { secp256k1 } from "npm:@noble/curves@1.8.1/secp256k1";

export const ALEPH_ACCOUNT_MAGIC_HEX = "01";
export const ALEPH_EMPTY_MPF_ROOT_HEX = "00".repeat(32);

export function createEntitlementState(batchWitnessHash: string): Entitlement[] {
  validateHex("alephBatchWitness.hash", batchWitnessHash, 56);
  return [{
    kind: "alephBatchWitnessAllowlist",
    hash: batchWitnessHash,
    rewardAddress: batchWitnessRewardAddress(batchWitnessHash),
  }];
}

export function batchWitnessRewardAddress(batchWitnessHash: string): string {
  validateHex("alephBatchWitness.hash", batchWitnessHash, 56);
  return credentialToRewardAddress("Preprod", { type: "Script", hash: batchWitnessHash });
}

export function requireBatchWitnessEntitlement(
  state: PreprodE2eState,
  batchWitnessHash: string,
): Entitlement {
  const entitlement = state.entitlements?.find((item) => item.kind === "alephBatchWitnessAllowlist");
  if (!entitlement) throw new Error("Run 02-create-entitlements.ts before creating the Aleph account");
  if (entitlement.hash !== batchWitnessHash) {
    throw new Error("Stored alephBatchWitness entitlement does not match deployment");
  }
  return entitlement;
}

export function attachAccountOutputRefToEntitlement(
  state: PreprodE2eState,
  outputRef: OutputRef,
): PreprodE2eState {
  return {
    ...state,
    entitlements: state.entitlements?.map((item) =>
      item.kind === "alephBatchWitnessAllowlist" ? { ...item, accountOutputRef: outputRef } : item
    ),
  };
}

export function deriveCompressedPublicKey(privateKeyHex: string): string {
  validateHex("ACCOUNT_HOT_PRIVATE_KEY_HEX", privateKeyHex, 64);
  return bytesToHex(secp256k1.getPublicKey(hexToBytes(privateKeyHex), true));
}

export function accountIdFromMainKey(mainKeyHex: string): string {
  validateHex("mainKeyHex", mainKeyHex, 66);
  return bytesToHex(secp256k1.ProjectivePoint.fromHex(mainKeyHex).toRawBytes(false).slice(1, 33));
}

export function randomHotPrivateKeyHex(): string {
  return bytesToHex(secp256k1.utils.randomPrivateKey());
}

export function signIntentDigest(privateKeyHex: string, digestHex: string): string {
  validateHex("privateKeyHex", privateKeyHex, 64);
  validateHex("digestHex", digestHex, 64);
  const signature = secp256k1.sign(hexToBytes(digestHex), hexToBytes(privateKeyHex), {
    lowS: true,
    prehash: false,
  });
  return bytesToHex(signature.toCompactRawBytes());
}

export function hexToBytes(hex: string): Uint8Array {
  if (hex.length % 2 !== 0) throw new Error("hex must have even length");
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return bytes;
}

export function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}
