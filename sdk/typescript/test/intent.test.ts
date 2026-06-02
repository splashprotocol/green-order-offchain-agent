import test from "node:test";
import assert from "node:assert/strict";

import {
  adaAsset,
  alephIntentionDigest,
  buildIntentPayload,
  nativeAsset,
  signGreenOrderIntent,
} from "../src/index.js";

const accountId = "11".repeat(32);
const operatorKeyHash = "22".repeat(28);
const policyId = "33".repeat(28);
const assetName = "677265656e";
const fixtureDigest = "224624d7aaca91e4cff7ade3f50e01172db3ccc048edaff9d6a5bbc9a8a77982";

test("encodes ADA and native assets for the agent wire format", () => {
  assert.equal(adaAsset().wireId, "00");
  assert.equal(nativeAsset(policyId, assetName).wireId, `${policyId}${assetName}`);
});

test("computes current Aleph intention digest compatible with the preprod fixture", () => {
  const digest = alephIntentionDigest({
    targetNonceSlot: 2,
    targetNonceValue: 10n,
    inputAsset: adaAsset(),
    leavingAmount: 1_000_000n,
    outputAsset: nativeAsset("01".repeat(28), "7431"),
    expectedArrivingAmount: 900_000n,
    feeLovelace: 2_000_000n,
    operatorKeyHash: "07".repeat(28),
  });

  assert.equal(digest, fixtureDigest);
});

test("builds the JSON payload accepted by POST /intents", () => {
  const payload = buildIntentPayload({
    accountId,
    originalIntentDigest: "44".repeat(32),
    inputAsset: adaAsset(),
    outputAsset: nativeAsset(policyId, assetName),
    leavingAmount: 1_000_000n,
    expectedArrivingAmount: 900_000n,
    feeLovelace: 2_000_000n,
    targetNonceSlot: 0,
    targetNonceValue: 1n,
    operatorKeyHash,
    auth: {
      type: "sig",
      prefix: "",
      postfix: "",
      signature: "55".repeat(64),
      updateProof: "",
    },
  });

  assert.deepEqual(payload, {
    accountId,
    originalIntentDigest: "44".repeat(32),
    inputAsset: "00",
    outputAsset: `${policyId}${assetName}`,
    leavingAmount: 1_000_000,
    expectedArrivingAmount: 900_000,
    feeLovelace: 2_000_000,
    targetNonceSlot: 0,
    targetNonceValue: 1,
    operatorKeyHash,
    auth: {
      type: "sig",
      prefix: "",
      postfix: "",
      signature: "55".repeat(64),
      updateProof: "",
    },
  });
});

test("signs an intent through an injected signer without owning wallet keys", async () => {
  const signed = await signGreenOrderIntent(
    {
      accountId,
      inputAsset: adaAsset(),
      outputAsset: nativeAsset(policyId, assetName),
      leavingAmount: 1_000_000n,
      expectedArrivingAmount: 900_000n,
      feeLovelace: 2_000_000n,
      targetNonceSlot: 0,
      targetNonceValue: 1n,
      operatorKeyHash,
    },
    {
      sign: async (messageHex) => `aa${messageHex.slice(2)}`.padEnd(128, "0").slice(0, 128),
    },
  );

  assert.equal(signed.originalIntentDigest, "213f306e107aed143c2ea7a822ebf29c03c4890a47949c6950309eb1a89fbce1");
  assert.equal(signed.auth.signature.length, 128);
});
