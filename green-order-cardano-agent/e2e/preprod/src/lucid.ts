import { Lucid, LucidEvolution } from "npm:@lucid-evolution/lucid@0.3.53";
import { PreprodE2eConfig } from "./config.ts";
import { preprodProvider } from "./provider.ts";
import { withRetries } from "./retry.ts";

export async function getLucid(config: PreprodE2eConfig): Promise<LucidEvolution> {
  const lucid = await Lucid(preprodProvider(), "Preprod");
  lucid.selectWallet.fromSeed(config.fundedWalletSeed, { addressType: "Enterprise" });
  return lucid;
}

export async function submitAndWait(
  tx: { sign: { withWallet: () => { complete: () => Promise<SignedTx> } } },
  awaitTx: (txHash: string) => Promise<boolean>,
): Promise<string> {
  const signed = await withRetries(() => tx.sign.withWallet().complete(), {
    description: "sign transaction",
  });
  const txHash = await submitSignedTx(signed);
  const confirmed = await withRetries(() => awaitTx(txHash), {
    description: `wait for transaction ${txHash}`,
  });
  if (!confirmed) {
    throw new Error(`transaction ${txHash} was not confirmed`);
  }
  return txHash;
}

type SignedTx = {
  submit: () => Promise<string>;
  toCBOR: () => string;
  toHash: () => string;
};

export async function submitSignedTx(signed: SignedTx): Promise<string> {
  return await withRetries(
    async () => {
      try {
        return await signed.submit();
      } catch (error) {
        const koiosUrl = Deno.env.get("KOIOS_PREPROD_URL")?.trim() ?? "https://preprod.koios.rest/api/v1";
        const response = await fetch(`${koiosUrl}/submittx`, {
          method: "POST",
          headers: { "content-type": "application/cbor" },
          body: hexToBytes(signed.toCBOR()),
        });
        if (!response.ok) {
          throw new Error(
            `provider submit failed (${error}); direct Koios submit failed ${response.status}: ${await response
              .text()}`,
          );
        }
        const returned = (await response.text()).trim().replace(/^"|"$/g, "");
        const expected = signed.toHash();
        if (returned && returned !== expected) {
          throw new Error(`direct Koios submit returned tx hash ${returned}, expected ${expected}`);
        }
        return expected;
      }
    },
    { description: `submit transaction ${signed.toHash()}` },
  );
}

function hexToBytes(hex: string): Uint8Array {
  if (hex.length % 2 !== 0) throw new Error("hex string has odd length");
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return bytes;
}
