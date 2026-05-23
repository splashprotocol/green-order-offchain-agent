import { Lucid } from "npm:@lucid-evolution/lucid@0.3.53";
import { submitSignedTx } from "./src/lucid.ts";
import { loadConfig } from "./src/config.ts";
import { loadState } from "./src/state.ts";
import { loadPreprodEnv, lovelaceToAda, preprodProvider } from "./src/provider.ts";
import { koiosUtxosAt } from "./src/koios.ts";

type OperatorState = {
  fundingAddresses: string[];
};

await loadPreprodEnv();
const config = await loadConfig();
const state = await loadState(config.statePath);
const operator = JSON.parse(await Deno.readTextFile(".state/preprod-operator.json")) as OperatorState;

if (!state.account) throw new Error("Run 03-create-aleph-account-and-bind.ts first");
if (!state.pool) throw new Error("Run 01-create-royalty-v1-pool.ts first");

const lower = state.account.outputRef.txHash;
const upper = state.pool.outputRef.txHash;
if (!(lower < upper)) {
  throw new Error(`account ref ${lower} must sort before pool ref ${upper}`);
}

const separatorAddresses = operator.fundingAddresses;
const separatorLovelace = BigInt(Deno.env.get("SEPARATOR_FUNDING_LOVELACE") ?? "50000000");

const existingByAddress = await Promise.all(
  separatorAddresses.map(async (address) => ({
    address,
    utxos: await preprodProvider().getUtxos(address),
  })),
);
const existingSeparators = existingByAddress.map(({ address, utxos }) => ({
  address,
  separator: utxos.find((utxo) =>
    lower < utxo.txHash && utxo.txHash < upper && (utxo.assets.lovelace ?? 0n) >= separatorLovelace
  ),
}));
if (existingSeparators.every(({ separator }) => separator)) {
  console.log(
    `separator_funding_already_available=${
      existingSeparators
        .map(({ separator }) => `${separator!.txHash}#${separator!.outputIndex}`)
        .join(",")
    }`,
  );
  Deno.exit(0);
}

const lucid = await Lucid(preprodProvider(), "Preprod");
lucid.selectWallet.fromSeed(config.fundedWalletSeed, { addressType: "Enterprise" });

const batcherAddress = await lucid.wallet().address();
const expectedBatcherAddress = Deno.env.get("BATCHER_ADDRESS")?.trim();
if (expectedBatcherAddress && batcherAddress !== expectedBatcherAddress) {
  throw new Error(`funding seed derives ${batcherAddress}, expected ${expectedBatcherAddress}`);
}

const maxAttempts = Number(Deno.env.get("SEPARATOR_FUNDING_MAX_ATTEMPTS") ?? "512");
for (let attempt = 0; attempt < maxAttempts; attempt++) {
  const lovelace = separatorLovelace + BigInt(attempt);
  let txBuilder = lucid.newTx();
  for (const address of separatorAddresses) {
    txBuilder = txBuilder.pay.ToAddress(address, { lovelace });
  }
  const tx = await txBuilder.complete();
  const signed = await tx.sign.withWallet().complete();
  const txHash = signed.toHash();
  if (lower < txHash && txHash < upper) {
    const submitted = await submitSignedTx(signed);
    console.log(`separator_funding_tx=${submitted}`);
    console.log(`separator_funding_addresses=${separatorAddresses.join(",")}`);
    console.log(`separator_funding_lovelace=${lovelace}`);
    console.log(`separator_funding_ada=${lovelaceToAda(lovelace)}`);
    await waitForSeparators(separatorAddresses, submitted);
    Deno.exit(0);
  }
  if (attempt % 32 === 31) {
    console.log(`searched_separator_candidates=${attempt + 1}`);
  }
}

throw new Error(`No separator funding tx hash found in ${maxAttempts} attempts for (${lower}, ${upper})`);

async function waitForSeparators(addresses: string[], txHash: string): Promise<void> {
  for (let attempt = 0; attempt < 60; attempt++) {
    const found = await Promise.all(
      addresses.map(async (address) => {
        const utxos = await koiosUtxosAt(address).catch(() => []);
        return utxos.some((utxo) => utxo.txHash === txHash);
      }),
    );
    if (found.every(Boolean)) return;
    await new Promise((resolve) => setTimeout(resolve, 5_000));
  }
  throw new Error(`separator funding ${txHash} was not observed on every preprod funding address`);
}
