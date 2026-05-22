import { Constr, Data } from "npm:@lucid-evolution/lucid@0.3.53";

export type ParsedAccountDatum = {
  nonce0: bigint;
  mainKeyHex: string;
  storeRootHex?: string;
};

export function parseAccountDatum(
  datumCbor: string,
  alephAbi: "current" | "legacy",
): ParsedAccountDatum {
  const datum = Data.from(datumCbor) as unknown;
  if (alephAbi === "legacy") {
    if (!(datum instanceof Constr) || datum.index !== 0 || datum.fields.length !== 4) {
      throw new Error("successor legacy account datum has unexpected shape");
    }
    if (typeof datum.fields[1] !== "bigint" || typeof datum.fields[2] !== "string") {
      throw new Error("successor legacy account nonce or main_key is malformed");
    }
    return { nonce0: datum.fields[1], mainKeyHex: datum.fields[2] };
  }

  if (!(datum instanceof Constr) || datum.index !== 0 || datum.fields.length !== 6) {
    throw new Error("successor account datum has unexpected shape");
  }
  const nonce = datum.fields[2];
  if (!Array.isArray(nonce) || typeof nonce[0] !== "bigint") {
    throw new Error("successor account nonce[0] is missing or malformed");
  }
  const hotCred = datum.fields[3];
  if (!Array.isArray(hotCred) || typeof hotCred[0] !== "string") {
    throw new Error("successor account main_key is missing or malformed");
  }
  const storeRootHex = datum.fields[5];
  if (typeof storeRootHex !== "string") {
    throw new Error("successor account store root is missing or malformed");
  }
  return { nonce0: nonce[0], mainKeyHex: hotCred[0], storeRootHex };
}
