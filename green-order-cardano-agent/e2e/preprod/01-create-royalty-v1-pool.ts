import { Data } from "npm:@lucid-evolution/lucid@0.3.53";
import {
  credentialToAddress,
  mintingPolicyToId,
  paymentCredentialOf,
  scriptFromNative,
} from "npm:@lucid-evolution/utils@0.1.65";
import { loadConfig, selectedPoolValidator } from "./src/config.ts";
import { getLucid, submitSignedTx } from "./src/lucid.ts";
import {
  LQ_TOKEN_NAME_PREFIX_HEX,
  planRoyaltyPool,
  POOL_NFT_NAME_PREFIX_HEX,
  TEST_TOKEN_NAME_PREFIX_HEX,
  validateRoyaltyPoolPlan,
} from "./src/royalty_pool_v1.ts";
import { loadState, PoolValidator, PreprodE2eState, saveState } from "./src/state.ts";
import { withRetries } from "./src/retry.ts";
import { waitFor } from "./src/wait.ts";

const dryRun = Deno.args.includes("--dry-run");
const force = Deno.args.includes("--force");
const minPoolTxHash = Deno.env.get("POOL_MIN_TX_HASH")?.trim();

const config = await loadConfig();
let state = await loadState(config.statePath);
if (state.pool && !force) {
  console.log(
    `state.pool already exists at ${state.pool.outputRef.txHash}#${state.pool.outputRef.outputIndex}; nothing to create`,
  );
  Deno.exit(0);
}
if (force) {
  delete state.pool;
  delete state.pendingPool;
  delete state.smoke;
}

const validator = selectedPoolValidator(config.deployment);
console.log(`Royalty pool validator: ${validator}`);
console.log(`Royalty pool script hash: ${config.deployment[validator].hash}`);
console.log(`Royalty pool reference UTxO: ${JSON.stringify(config.deployment[validator].referenceUtxo)}`);

const lucid = await getLucid(config);
const walletAddress = await lucid.wallet().address();
const paymentCred = paymentCredentialOf(walletAddress);
if (paymentCred.type !== "Key") {
  throw new Error("funded wallet must use a payment key credential");
}
const nativePolicy = scriptFromNative({ type: "sig", keyHash: paymentCred.hash });
const tokenPolicyId = mintingPolicyToId(nativePolicy);
const lqPolicyId = tokenPolicyId;
let pendingPool = state.pendingPool ?? newPendingPool(validator);
let plan = planPendingPool(pendingPool);
validateRoyaltyPoolPlan(plan);
pendingPool = { ...pendingPool, poolNft: plan.poolNft, assetLq: plan.assetLq, assetY: plan.assetY };
state = { ...state, pendingPool };
if (!dryRun) await saveState(config.statePath, state);

console.log(`Pool asset X: ${plan.assetX}`);
console.log(`Pool asset Y: ${plan.assetY}`);
console.log(`Pool NFT: ${plan.poolNft}`);
console.log(`Pool LQ: ${plan.assetLq}`);
console.log(`Initial lovelace: ${plan.initialLovelace}`);
console.log(`Initial token amount: ${plan.initialTokenAmount}`);
console.log(`Deposited LQ: ${plan.depositedLq}`);
console.log(`Royalty datum CBOR: ${plan.datum}`);

if (dryRun) Deno.exit(0);

const poolAddress = credentialToAddress("Preprod", {
  type: "Script",
  hash: config.deployment[validator].hash,
});
const existingPool = (await withRetries(() => lucid.utxosAt(poolAddress), {
  description: "load Royalty V1 pool UTxOs",
})).find((utxo) => utxo.assets[plan.poolNft] === 1n);
if (existingPool) {
  if (!force) {
    throw new Error(
      `pool NFT ${plan.poolNft} already exists at ${existingPool.txHash}#${existingPool.outputIndex}`,
    );
  }
  await saveState(config.statePath, {
    ...state,
    pool: {
      outputRef: { txHash: existingPool.txHash, outputIndex: existingPool.outputIndex },
      validator: plan.validator,
      poolNft: plan.poolNft,
      assetLq: plan.assetLq,
      assetX: plan.assetX,
      assetY: plan.assetY,
      initialLovelace: plan.initialLovelace.toString(),
      initialTokenAmount: plan.initialTokenAmount.toString(),
      depositedLq: plan.depositedLq.toString(),
    },
  });
  console.log(`Resumed existing Royalty V1 pool at ${existingPool.txHash}#${existingPool.outputIndex}`);
  Deno.exit(0);
}

