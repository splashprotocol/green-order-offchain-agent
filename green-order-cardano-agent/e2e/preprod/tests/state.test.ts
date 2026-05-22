import { assertEquals, assertThrows } from "https://deno.land/std@0.224.0/assert/mod.ts";
import { emptyState, loadState, saveState, validateState } from "../src/state.ts";

const tempStatePath = () => Deno.makeTempFileSync({ prefix: "green-preprod-state-", suffix: ".json" });

Deno.test("empty state has no pool account or entitlements", () => {
  assertEquals(emptyState(), {});
});

Deno.test("state roundtrip preserves pool account pending account and entitlements", async () => {
  const path = tempStatePath();
  const state = {
    pool: {
      outputRef: {
        txHash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        outputIndex: 0,
      },
      validator: "royaltyPool" as const,
      poolNft: "b".repeat(64),
      assetLq: "c".repeat(64),
      assetX: "00",
      assetY: "d".repeat(64),
      initialLovelace: "100000000",
      initialTokenAmount: "500000000",
      depositedLq: "9223372036854775807",
    },
    pendingAccount: {
      accountId: "1".repeat(64),
      outputRef: {
        txHash: "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        outputIndex: 1,
      },
      hotPrivateKeyHex: "2".repeat(64),
      mainKeyHex: "03" + "3".repeat(64),
      coldKeyHash: "4".repeat(56),
      initialStoreRoot: "0".repeat(64),
      batchWitnessHash: "5".repeat(56),
    },
    account: {
      accountId: "6".repeat(64),
      outputRef: {
        txHash: "7777777777777777777777777777777777777777777777777777777777777777",
        outputIndex: 2,
      },
      hotPrivateKeyHex: "8".repeat(64),
      mainKeyHex: "02" + "9".repeat(64),
      coldKeyHash: "a".repeat(56),
      initialStoreRoot: "0".repeat(64),
      batchWitnessHash: "b".repeat(56),
    },
    entitlements: [
      {
        kind: "alephBatchWitnessAllowlist" as const,
        hash: "c".repeat(56),
        rewardAddress: "stake_test1" + "q".repeat(20),
        registrationTxHash: "e".repeat(64),
        accountOutputRef: {
          txHash: "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
          outputIndex: 3,
        },
      },
    ],
    partialSmoke: {
      submittedIntentDigest: "f".repeat(64),
      executionTxHash: "1".repeat(64),
      oldStoreRootHex: "2".repeat(64),
      newStoreRootHex: "3".repeat(64),
      receivedAmount: "123",
      storePath: "/tmp/green-account-stores.json",
    },
  };

  await saveState(path, state);

  assertEquals(await loadState(path), state);
});

Deno.test("state validation rejects malformed account id", () => {
  assertThrows(
    () =>
      validateState({
        account: {
          accountId: "abc",
          outputRef: {
            txHash: "7777777777777777777777777777777777777777777777777777777777777777",
            outputIndex: 2,
          },
          hotPrivateKeyHex: "8".repeat(64),
          mainKeyHex: "02" + "9".repeat(64),
          coldKeyHash: "a".repeat(56),
          initialStoreRoot: "0".repeat(64),
          batchWitnessHash: "b".repeat(56),
        },
      }),
    Error,
    "account.accountId",
  );
});

Deno.test("state validation rejects malformed tx hash", () => {
  assertThrows(
    () =>
      validateState({
        pendingAccount: {
          accountId: "1".repeat(64),
          outputRef: { txHash: "bad", outputIndex: 0 },
          hotPrivateKeyHex: "2".repeat(64),
          mainKeyHex: "03" + "3".repeat(64),
          coldKeyHash: "4".repeat(56),
          initialStoreRoot: "0".repeat(64),
          batchWitnessHash: "5".repeat(56),
        },
      }),
    Error,
    "pendingAccount.outputRef.txHash",
  );
});

Deno.test("state validation rejects partial smoke without store path", () => {
  assertThrows(
    () =>
      validateState({
        partialSmoke: {
          submittedIntentDigest: "f".repeat(64),
          executionTxHash: "1".repeat(64),
          receivedAmount: "123",
          storePath: "",
        },
      }),
    Error,
    "partialSmoke.storePath",
  );
});
