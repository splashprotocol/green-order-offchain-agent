import { assertEquals, assertRejects } from "https://deno.land/std@0.224.0/assert/mod.ts";
import { withRetries } from "../src/retry.ts";

Deno.test("withRetries retries transient provider failures", async () => {
  let attempts = 0;
  const result = await withRetries(
    async () => {
      attempts += 1;
      if (attempts < 3) throw new Error("Koios timeout");
      return "ok";
    },
    { description: "test provider call", attempts: 4, delayMs: 0 },
  );

  assertEquals(result, "ok");
  assertEquals(attempts, 3);
});

Deno.test("withRetries throws the last provider failure after exhausting attempts", async () => {
  let attempts = 0;
  await assertRejects(
    () =>
      withRetries(
        async () => {
          attempts += 1;
          throw new Error(`failure ${attempts}`);
        },
        { description: "test provider call", attempts: 2, delayMs: 0 },
      ),
    Error,
    "failure 2",
  );

  assertEquals(attempts, 2);
});
