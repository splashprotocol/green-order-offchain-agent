import { assertEquals } from "https://deno.land/std@0.224.0/assert/mod.ts";
import { Constr, Data } from "npm:@lucid-evolution/lucid@0.3.53";
import { parseAccountDatum } from "../src/account_datum.ts";

Deno.test("parseAccountDatum reads current ABI nonce main key and store root", () => {
  const datum = Data.to(
    new Constr(0, [
      0n,
      "00".repeat(32),
      [1n, 0n, 0n, 0n],
      ["11".repeat(33)],
      ["22".repeat(33)],
      "33".repeat(32),
    ] as any) as any,
  );

  const parsed = parseAccountDatum(datum, "current");

  assertEquals(parsed.nonce0, 1n);
  assertEquals(parsed.mainKeyHex, "11".repeat(33));
  assertEquals(parsed.storeRootHex, "33".repeat(32));
});

Deno.test("parseAccountDatum reads legacy ABI nonce and main key", () => {
  const datum = Data.to(new Constr(0, [0n, 2n, "03" + "44".repeat(32), "55".repeat(32)] as any) as any);

  const parsed = parseAccountDatum(datum, "legacy");

  assertEquals(parsed.nonce0, 2n);
  assertEquals(parsed.mainKeyHex, "03" + "44".repeat(32));
  assertEquals(parsed.storeRootHex, undefined);
});
