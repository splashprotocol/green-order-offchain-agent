export type PartialAgentConfigResult = {
  config: any;
  accountStorePath: string;
};

export type ChainSyncPoint = {
  slot: number;
  hash: string;
};

import { withRetries } from "./retry.ts";

type KoiosFetchFirst = (endpoint: string) => Promise<any>;

type ChainSyncPointOptions = {
  attempts?: number;
  delayMs?: number;
  fetchFirst?: KoiosFetchFirst;
};

export async function runPartialAgentConfigCli(args: string[] = Deno.args): Promise<void> {
  const outIndex = args.indexOf("--out");
  if (outIndex < 0 || !args[outIndex + 1]) {
    throw new Error("Usage: 09-create-partial-agent-config.ts --out <path>");
  }
  const originalPath = Deno.env.get("BASE_AGENT_CONFIG_PATH")?.trim() ??
    "../../resources/preprod.config.json";
  const original = JSON.parse(await Deno.readTextFile(originalPath));
  const lookbackSeconds = Number(Deno.env.get("PARTIAL_CHAIN_SYNC_LOOKBACK_SECONDS")?.trim() || "3600");
  const chainSyncPoint = await recentChainSyncPoint(lookbackSeconds);
  const result = await createPartialAgentConfig(original, {
    outputPath: args[outIndex + 1],
    partialDbPath: Deno.env.get("PARTIAL_AGENT_DB_PATH")?.trim() || undefined,
    chainSyncPoint,
  });
  console.log(`partial_agent_config=${args[outIndex + 1]}`);
  console.log(`greenOrders.allowPartial=${result.config.greenOrders.allowPartial}`);
  console.log(`chainSync.startingPoint=${chainSyncPoint.slot}#${chainSyncPoint.hash}`);
  console.log(`chainSync.dbPath=${result.config.chainSync.dbPath}`);
  console.log(`accountStorePath=${result.accountStorePath}`);
}

export async function createPartialAgentConfig(
  original: any,
  options: {
    repoRoot?: string;
    outputPath?: string;
    partialDbPath?: string;
    chainSyncPoint?: ChainSyncPoint;
  } = {},
): Promise<PartialAgentConfigResult> {
  const repoRoot = options.repoRoot ?? await findRepoRoot();
  const config = structuredClone(original);
  config.greenOrders = {
    ...(config.greenOrders ?? {}),
    allowPartial: true,
    autoSubmitContinuations: false,
  };
  config.chainSync = { ...(config.chainSync ?? {}) };
  config.chainSync.dbPath = absolutePartialDbPath(config.chainSync.dbPath, repoRoot, options.partialDbPath);
  const nodeSocketPath = Deno.env.get("CARDANO_NODE_SOCKET_PATH")?.trim();
  if (nodeSocketPath) {
    config.node = { ...(config.node ?? {}), path: nodeSocketPath };
  }
  const healthListenAddr = Deno.env.get("AGENT_HEALTH_LISTEN_ADDR")?.trim();
  if (healthListenAddr) {
    config.healthListenAddr = healthListenAddr;
  }
  const httpListenAddr = Deno.env.get("AGENT_HTTP_LISTEN_ADDR")?.trim();
  if (httpListenAddr) {
    config.greenOrders.intentSource = {
      ...(config.greenOrders.intentSource ?? {}),
      httpListenAddr,
    };
  }
  const agentHmacSecret = Deno.env.get("AGENT_HMAC_SECRET")?.trim();
  if (agentHmacSecret) {
    config.greenOrdersHmacAuth = {
      secret: agentHmacSecret,
      keyId: Deno.env.get("AGENT_HMAC_KEY_ID")?.trim() || undefined,
    };
  }
  if (options.chainSyncPoint) {
    const point = { Specific: [options.chainSyncPoint.slot, options.chainSyncPoint.hash] };
    config.chainSync.startingPoint = point;
    config.chainSync.replayFromPoint = point;
    config.chainSync.disableRollbacksUntil = options.chainSyncPoint.slot;
  }
  const accountStorePath = `${config.chainSync.dbPath}.green-account-stores.json`;
  if (options.outputPath) {
    await Deno.writeTextFile(options.outputPath, `${JSON.stringify(config, null, 2)}\n`);
  }
  return { config, accountStorePath };
}

export function absolutePartialDbPath(
  originalDbPath: string | undefined,
  repoRoot: string,
  override?: string,
): string {
  if (override?.trim()) return absolutePath(override.trim(), repoRoot);
  const original = originalDbPath?.trim() || "green-order-cardano-agent/e2e/preprod/.state/agent-chain-sync";
  const dir = original.replace(/\/[^/]*$/, "");
  return absolutePath(`${dir}/agent-chain-sync-partial-fill`, repoRoot);
}

function absolutePath(path: string, repoRoot: string): string {
  if (path.startsWith("/")) return path;
  return `${repoRoot.replace(/\/$/, "")}/${path}`;
}

async function findRepoRoot(): Promise<string> {
  let cwd = Deno.cwd();
  while (true) {
    try {
      const stat = await Deno.stat(`${cwd}/.git`);
      if (stat.isDirectory || stat.isFile) return cwd;
    } catch {
      // Continue upward.
    }
    const next = cwd.replace(/\/[^/]+$/, "");
    if (next === cwd) return Deno.cwd();
    cwd = next;
  }
}

export async function recentChainSyncPoint(
  lookbackSeconds: number,
  options: ChainSyncPointOptions = {},
): Promise<ChainSyncPoint> {
  if (!Number.isSafeInteger(lookbackSeconds) || lookbackSeconds < 0) {
    throw new Error(
      `PARTIAL_CHAIN_SYNC_LOOKBACK_SECONDS must be a non-negative integer, got ${lookbackSeconds}`,
    );
  }
  const fetchFirst = options.fetchFirst ?? koiosGetFirst;
  const tip = await fetchFirst("tip");
  const tipSlot = Number(tip?.abs_slot);
  if (!Number.isSafeInteger(tipSlot) || tipSlot <= 0) {
    throw new Error(`Koios tip response has invalid abs_slot: ${JSON.stringify(tip)}`);
  }
  const targetSlot = Math.max(0, tipSlot - lookbackSeconds);
  const block = await withRetries(
    () => fetchFirst(`blocks?abs_slot=lte.${targetSlot}&order=abs_slot.desc&limit=1`),
    {
      description: "Koios recent block lookup",
      attempts: options.attempts ?? 6,
      delayMs: options.delayMs ?? 5_000,
    },
  );
  const slot = Number(block?.abs_slot);
  const hash = String(block?.hash ?? "");
  if (!Number.isSafeInteger(slot) || slot <= 0 || !/^[0-9a-f]{64}$/i.test(hash)) {
    throw new Error(`Koios blocks response has invalid chain point: ${JSON.stringify(block)}`);
  }
  return { slot, hash };
}

async function koiosGetFirst(endpoint: string): Promise<any> {
  const baseUrl = Deno.env.get("KOIOS_PREPROD_URL")?.trim() ?? "https://preprod.koios.rest/api/v1";
  const response = await fetch(`${baseUrl}/${endpoint}`);
  if (!response.ok) {
    throw new Error(`Koios ${endpoint} failed ${response.status}: ${await response.text()}`);
  }
  const rows = await response.json();
  if (!Array.isArray(rows) || rows.length === 0) {
    throw new Error(`Koios ${endpoint} returned no rows`);
  }
  return rows[0];
}
