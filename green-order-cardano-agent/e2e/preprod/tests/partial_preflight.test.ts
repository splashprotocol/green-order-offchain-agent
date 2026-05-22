import { assertEquals, assertRejects } from "https://deno.land/std@0.224.0/assert/mod.ts";
import { computePartialFillPlan } from "../src/partial_preflight.ts";

Deno.test("partial preflight selects a nonzero strict partial fill using Rust pool logic", async () => {
  const plan = await computePartialFillPlan({
    poolLovelaceReserve: 40_000_000n,
    poolTokenReserve: 40_000_000n,
    poolAssetY: "01".repeat(28) + "74657374",
    leavingLovelace: 20_000_000n,
    expectedTokenAmount: 15_000_000n,
    feeLovelace: 2_000_000n,
  });

  if (plan.consumedLeaving <= 0n) throw new Error("expected positive consumed input");
  if (plan.receivedOutput <= 0n) throw new Error("expected positive received output");
  if (plan.remainingLeaving <= 0n) throw new Error("expected strict partial remainder");
  assertEquals(plan.consumedLeaving + plan.remainingLeaving, 20_000_000n);
  assertEquals(plan.receivedOutput + plan.remainingExpectedOutput, 15_000_000n);
  assertEquals(plan.remainingFee, plan.remainingLeaving * 2_000_000n / 20_000_000n);
});

Deno.test("partial preflight rejects a full-fill candidate", async () => {
  await assertRejects(
    () =>
      computePartialFillPlan({
        poolLovelaceReserve: 40_000_000n,
        poolTokenReserve: 40_000_000n,
        poolAssetY: "01".repeat(28) + "74657374",
        leavingLovelace: 1_000_000n,
        expectedTokenAmount: 1n,
        feeLovelace: 2_000_000n,
      }),
    Error,
    "not strict partial",
  );
});
