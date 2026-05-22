use std::{env, fs};

use cbor_event::se::Serializer;
use cml_chain::address::Address;
use cml_chain::assets::AssetName;
use cml_chain::plutus::{PlutusData, PlutusV1Script, PlutusV2Script, PlutusV3Script};
use cml_chain::transaction::{DatumOption, Transaction, TransactionInput, TransactionOutput};
use cml_chain::Deserialize;
use cml_chain::{PolicyId, Script, Value};
use cml_core::serialization::Serialize;
use cml_crypto::TransactionHash;
use serde::Deserialize as SerdeDeserialize;

fn usage() -> ! {
    eprintln!("usage: tx_sim_inputs <tx.cbor> <inputs.hex> <outputs.hex>");
    std::process::exit(2);
}

#[tokio::main]
async fn main() {
    let tx_path = env::args().nth(1).unwrap_or_else(|| usage());
    let inputs_path = env::args().nth(2).unwrap_or_else(|| usage());
    let outputs_path = env::args().nth(3).unwrap_or_else(|| usage());

    let tx_bytes = fs::read(tx_path).expect("read tx cbor");
    let tx = Transaction::from_cbor_bytes(&tx_bytes).expect("decode tx");
    let mut inputs = Vec::<TransactionInput>::new();
    let mut outputs = Vec::<TransactionOutput>::new();
    let all_inputs = tx
        .body
        .inputs
        .iter()
        .chain(tx.body.reference_inputs.iter().flat_map(|refs| refs.iter()))
        .collect::<Vec<_>>();
    let refs = all_inputs
        .iter()
        .map(|input| format!("{}#{}", input.transaction_id, input.index))
        .collect::<Vec<_>>();
    let body = serde_json::json!({
        "_utxo_refs": refs,
        "_extended": true,
    });
    let response = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("http client")
        .post("https://preprod.koios.rest/api/v1/utxo_info")
        .json(&body)
        .send()
        .await
        .expect("koios request");
    let mut utxos = response
        .json::<Vec<serde_json::Value>>()
        .await
        .expect("koios json");
    for input in all_inputs {
        let ref_text = format!("{}#{}", input.transaction_id, input.index);
        let idx = utxos
            .iter()
            .position(|row| {
                row["tx_hash"].as_str() == Some(input.transaction_id.to_hex().as_str())
                    && row["tx_index"].as_u64() == Some(input.index)
            })
            .unwrap_or_else(|| panic!("missing input {}#{}", input.transaction_id, input.index));
        let row = utxos.remove(idx);
        let utxo =
            parse_utxo_json(row.clone()).unwrap_or_else(|| panic!("failed to parse {ref_text}: {row}"));
        inputs.push(utxo.input);
        outputs.push(utxo.output);
    }

    write_vec(&inputs_path, inputs);
    write_vec(&outputs_path, outputs);
}

fn write_vec<T: Serialize>(path: &str, values: Vec<T>) {
    let mut serializer = Serializer::new_vec();
    serializer
        .write_array(cbor_event::Len::Len(values.len() as u64))
        .expect("write array header");
    for value in values {
        value.serialize(&mut serializer, false).expect("write cbor item");
    }
    fs::write(path, hex::encode(serializer.finalize())).expect("write cbor hex");
}

#[derive(SerdeDeserialize)]
struct KoiosUtxo {
    tx_hash: String,
    tx_index: u64,
    address: String,
    value: String,
    #[serde(default)]
    inline_datum: Option<KoiosBytes>,
    #[serde(default)]
    reference_script: Option<KoiosReferenceScript>,
    #[serde(default)]
    asset_list: Vec<KoiosAsset>,
}

#[derive(SerdeDeserialize)]
struct KoiosBytes {
    bytes: String,
}

#[derive(SerdeDeserialize)]
struct KoiosReferenceScript {
    #[serde(rename = "type")]
    script_type: String,
    bytes: String,
}

#[derive(SerdeDeserialize)]
struct KoiosAsset {
    policy_id: String,
    asset_name: String,
    quantity: String,
}

fn parse_utxo_json(
    row: serde_json::Value,
) -> Option<cml_chain::builders::tx_builder::TransactionUnspentOutput> {
    let utxo: KoiosUtxo = serde_json::from_value(row).ok()?;
    let mut value = Value::zero();
    value.coin = utxo.value.parse::<u64>().ok()?;
    for asset in utxo.asset_list {
        let policy = PolicyId::from_hex(asset.policy_id.as_str()).ok()?;
        let asset_name = AssetName::new(hex::decode(asset.asset_name).ok()?).ok()?;
        let quantity = asset.quantity.parse::<u64>().ok()?;
        value.multiasset.set(policy, asset_name, quantity);
    }
    let datum = utxo.inline_datum.and_then(|datum| {
        PlutusData::from_cbor_bytes(hex::decode(datum.bytes).ok()?.as_slice())
            .ok()
            .map(DatumOption::new_datum)
    });
    let script = utxo.reference_script.and_then(|script| {
        let bytes = hex::decode(script.bytes).ok()?;
        match script.script_type.as_str() {
            "plutusV1" => Some(Script::new_plutus_v1(PlutusV1Script::new(bytes))),
            "plutusV2" => Some(Script::new_plutus_v2(PlutusV2Script::new(bytes))),
            "plutusV3" => Some(Script::new_plutus_v3(PlutusV3Script::new(bytes))),
            _ => None,
        }
    });
    Some(cml_chain::builders::tx_builder::TransactionUnspentOutput::new(
        TransactionInput::new(
            TransactionHash::from_hex(utxo.tx_hash.as_str()).ok()?,
            utxo.tx_index,
        ),
        TransactionOutput::new(
            Address::from_bech32(utxo.address.as_str()).ok()?,
            value,
            datum,
            script,
        ),
    ))
}
