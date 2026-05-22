export type OutputRef = { txHash: string; outputIndex: number };

export type PoolValidator = "royaltyPool" | "royaltyPoolLedgerFixed";

export type Entitlement = {
  kind: "alephBatchWitnessAllowlist";
  hash: string;
  rewardAddress?: string;
  registrationTxHash?: string;
  accountOutputRef?: OutputRef;
};

export type PreprodE2eState = {
  pool?: {
    outputRef: OutputRef;
    validator: PoolValidator;
    poolNft: string;
    assetLq: string;
    assetX: string;
    assetY: string;
    initialLovelace: string;
    initialTokenAmount: string;
    depositedLq: string;
  };
  pendingPool?: {
    validator: PoolValidator;
    poolNftNameHex: string;
    lqTokenNameHex: string;
    testTokenNameHex: string;
    poolNft?: string;
    assetLq?: string;
    assetY?: string;
  };
  account?: {
    alephAbi?: "current" | "legacy";
    accountId: string;
    outputRef: OutputRef;
    hotPrivateKeyHex: string;
    mainKeyHex: string;
    coldKeyHash: string;
    initialStoreRoot: string;
    batchWitnessHash: string;
  };
  pendingAccount?: {
    alephAbi?: "current" | "legacy";
    accountId: string;
    outputRef?: OutputRef;
    hotPrivateKeyHex: string;
    mainKeyHex: string;
    coldKeyHash: string;
    initialStoreRoot: string;
    batchWitnessHash: string;
  };
  entitlements?: Entitlement[];
  smoke?: {
    submittedIntentDigest?: string;
    executionTxHash?: string;
  };
  partialSmoke?: {
    submittedIntentDigest: string;
    executionTxHash: string;
    oldStoreRootHex?: string;
    newStoreRootHex?: string;
    receivedAmount: string;
    storePath: string;
  };
};

export function emptyState(): PreprodE2eState {
  return {};
}

export async function loadState(path: string): Promise<PreprodE2eState> {
  try {
    const parsed = JSON.parse(await Deno.readTextFile(path)) as PreprodE2eState;
    validateState(parsed);
    return parsed;
  } catch (error) {
    if (error instanceof Deno.errors.NotFound) {
      return emptyState();
    }
    throw error;
  }
}

export async function saveState(path: string, state: PreprodE2eState): Promise<void> {
  validateState(state);
  const dir = path.substring(0, path.lastIndexOf("/"));
  if (dir.length > 0) {
    await Deno.mkdir(dir, { recursive: true });
  }
  await Deno.writeTextFile(path, JSON.stringify(state, null, 2) + "\n");
}

export function validateState(state: PreprodE2eState): void {
  if (state.pool) {
    validateOutputRef("pool.outputRef", state.pool.outputRef);
    validateOneOf("pool.validator", state.pool.validator, ["royaltyPool", "royaltyPoolLedgerFixed"]);
    validateHexLikeAsset("pool.poolNft", state.pool.poolNft);
    validateHexLikeAsset("pool.assetLq", state.pool.assetLq);
    validateAsset("pool.assetX", state.pool.assetX);
    validateAsset("pool.assetY", state.pool.assetY);
    validateDecimal("pool.initialLovelace", state.pool.initialLovelace);
    validateDecimal("pool.initialTokenAmount", state.pool.initialTokenAmount);
    validateDecimal("pool.depositedLq", state.pool.depositedLq);
  }
  if (state.pendingPool) {
    validateOneOf("pendingPool.validator", state.pendingPool.validator, [
      "royaltyPool",
      "royaltyPoolLedgerFixed",
    ]);
    validateHex("pendingPool.poolNftNameHex", state.pendingPool.poolNftNameHex);
    validateHex("pendingPool.lqTokenNameHex", state.pendingPool.lqTokenNameHex);
    validateHex("pendingPool.testTokenNameHex", state.pendingPool.testTokenNameHex);
    if (state.pendingPool.poolNft) validateHexLikeAsset("pendingPool.poolNft", state.pendingPool.poolNft);
    if (state.pendingPool.assetLq) validateHexLikeAsset("pendingPool.assetLq", state.pendingPool.assetLq);
    if (state.pendingPool.assetY) validateHexLikeAsset("pendingPool.assetY", state.pendingPool.assetY);
  }
  if (state.account) {
    validateAccountLike("account", state.account);
    validateOutputRef("account.outputRef", state.account.outputRef);
  }
  if (state.pendingAccount) {
    validateAccountLike("pendingAccount", state.pendingAccount);
    if (state.pendingAccount.outputRef) {
      validateOutputRef("pendingAccount.outputRef", state.pendingAccount.outputRef);
    }
  }
  if (state.entitlements) {
    state.entitlements.forEach((entitlement, index) => {
      validateOneOf(`entitlements[${index}].kind`, entitlement.kind, ["alephBatchWitnessAllowlist"]);
      validateHex(`entitlements[${index}].hash`, entitlement.hash, 56);
      if (entitlement.rewardAddress && !entitlement.rewardAddress.startsWith("stake_test1")) {
        throw new Error(`entitlements[${index}].rewardAddress must be a preprod reward address`);
      }
      if (entitlement.registrationTxHash) {
        validateHex(`entitlements[${index}].registrationTxHash`, entitlement.registrationTxHash, 64);
      }
      if (entitlement.accountOutputRef) {
        validateOutputRef(`entitlements[${index}].accountOutputRef`, entitlement.accountOutputRef);
      }
    });
  }
  if (state.partialSmoke) {
    validateHex("partialSmoke.submittedIntentDigest", state.partialSmoke.submittedIntentDigest, 64);
    validateHex("partialSmoke.executionTxHash", state.partialSmoke.executionTxHash, 64);
    if (state.partialSmoke.oldStoreRootHex) {
      validateHex("partialSmoke.oldStoreRootHex", state.partialSmoke.oldStoreRootHex, 64);
    }
    if (state.partialSmoke.newStoreRootHex) {
      validateHex("partialSmoke.newStoreRootHex", state.partialSmoke.newStoreRootHex, 64);
    }
    validateDecimal("partialSmoke.receivedAmount", state.partialSmoke.receivedAmount);
    if (!state.partialSmoke.storePath.trim()) {
      throw new Error("partialSmoke.storePath must be a non-empty string");
    }
  }
}

