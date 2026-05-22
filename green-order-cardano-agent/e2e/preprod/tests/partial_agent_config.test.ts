import { assert, assertEquals, assertStringIncludes } from "https://deno.land/std@0.224.0/assert/mod.ts";
import { createPartialAgentConfig } from "../src/partial_agent_config.ts";

Deno.test("createPartialAgentConfig enables partial and writes absolute db path", async () => {
  const original = {
    chainSync: {
      dbPath: "green-order-cardano-agent/e2e/preprod/.state/agent-chain-sync-funding",
      startingPoint: { Specific: [1, "a".repeat(64)] },
      replayFromPoint: { Specific: [1, "a".repeat(64)] },
    },
    greenOrders: { allowPartial: false, intentSource: { httpListenAddr: "127.0.0.1:9031" } },
    operatorKey: "operator",
    node: { path: "socket", magic: 1 },
    networkId: 0,
  };
  const out = await createPartialAgentConfig(original, {
    repoRoot: "/repo",
    outputPath: "/tmp/partial.json",
    chainSyncPoint: { slot: 123, hash: "b".repeat(64) },
  });

  assertEquals(out.config.greenOrders.allowPartial, true);
  assert(out.config.chainSync.dbPath.startsWith("/repo/"));
  assertStringIncludes(out.config.chainSync.dbPath, "green-order-cardano-agent/e2e/preprod/.state/");
  assertEquals(out.accountStorePath, `${out.config.chainSync.dbPath}.green-account-stores.json`);
  assertEquals(out.config.chainSync.startingPoint, { Specific: [123, "b".repeat(64)] });
  assertEquals(out.config.chainSync.replayFromPoint, { Specific: [123, "b".repeat(64)] });
  assertEquals(out.config.chainSync.disableRollbacksUntil, 123);
  assertEquals(out.config.operatorKey, "operator");
});
