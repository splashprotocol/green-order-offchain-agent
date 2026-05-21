import { assertEquals, assertThrows } from "https://deno.land/std@0.224.0/assert/mod.ts";
import {
  buildBindAccountPayload,
  parseAgentAcceptedResponse,
  parseAgentBoundResponse,
} from "../src/agent.ts";
import {
  adaAsset,
  alephIntentionCbor,
  alephIntentionDigest,
  buildIntentPayload,
  nativeAsset,
} from "../src/intent.ts";
import { batchWitnessRewardAddress, createEntitlementState } from "../src/aleph.ts";
import { operatorKeyHashFromBech32 } from "../src/config.ts";

Deno.test("bind account payload matches HTTP endpoint wire format", () => {
  assertEquals(
    buildBindAccountPayload("a".repeat(64), {
      txHash: "b".repeat(64),
      outputIndex: 7,
    }),
    {
      accountId: "a".repeat(64),
      txHash: "b".repeat(64),
      outputIndex: 7,
    },
  );
});

Deno.test("intent payload encodes ADA as 00 and native assets as policy plus name", () => {
  const payload = buildIntentPayload({
    accountId: "1".repeat(64),
    originalIntentDigest: "2".repeat(64),
    inputAsset: adaAsset(),
    outputAsset: nativeAsset("3".repeat(56), "74657374"),
    leavingAmount: 1_000_000n,
    expectedArrivingAmount: 900_000n,
    feeLovelace: 2_000_000n,
    targetNonceSlot: 0,
    targetNonceValue: 1n,
    operatorKeyHash: "4".repeat(56),
    auth: {
      type: "sig",
      prefix: "",
      postfix: "",
      signature: "5".repeat(128),
      updateProof: "",
    },
  });

  assertEquals(payload.inputAsset, "00");
  assertEquals(payload.outputAsset, "3".repeat(56) + "74657374");
  assertEquals(payload.leavingAmount, 1_000_000);
});

Deno.test("agent response parsers accept success bodies", () => {
  parseAgentAcceptedResponse({ status: "accepted", reason: null });
  parseAgentBoundResponse({ status: "bound", reason: null });
});

Deno.test("agent response parsers reject wrong success kind", () => {
  assertThrows(
    () => parseAgentAcceptedResponse({ status: "bound", reason: null }),
    Error,
    "accepted",
  );
});

Deno.test("entitlement state records batch witness hash before account creation", () => {
  const hash = "a".repeat(56);
  assertEquals(createEntitlementState(hash), [
    {
      kind: "alephBatchWitnessAllowlist",
      hash,
      rewardAddress: batchWitnessRewardAddress(hash),
    },
  ]);
});

Deno.test("batch witness reward address uses script credential on preprod", () => {
  assertEquals(
    batchWitnessRewardAddress("6f9aa8f0dc33673884a82c529322fe8caa44e13dd4d3ca1fd4bf6e2f"),
    "stake_test17phe428smsekwwyy4qk99yezl6x2538p8h2d8jsl6jlkutc69q989",
  );
});

Deno.test("operator key hash is derived from green order agent batcher key", () => {
  assertEquals(
    operatorKeyHashFromBech32(
      "xprv1zzfs2eqknq4z730pg84782l2mja8vskq5zph99r37vgplpa7razfmqsey2mg08l75refatytt504gh2cl6ld52qe5k47pp5n4zaukxzc7c4cnasd825qvpm3lss3k8vjuv8gus9qpue0ulgf5uk7gr2dwy4en3va",
    ),
    "cd52b4976906bfe539d5c8cc6d8101f3648c924bd58f3ffed43f46e9",
  );
});

Deno.test("TypeScript Aleph intention digest matches Rust fixture", () => {
  assertEquals(
    alephIntentionDigest({
      targetNonceSlot: 2,
      targetNonceValue: 10n,
      inputAsset: adaAsset(),
      leavingAmount: 1_000_000n,
      outputAsset: nativeAsset("01".repeat(28), "7431"),
      expectedArrivingAmount: 900_000n,
      feeLovelace: 2_000_000n,
      operatorKeyHash: "07".repeat(28),
    }),
    "224624d7aaca91e4cff7ade3f50e01172db3ccc048edaff9d6a5bbc9a8a77982",
  );
});

Deno.test("TypeScript Aleph intention CBOR uses Aiken tuple serialization", () => {
  const input = {
    targetNonceSlot: 0,
    targetNonceValue: 1n,
    inputAsset: adaAsset(),
    leavingAmount: 1_000_000n,
    outputAsset: nativeAsset(
      "aaf945ecdbe9256312f8d90bdd7cd904f558e9be2cf7aa92c4c2bf19",
      "677265656e61a6ae0db12dbdc9",
    ),
    expectedArrivingAmount: 1n,
    feeLovelace: 2_000_000n,
    operatorKeyHash: "cd52b4976906bfe539d5c8cc6d8101f3648c924bd58f3ffed43f46e9",
  };

  assertEquals(
    alephIntentionCbor(input),
    "d8799f9f0001ff9f4040ff1a000f42409f581caaf945ecdbe9256312f8d90bdd7cd904f558e9be2cf7aa92c4c2bf194d677265656e61a6ae0db12dbdc9ff011a001e8480581ccd52b4976906bfe539d5c8cc6d8101f3648c924bd58f3ffed43f46e9ff",
  );
  assertEquals(
    alephIntentionDigest(input),
    "8ab23d6f544cc866eed7021dd349318a0533e826cd3e9cb89bd25502abcb63b5",
  );
});

Deno.test("TypeScript legacy Aleph intention CBOR uses single nonce serialization", () => {
  const input = {
    alephAbi: "legacy" as const,
    targetNonceSlot: 0,
    targetNonceValue: 0n,
    inputAsset: adaAsset(),
    leavingAmount: 1_000_000n,
    outputAsset: nativeAsset(
      "aaf945ecdbe9256312f8d90bdd7cd904f558e9be2cf7aa92c4c2bf19",
      "677265656e61a6ae0db12dbdc9",
    ),
    expectedArrivingAmount: 1n,
    feeLovelace: 2_000_000n,
    operatorKeyHash: "cd52b4976906bfe539d5c8cc6d8101f3648c924bd58f3ffed43f46e9",
  };

  assertEquals(
    alephIntentionCbor(input),
    "d8799f009f4040ff1a000f42409f581caaf945ecdbe9256312f8d90bdd7cd904f558e9be2cf7aa92c4c2bf194d677265656e61a6ae0db12dbdc9ff011a001e8480581ccd52b4976906bfe539d5c8cc6d8101f3648c924bd58f3ffed43f46e9ff",
  );
});