const signed = await signedPoolTx();
const txHash = await submitSignedTx(signed);
const outputRef = await waitFor("created Royalty V1 pool output", async () => {
  const utxos = await withRetries(() => lucid.utxosAt(poolAddress), {
    description: "poll created Royalty V1 pool output",
  });
  const found = utxos.find((utxo) => utxo.txHash === txHash && utxo.assets[plan.poolNft] === 1n);
  return found ? { txHash: found.txHash, outputIndex: found.outputIndex } : undefined;
});

const finalState = {
  ...state,
  pool: {
    outputRef,
    validator: plan.validator,
    poolNft: plan.poolNft,
    assetLq: plan.assetLq,
    assetX: plan.assetX,
    assetY: plan.assetY,
    initialLovelace: plan.initialLovelace.toString(),
    initialTokenAmount: plan.initialTokenAmount.toString(),
    depositedLq: plan.depositedLq.toString(),
  },
};
delete finalState.pendingPool;
await saveState(config.statePath, finalState);
console.log(`Created Royalty V1 pool at ${outputRef.txHash}#${outputRef.outputIndex}`);

type PendingPool = NonNullable<PreprodE2eState["pendingPool"]>;

function newPendingPool(validatorName: PoolValidator): PendingPool {
  const suffix = randomHex(8);
  return {
    validator: validatorName,
    poolNftNameHex: `${POOL_NFT_NAME_PREFIX_HEX}${suffix}`,
    lqTokenNameHex: `${LQ_TOKEN_NAME_PREFIX_HEX}${suffix}`,
    testTokenNameHex: `${TEST_TOKEN_NAME_PREFIX_HEX}${suffix}`,
  };
}

function planPendingPool(pool: PendingPool) {
  return planRoyaltyPool(config.deployment, tokenPolicyId, lqPolicyId, {
    initialLovelace: config.poolInitialLovelace,
    initialTokenAmount: config.poolInitialTokenAmount,
    poolNftNameHex: pool.poolNftNameHex,
    lqTokenNameHex: pool.lqTokenNameHex,
    testTokenNameHex: pool.testTokenNameHex,
    adminScriptHash: config.deployment[validator].hash,
    treasuryScriptHash: config.deployment[validator].hash,
    royaltyPubKeyHex: paymentCred.hash.padEnd(64, "0").slice(0, 64),
  });
}

async function signedPoolTx() {
  const maxCandidates = Number(Deno.env.get("POOL_TX_HASH_MAX_CANDIDATES") ?? "512");
  for (let attempt = 0; attempt < maxCandidates; attempt++) {
    const candidate = attempt === 0 ? pendingPool : newPendingPool(validator);
    const candidatePlan = planPendingPool(candidate);
    validateRoyaltyPoolPlan(candidatePlan);
    const candidateAssets: Record<string, bigint> = {
      lovelace: candidatePlan.initialLovelace,
      [candidatePlan.assetY]: candidatePlan.initialTokenAmount,
      [candidatePlan.poolNft]: 1n,
      [candidatePlan.assetLq]: candidatePlan.depositedLq,
    };
    const candidateSigned = await withRetries(async () => {
      const tx = await lucid
        .newTx()
        .attach.Script(nativePolicy)
        .mintAssets({
          [candidatePlan.assetY]: candidatePlan.initialTokenAmount,
          [candidatePlan.poolNft]: 1n,
          [candidatePlan.assetLq]: candidatePlan.depositedLq,
        }, Data.to(0n))
        .pay.ToAddressWithData(poolAddress, { kind: "inline", value: candidatePlan.datum }, candidateAssets)
        .addSignerKey(paymentCred.hash)
        .complete();
      return await tx.sign.withWallet().complete();
    }, {
      description: `build and sign Royalty V1 pool candidate ${attempt + 1}`,
    });
    const candidateTxHash = candidateSigned.toHash();
    if (!minPoolTxHash || minPoolTxHash < candidateTxHash) {
      if (attempt > 0 || minPoolTxHash) {
        console.log(`selected_pool_tx_hash=${candidateTxHash}`);
        console.log(`searched_pool_candidates=${attempt + 1}`);
      }
      pendingPool = {
        ...candidate,
        poolNft: candidatePlan.poolNft,
        assetLq: candidatePlan.assetLq,
        assetY: candidatePlan.assetY,
      };
      plan = candidatePlan;
      state = { ...state, pendingPool };
      await saveState(config.statePath, state);
      return candidateSigned;
    }
    if (attempt % 32 === 31) {
      console.log(`searched_pool_candidates=${attempt + 1}`);
    }
  }
  throw new Error(`No pool tx hash found after ${maxCandidates} candidates above ${minPoolTxHash}`);
}

function randomHex(bytes: number): string {
  const data = new Uint8Array(bytes);
  crypto.getRandomValues(data);
  return Array.from(data, (byte) => byte.toString(16).padStart(2, "0")).join("");
}
