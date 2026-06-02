import { assertHex } from "./hex.js";

export type AssetClass = AdaAsset | NativeAsset;

export type AdaAsset = {
  kind: "ada";
  wireId: "00";
};

export type NativeAsset = {
  kind: "native";
  policyId: string;
  assetName: string;
  wireId: string;
};

export function adaAsset(): AdaAsset {
  return { kind: "ada", wireId: "00" };
}

export function nativeAsset(policyId: string, assetName: string): NativeAsset {
  assertHex("policyId", policyId, 56);
  assertHex("assetName", assetName);
  return {
    kind: "native",
    policyId: policyId.toLowerCase(),
    assetName: assetName.toLowerCase(),
    wireId: `${policyId}${assetName}`.toLowerCase(),
  };
}

export function encodeAssetClass(asset: AssetClass): string {
  return asset.wireId;
}
