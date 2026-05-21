use std::{env, fs};

use cml_chain::plutus::Redeemers;
use cml_chain::transaction::Transaction;
use cml_chain::Deserialize;
use cml_core::serialization::Serialize;

fn main() {
    let path = env::args().nth(1).expect("usage: dump_tx <tx.cbor>");
    let bytes = fs::read(path).expect("read tx cbor");
    let tx = Transaction::from_cbor_bytes(&bytes).expect("decode tx");

    println!("tx_hash={}", tx.body.hash());
    println!("fee={}", tx.body.fee);
    println!("inputs:");
    for (ix, input) in tx.body.inputs.iter().enumerate() {
        println!("  [{ix}] {}#{}", input.transaction_id, input.index);
    }
    println!("reference_inputs:");
    if let Some(reference_inputs) = &tx.body.reference_inputs {
        for (ix, input) in reference_inputs.iter().enumerate() {
            println!("  [{ix}] {}#{}", input.transaction_id, input.index);
        }
    }
    println!("required_signers:");
    if let Some(signers) = &tx.body.required_signers {
        for (ix, signer) in signers.iter().enumerate() {
            println!("  [{ix}] {signer}");
        }
    }
    println!("withdrawals:");
    if let Some(withdrawals) = &tx.body.withdrawals {
        for (reward_account, amount) in withdrawals.iter() {
            println!("  {:?} => {}", reward_account, amount);
        }
    }
    println!("outputs:");
    for (ix, output) in tx.body.outputs.iter().enumerate() {
        println!("  [{ix}] address={:?}", output.address());
        println!("      amount={:?}", output.amount());
        println!("      datum={:?}", output.datum());
    }
    println!("redeemers:");
    if let Some(redeemers) = &tx.witness_set.redeemers {
        match redeemers {
            Redeemers::ArrLegacyRedeemer {
                arr_legacy_redeemer,
                ..
            } => {
                for redeemer in arr_legacy_redeemer {
                    println!(
                        "  legacy tag={:?} index={} ex_units={:?} data_cbor={}",
                        redeemer.tag,
                        redeemer.index,
                        redeemer.ex_units,
                        hex::encode(redeemer.data.to_cbor_bytes())
                    );
                }
            }
            Redeemers::MapRedeemerKeyToRedeemerVal {
                map_redeemer_key_to_redeemer_val,
                ..
            } => {
                for (key, val) in map_redeemer_key_to_redeemer_val.iter() {
                    println!(
                        "  map tag={:?} index={} ex_units={:?} data_cbor={}",
                        key.tag,
                        key.index,
                        val.ex_units,
                        hex::encode(val.data.to_cbor_bytes())
                    );
                }
            }
        }
    }
    println!("vkey_witnesses:");
    if let Some(vkeys) = &tx.witness_set.vkeywitnesses {
        for (ix, witness) in vkeys.iter().enumerate() {
            println!("  [{ix}] {:?}", witness);
        }
    }
}
