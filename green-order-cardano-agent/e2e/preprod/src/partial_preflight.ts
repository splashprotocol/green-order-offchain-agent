export type PartialFillInput = {
  poolLovelaceReserve: bigint;
  poolTokenReserve: bigint;
  poolAssetY: string;
  leavingLovelace: bigint;
  expectedTokenAmount: bigint;
  feeLovelace: bigint;
};

export type PartialFillPlan = {
  consumedLeaving: bigint;
  receivedOutput: bigint;
  remainingLeaving: bigint;
  remainingExpectedOutput: bigint;
  remainingFee: bigint;
};

export async function computePartialFillPlan(input: PartialFillInput): Promise<PartialFillPlan> {
  const payload = JSON.stringify({
    poolLovelaceReserve: input.poolLovelaceReserve.toString(),
    poolTokenReserve: input.poolTokenReserve.toString(),
    poolAssetY: input.poolAssetY,
    leavingLovelace: input.leavingLovelace.toString(),
    expectedTokenAmount: input.expectedTokenAmount.toString(),
    feeLovelace: input.feeLovelace.toString(),
  });
  const command = new Deno.Command(await partialPreflightCommand(), {
    stdin: "piped",
    stdout: "piped",
    stderr: "piped",
  });
  const child = command.spawn();
  const writer = child.stdin.getWriter();
  await writer.write(new TextEncoder().encode(payload));
  await writer.close();
  const output = await child.output();
  if (!output.success) {
    throw new Error(`Rust partial preflight failed: ${new TextDecoder().decode(output.stderr).trim()}`);
  }
  const parsed = JSON.parse(new TextDecoder().decode(output.stdout));
  return {
    consumedLeaving: BigInt(String(parsed.consumedLeaving)),
    receivedOutput: BigInt(String(parsed.receivedOutput)),
    remainingLeaving: BigInt(String(parsed.remainingLeaving)),
    remainingExpectedOutput: BigInt(String(parsed.remainingExpectedOutput)),
    remainingFee: BigInt(String(parsed.remainingFee)),
  };
}

export function partialPlanToJson(plan: PartialFillPlan): Record<string, string> {
  return {
    consumedLeaving: plan.consumedLeaving.toString(),
    receivedOutput: plan.receivedOutput.toString(),
    remainingLeaving: plan.remainingLeaving.toString(),
    remainingExpectedOutput: plan.remainingExpectedOutput.toString(),
    remainingFee: plan.remainingFee.toString(),
  };
}

function repoRoot(): string {
  return new URL("../../../..", import.meta.url).pathname.replace(/\/$/, "");
}

async function partialPreflightCommand(): Promise<string> {
  const override = Deno.env.get("PARTIAL_PREFLIGHT_BIN")?.trim();
  if (override) return override;
  const path = `${repoRoot()}/target/debug/partial_preflight`;
  try {
    const stat = await Deno.stat(path);
    if (stat.isFile) return path;
  } catch (error) {
    if (!(error instanceof Deno.errors.NotFound)) throw error;
  }
  throw new Error(
    `Build partial preflight helper first: cargo build -q -p green-order-cardano-agent --bin partial_preflight`,
  );
}
