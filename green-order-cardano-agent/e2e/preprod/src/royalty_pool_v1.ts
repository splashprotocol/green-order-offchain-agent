import { Constr, Data } from "npm:@lucid-evolution/lucid@0.3.53";
import { Deployment, selectedPoolValidator } from "./config.ts";
import { PoolValidator } from "./state.ts";

export const ADA_WIRE = "00";
export const POOL_NFT_NAME_PREFIX_HEX = "6e6674";
export const LQ_TOKEN_NAME_PREFIX_HEX = "6c71";
export const TEST_TOKEN_NAME_PREFIX_HEX = "677265656e";
export const MAX_LQ_CAP = 1_000_000_000n;

export type RoyaltyPoolPlan = {
  validator: PoolValidator;
  poolNft: string;
  assetLq: string;
  assetX: string;
  assetY: string;
  initialLovelace: bigint;
  initialTokenAmount: bigint;
  depositedLq: bigint;
  datum: string;
};

export function planRoyaltyPool(
  deployment: Deployment,
  tokenPolicyId: string,
  lqPolicyId: string,
  options: {
    initialLovelace: bigint;
    initialTokenAmount: bigint;
    poolNftNameHex: string;
    lqTokenNameHex: string;
    testTokenNameHex: string;
    adminScriptHash: string;
    treasuryScriptHash: string;
    royaltyPubKeyHex: string;
  },
): RoyaltyPoolPlan {
  const validator = selectedPoolValidator(deployment);
  const poolNft = `${tokenPolicyId}${options.poolNftNameHex}`;
  const assetLq = `${lqPolicyId}${options.lqTokenNameHex}`;
  const assetY = `${tokenPolicyId}${options.testTokenNameHex}`;
  const depositedLq = MAX_LQ_CAP;
  const datum = buildRoyaltyPoolDatum({
    poolNft,
    assetX: ADA_WIRE,
    assetY,
    assetLq,
    lpFeeNum: 99_700n,
    treasuryFeeNum: 0n,
    royaltyFeeNum: 0n,
    treasuryX: 0n,
    treasuryY: 0n,
    royaltyX: 0n,
    royaltyY: 0n,
    adminScriptHash: options.adminScriptHash,
    treasuryScriptHash: options.treasuryScriptHash,
    royaltyPubKeyHex: options.royaltyPubKeyHex,
    nonce: 0n,
  });
  return {
    validator,
    poolNft,
    assetLq,
    assetX: ADA_WIRE,
    assetY,
    initialLovelace: options.initialLovelace,
    initialTokenAmount: options.initialTokenAmount,
    depositedLq,
    datum,
  };
}

export function validateRoyaltyPoolPlan(plan: RoyaltyPoolPlan): void {
  if (plan.assetX !== ADA_WIRE) throw new Error("first smoke Royalty V1 pool must use ADA as assetX");
  for (
    const [name, value] of [
      ["poolNft", plan.poolNft],
      ["assetLq", plan.assetLq],
      ["assetY", plan.assetY],
    ] as const
  ) {
    if (!/^[0-9a-f]+$/i.test(value) || value.length < 58 || value.length % 2 !== 0) {
      throw new Error(`${name} must be policy id plus asset name hex`);
    }
  }
  if (plan.initialLovelace <= 0n || plan.initialTokenAmount <= 0n || plan.depositedLq <= 0n) {
    throw new Error("Royalty V1 pool reserves and deposited LQ must be positive");
  }
  if (!plan.datum.startsWith("d879")) {
    throw new Error("Royalty V1 datum must be constructor-0 PlutusData CBOR");
  }
  const parsed = Data.from(plan.datum) as any;
  if (!(parsed instanceof Constr) || parsed.index !== 0 || parsed.fields.length !== 15) {
    throw new Error("Royalty V1 datum must be constructor 0 with 15 fields");
  }
  for (const [index, name] of [[0, "poolNft"], [1, "assetX"], [2, "assetY"], [3, "assetLq"]] as const) {
    const asset = parsed.fields[index];
    if (!(asset instanceof Constr) || asset.index !== 0 || asset.fields.length !== 2) {
      throw new Error(`Royalty V1 datum ${name} must be an AssetClass constructor`);
    }
  }
  const admins = parsed.fields[11];
  const admin = Array.isArray(admins) ? admins[0] : undefined;
  const credential = admin instanceof Constr ? admin.fields[0] : undefined;
  if (
    !(admin instanceof Constr) || admin.index !== 0 || !(credential instanceof Constr) ||
    credential.index !== 1
  ) {
    throw new Error("Royalty V1 datum admin_address must be inline script credential");
  }
  for (
    const [index, name] of [[4, "lp_fee_num"], [5, "treasury_fee_num"], [6, "royalty_fee_num"], [
      14,
      "nonce",
    ]] as const
  ) {
    if (typeof parsed.fields[index] !== "bigint") {
      throw new Error(`Royalty V1 datum ${name} must be integer`);
    }
  }
}

export function buildRoyaltyPoolDatum(conf: {
  poolNft: string;
  assetX: string;
  assetY: string;
  assetLq: string;
  lpFeeNum: bigint;
  treasuryFeeNum: bigint;
  royaltyFeeNum: bigint;
  treasuryX: bigint;
  treasuryY: bigint;
  royaltyX: bigint;
  royaltyY: bigint;
  adminScriptHash: string;
  treasuryScriptHash: string;
  royaltyPubKeyHex: string;
  nonce: bigint;
}): string {
  return Data.to(
    new Constr(0, [
      assetClassData(conf.poolNft),
      assetClassData(conf.assetX),
      assetClassData(conf.assetY),
      assetClassData(conf.assetLq),
      conf.lpFeeNum,
      conf.treasuryFeeNum,
      conf.royaltyFeeNum,
      conf.treasuryX,
      conf.treasuryY,
      conf.royaltyX,
      conf.royaltyY,
      [new Constr(0, [new Constr(1, [conf.adminScriptHash])])],
      conf.treasuryScriptHash,
      conf.royaltyPubKeyHex,
      conf.nonce,
    ] as any) as any,
  );
}

export function assetClassData(asset: string): unknown {
  if (asset === ADA_WIRE) {
    return new Constr(0, ["", ""]);
  }
  return new Constr(0, [asset.slice(0, 56), asset.slice(56)]);
}
