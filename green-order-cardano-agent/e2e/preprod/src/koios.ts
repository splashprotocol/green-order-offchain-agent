import { OutputRef } from "./state.ts";

export type SimpleUtxo = {
  txHash: string;
  outputIndex: number;
  datum?: string | null;
  assets: Record<string, bigint>;
};

export async function koiosUtxoByRef(ref: OutputRef): Promise<SimpleUtxo | undefined> {
  const rows = await koiosPost("utxo_info", {
    _utxo_refs: [`${ref.txHash}#${ref.outputIndex}`],
    _extended: true,
  });
  return rows.filter((row) => row.is_spent !== true).map(parseKoiosUtxo)[0];
}

export async function koiosUtxosAt(address: string): Promise<SimpleUtxo[]> {
  const rows = await koiosPost("address_utxos", {
    _addresses: [address],
    _extended: true,
  });
  return rows.map(parseKoiosUtxo);
}

async function koiosPost(endpoint: string, body: unknown): Promise<any[]> {
  const baseUrl = Deno.env.get("KOIOS_PREPROD_URL")?.trim() ?? "https://preprod.koios.rest/api/v1";
  const response = await fetch(`${baseUrl}/${endpoint}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    throw new Error(`Koios ${endpoint} failed ${response.status}: ${await response.text()}`);
  }
  return await response.json();
}

function parseKoiosUtxo(row: any): SimpleUtxo {
  const assets: Record<string, bigint> = { lovelace: BigInt(row.value) };
  for (const asset of row.asset_list ?? []) {
    const unit = `${asset.policy_id}${asset.asset_name ?? ""}`;
    assets[unit] = BigInt(asset.quantity);
  }
  return {
    txHash: row.tx_hash,
    outputIndex: Number(row.tx_index),
    datum: row.inline_datum?.bytes ?? null,
    assets,
  };
}
