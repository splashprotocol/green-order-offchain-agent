import test from "node:test";
import assert from "node:assert/strict";

import { buildBindAccountPayload, buildAccountStatusQuery } from "../src/index.js";

test("builds account bind payloads for the agent route", () => {
  assert.deepEqual(
    buildBindAccountPayload("aa", {
      txHash: "bb",
      outputIndex: 7,
    }),
    {
      accountId: "aa",
      txHash: "bb",
      outputIndex: 7,
    },
  );
});

test("builds account status query strings for the existing route", () => {
  assert.equal(
    buildAccountStatusQuery({
      accountId: "aa",
      txHash: "bb",
      outputIndex: 7,
    }),
    "accountId=aa&txHash=bb&outputIndex=7",
  );
});