function validateAccountLike(
  prefix: string,
  account: {
    alephAbi?: "current" | "legacy";
    accountId: string;
    hotPrivateKeyHex: string;
    mainKeyHex: string;
    coldKeyHash: string;
    initialStoreRoot: string;
    batchWitnessHash: string;
  },
) {
  if (account.alephAbi) validateOneOf(`${prefix}.alephAbi`, account.alephAbi, ["current", "legacy"]);
  validateHex(`${prefix}.accountId`, account.accountId, 64);
  validateHex(`${prefix}.hotPrivateKeyHex`, account.hotPrivateKeyHex, 64);
  validateHex(`${prefix}.mainKeyHex`, account.mainKeyHex, 66);
  if (!account.mainKeyHex.startsWith("02") && !account.mainKeyHex.startsWith("03")) {
    throw new Error(`${prefix}.mainKeyHex must be a compressed secp256k1 public key`);
  }
  validateHex(`${prefix}.coldKeyHash`, account.coldKeyHash, 56);
  validateHex(`${prefix}.initialStoreRoot`, account.initialStoreRoot, 64);
  validateHex(`${prefix}.batchWitnessHash`, account.batchWitnessHash, 56);
}

export function validateOutputRef(prefix: string, ref: OutputRef): void {
  validateHex(`${prefix}.txHash`, ref.txHash, 64);
  if (!Number.isInteger(ref.outputIndex) || ref.outputIndex < 0) {
    throw new Error(`${prefix}.outputIndex must be a non-negative integer`);
  }
}

export function validateHex(prefix: string, value: string, length?: number): void {
  if (!/^[0-9a-fA-F]*$/.test(value)) {
    throw new Error(`${prefix} must be hex`);
  }
  if (length !== undefined && value.length !== length) {
    throw new Error(`${prefix} must be ${length} hex chars`);
  }
}

function validateHexLikeAsset(prefix: string, value: string): void {
  validateHex(prefix, value);
  if (value.length < 56 || value.length % 2 !== 0) {
    throw new Error(`${prefix} must be policy id plus optional asset name hex`);
  }
}

function validateAsset(prefix: string, value: string): void {
  if (value === "00") return;
  validateHexLikeAsset(prefix, value);
}

function validateDecimal(prefix: string, value: string): void {
  if (!/^[0-9]+$/.test(value)) {
    throw new Error(`${prefix} must be a decimal string`);
  }
}

function validateOneOf<T extends string>(
  prefix: string,
  value: string,
  options: readonly T[],
): asserts value is T {
  if (!options.includes(value as T)) {
    throw new Error(`${prefix} must be one of ${options.join(", ")}`);
  }
}

export function isZeroTxHash(txHash: string): boolean {
  return /^0{64}$/i.test(txHash);
}

export function isPlaceholderTxHash(txHash: string): boolean {
  return /^0{63}[0-9a-f]$/i.test(txHash);
}
