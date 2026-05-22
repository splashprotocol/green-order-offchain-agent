import { CML } from "npm:@lucid-evolution/lucid@0.3.53";
import {
  isPlaceholderTxHash,
  isZeroTxHash,
  OutputRef,
  PoolValidator,
  validateHex,
  validateOutputRef,
} from "./state.ts";

export type DeployedValidator = {
  hash: string;
  referenceUtxo?: OutputRef;
  cost?: { mem: number; steps: number };
  marginalCost?: { mem: number; steps: number };
};

export type Deployment = Record<string, DeployedValidator>;

export type PreprodE2eConfig = {
  fundedWalletSeed: string;
  agentUrl: string;
  agentHealthUrl: string;
  deploymentPath: string;
  agentConfigPath: string;
  statePath: string;
  operatorKeyHashHex: string;
  accountHotPrivateKeyHex?: string;
  poolInitialLovelace: bigint;
  poolInitialTokenAmount: bigint;
  accountInitialLovelace: bigint;
  smokeLeavingLovelace: bigint;
  smokeExpectedTokenAmount: bigint;
  smokeFeeLovelace: bigint;
  partialLeavingLovelace: bigint;
  partialExpectedTokenAmount: bigint;
  partialFeeLovelace: bigint;
  partialExecutionTimeoutMs: number;
  deployment: Deployment;
};

export async function loadConfig(): Promise<PreprodE2eConfig> {
  await loadDotEnv(".env");
  await loadDotEnv(".env.operator");
  await loadDotEnv(".env.wallets");
  const deploymentPath = env("DEPLOYMENT_PATH", "../../resources/preprod.deployment.json");
  const deployment = JSON.parse(await Deno.readTextFile(deploymentPath)) as Deployment;
  const agentConfigPath = env("AGENT_CONFIG_PATH", "../../resources/preprod.config.json");
  const operatorKeyHashHex = await operatorKeyHashFromAgentConfig(agentConfigPath);
  const envOperatorKeyHashHex = optionalEnv("OPERATOR_KEY_HASH_HEX");
  if (envOperatorKeyHashHex && envOperatorKeyHashHex !== operatorKeyHashHex) {
    console.warn(
      `Ignoring OPERATOR_KEY_HASH_HEX=${envOperatorKeyHashHex}; using green-order agent batcher ${operatorKeyHashHex}`,
    );
  }

  validateDeploymentForGreenOrder(deployment, selectedPoolValidator(deployment));

  const accountHotPrivateKeyHex = optionalEnv("ACCOUNT_HOT_PRIVATE_KEY_HEX");
  if (accountHotPrivateKeyHex) {
    validateHex("ACCOUNT_HOT_PRIVATE_KEY_HEX", accountHotPrivateKeyHex, 64);
  }

  return {
    fundedWalletSeed: requiredEnv("FUNDED_WALLET_SEED"),
    agentUrl: env("AGENT_URL", "http://127.0.0.1:9031"),
    agentHealthUrl: env("AGENT_HEALTH_URL", "http://127.0.0.1:9024/health"),
    deploymentPath,
    agentConfigPath,
    statePath: env("STATE_PATH", ".state/preprod-green-order-e2e.json"),
    operatorKeyHashHex,
    accountHotPrivateKeyHex,
    poolInitialLovelace: bigintEnv("POOL_INITIAL_LOVELACE", 40_000_000n),
    poolInitialTokenAmount: bigintEnv("POOL_INITIAL_TOKEN_AMOUNT", 40_000_000n),
    accountInitialLovelace: bigintEnv("ACCOUNT_INITIAL_LOVELACE", 25_000_000n),
    smokeLeavingLovelace: bigintEnv("SMOKE_LEAVING_LOVELACE", 1_000_000n),
    smokeExpectedTokenAmount: bigintEnv("SMOKE_EXPECTED_TOKEN_AMOUNT", 1n),
    smokeFeeLovelace: bigintEnv("SMOKE_FEE_LOVELACE", 2_000_000n),
    partialLeavingLovelace: bigintEnv("PARTIAL_LEAVING_LOVELACE", 20_000_000n),
    partialExpectedTokenAmount: bigintEnv("PARTIAL_EXPECTED_TOKEN_AMOUNT", 15_000_000n),
    partialFeeLovelace: bigintEnv("PARTIAL_FEE_LOVELACE", 2_000_000n),
    partialExecutionTimeoutMs: numberEnv("PARTIAL_EXECUTION_TIMEOUT_MS", 900_000),
    deployment,
  };
}

