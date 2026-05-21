import { Blockfrost, Koios, Provider } from "npm:@lucid-evolution/lucid@0.3.53";

export async function loadDotEnv(path: string): Promise<void> {
  try {
    const text = await Deno.readTextFile(path);
    for (const line of text.split(/\r?\n/)) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith("#") || !trimmed.includes("=")) continue;
      const [name, ...rest] = trimmed.split("=");
      if (!Deno.env.get(name)) {
        Deno.env.set(name, rest.join("="));
      }
    }
  } catch (error) {
    if (!(error instanceof Deno.errors.NotFound)) throw error;
  }
}

export async function loadPreprodEnv(): Promise<void> {
  await loadDotEnv(".env");
  await loadDotEnv(".env.wallets");
}

export function preprodProvider(): Provider {
  const blockfrostProjectId = Deno.env.get("BLOCKFROST_PROJECT_ID")?.trim();
  if (blockfrostProjectId) {
    const url = Deno.env.get("BLOCKFROST_PREPROD_URL")?.trim() ?? "https://cardano-preprod.blockfrost.io/api/v0";
    return new Blockfrost(url, blockfrostProjectId);
  }
  return new Koios(Deno.env.get("KOIOS_PREPROD_URL")?.trim() ?? "https://preprod.koios.rest/api/v1");
}

export function lovelaceToAda(lovelace: bigint): string {
  const whole = lovelace / 1_000_000n;
  const frac = (lovelace % 1_000_000n).toString().padStart(6, "0");
  return `${whole}.${frac}`;
}