export function operatorKeyHashFromBech32(operatorKey: string): string {
  return CML.Bip32PrivateKey.from_bech32(operatorKey)
    .to_public()
    .to_raw_key()
    .hash()
    .to_hex();
}

async function operatorKeyHashFromAgentConfig(agentConfigPath: string): Promise<string> {
  const config = JSON.parse(await Deno.readTextFile(agentConfigPath)) as { operatorKey?: string };
  if (!config.operatorKey) {
    throw new Error(`Set operatorKey in ${agentConfigPath}`);
  }
  const hash = operatorKeyHashFromBech32(config.operatorKey);
  validateHex(`${agentConfigPath}.operatorKey hash`, hash, 56);
  return hash;
}

async function loadDotEnv(path: string): Promise<void> {
  try {
    const text = await Deno.readTextFile(path);
    for (const line of text.split(/\r?\n/)) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith("#") || !trimmed.includes("=")) continue;
      const [name, ...rest] = trimmed.split("=");
      if (!Deno.env.get(name)) {
        Deno.env.set(name, rest.join("="));
      }
    }
  } catch (error) {
    if (!(error instanceof Deno.errors.NotFound)) throw error;
  }
}

export function validateDeploymentForGreenOrder(
  deployment: Deployment,
  poolValidator: PoolValidator,
): void {
  for (const key of ["alephAccount", "alephBatchWitness", poolValidator]) {
    const validator = deployment[key];
    if (!validator) {
      throw new Error(`deployment is missing ${key}`);
    }
    validateHex(`deployment.${key}.hash`, validator.hash, 56);
    if (!validator.referenceUtxo) {
      throw new Error(`deployment.${key}.referenceUtxo is missing`);
    }
    validateOutputRef(`deployment.${key}.referenceUtxo`, validator.referenceUtxo);
    if (isZeroTxHash(validator.referenceUtxo.txHash) || isPlaceholderTxHash(validator.referenceUtxo.txHash)) {
      throw new Error(`deployment.${key}.referenceUtxo is a placeholder tx hash`);
    }
  }
}

export function selectedPoolValidator(deployment: Deployment): PoolValidator {
  if (
    deployment.royaltyPoolLedgerFixed?.referenceUtxo &&
    !isZeroTxHash(deployment.royaltyPoolLedgerFixed.referenceUtxo.txHash)
  ) {
    return "royaltyPoolLedgerFixed";
  }
  if (deployment.royaltyPool?.referenceUtxo && !isZeroTxHash(deployment.royaltyPool.referenceUtxo.txHash)) {
    return "royaltyPool";
  }
  return "royaltyPool";
}

function requiredEnv(name: string): string {
  const value = optionalEnv(name);
  if (!value) throw new Error(`Set ${name}`);
  return value;
}

function optionalEnv(name: string): string | undefined {
  const value = Deno.env.get(name)?.trim();
  return value ? value : undefined;
}

function env(name: string, fallback: string): string {
  return optionalEnv(name) ?? fallback;
}

function bigintEnv(name: string, fallback: bigint): bigint {
  const value = optionalEnv(name);
  if (!value) return fallback;
  if (!/^[0-9]+$/.test(value)) throw new Error(`${name} must be a decimal integer`);
  return BigInt(value);
}

function numberEnv(name: string, fallback: number): number {
  const value = optionalEnv(name);
  if (!value) return fallback;
  if (!/^[0-9]+$/.test(value)) throw new Error(`${name} must be a decimal integer`);
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed)) throw new Error(`${name} must be a safe integer`);
  return parsed;
}
