use std::sync::Arc;

use bloom_offchain::execution_engine::funding_effect::FundingEvent;
use bloom_offchain_cardano::event_sink::order_index::KvIndex;
use cardano_explorer::CardanoNetwork;
use cml_chain::builders::tx_builder::TransactionUnspentOutput;
use log::{debug, info};
use spectrum_cardano_lib::output::FinalizedTxOut;
use spectrum_cardano_lib::transaction::TransactionOutputExtension;
use spectrum_cardano_lib::value::ValueExtension;
use spectrum_cardano_lib::{AssetClass, OutputRef};
use spectrum_offchain::display::display_vec;
use spectrum_offchain_cardano::funding::FundingAddresses;
use tokio::sync::Mutex;

pub fn funding_events_from_utxos<const N: usize>(
    funding_addresses: &FundingAddresses<N>,
    skip_set: OutputRef,
    min_lovelace: u64,
    utxos: Vec<TransactionUnspentOutput>,
) -> Vec<(usize, FundingEvent<FinalizedTxOut>)> {
    let mut events = Vec::new();

    for utxo in utxos {
        let o_ref = OutputRef::from(utxo.input);
        if o_ref == skip_set {
            debug!(
                "Skipping operator funding candidate {} because it is the collateral set",
                o_ref
            );
            continue;
        }

        let Some(partition) = funding_addresses.partition_by_address(utxo.output.address()) else {
            continue;
        };

        let lovelace = utxo
            .output
            .value()
            .amount_of(AssetClass::Native)
            .unwrap_or_default();
        if lovelace < min_lovelace {
            debug!(
                "Skipping operator funding candidate {} because it has {} lovelace, below {}",
                o_ref, lovelace, min_lovelace
            );
            continue;
        }

        events.push((
            partition,
            FundingEvent::Produced(FinalizedTxOut::new(utxo.output, o_ref)),
        ));
    }

    events.sort_by_key(|(partition, event)| (*partition, funding_event_ref(event)));
    events
}

pub async fn seed_funding_index<Index>(
    funding_index: Arc<Mutex<Index>>,
    events: &[(usize, FundingEvent<FinalizedTxOut>)],
) where
    Index: KvIndex<OutputRef, (usize, FinalizedTxOut)>,
{
    let mut index = funding_index.lock().await;
    index.run_eviction();
    for (partition, event) in events {
        match event {
            FundingEvent::Produced(tx_out) => {
                index.put(tx_out.reference(), (*partition, tx_out.clone()));
            }
            FundingEvent::Consumed(tx_out) => {
                index.register_for_eviction(tx_out.reference());
            }
        }
    }
}

pub async fn bootstrap_funding_from_explorer<const N: usize, Net>(
    explorer: &Net,
    funding_addresses: FundingAddresses<N>,
    skip_set: OutputRef,
    min_lovelace: u64,
    page_limit: u16,
) -> Vec<(usize, FundingEvent<FinalizedTxOut>)>
where
    Net: CardanoNetwork + Sync,
{
    let mut utxos = Vec::new();
    for partition in 0..N {
        let address = funding_addresses[partition].clone();
        let mut offset = 0;
        loop {
            let page = explorer
                .utxos_by_address(address.clone(), offset, page_limit)
                .await;
            if page.is_empty() {
                break;
            }
            offset += u32::from(page_limit);
            utxos.extend(page);
        }
    }

    let events = funding_events_from_utxos(&funding_addresses, skip_set, min_lovelace, utxos);
    let refs: Vec<_> = events.iter().map(|(_, event)| funding_event_ref(event)).collect();
    info!(
        "Bootstrapped {} current operator funding UTxOs from explorer: {}",
        refs.len(),
        display_vec(&refs)
    );
    events
}

fn funding_event_ref(event: &FundingEvent<FinalizedTxOut>) -> OutputRef {
    match event {
        FundingEvent::Consumed(tx_out) | FundingEvent::Produced(tx_out) => tx_out.reference(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cml_chain::address::Address;
    use cml_chain::assets::MultiAsset;
    use cml_chain::builders::tx_builder::TransactionUnspentOutput;
    use cml_chain::transaction::{TransactionInput, TransactionOutput};
    use cml_chain::Value;
    use cml_crypto::{RawBytesEncoding, TransactionHash};

    const FUNDING_ADDRESS_1: &str =
        "addr_test1qrx49dyhdyrtlefe6hyvcmvpq8ekfryjf02c70l76sl5d6glc0zcy52euulpk433tx9send350u6mhc36520mv5tnfds0y8y32";
    const FUNDING_ADDRESS_2: &str =
        "addr_test1qrx49dyhdyrtlefe6hyvcmvpq8ekfryjf02c70l76sl5d6gm44jsrzngflqh3syahtdna5se4n7veslcw8cp0c9xq33q57ncdm";
    const FUNDING_ADDRESS_3: &str =
        "addr_test1qrx49dyhdyrtlefe6hyvcmvpq8ekfryjf02c70l76sl5d6v37vz3lw47ufln089mr6rhdc87ntk6gszhuvkqk3nhdqmqp4u2na";
    const FUNDING_ADDRESS_4: &str =
        "addr_test1qrx49dyhdyrtlefe6hyvcmvpq8ekfryjf02c70l76sl5d6tqvx4aurmtcae3f7maccweezlh4fvq2gxcukfzaqmrepxs0kj42c";

    #[test]
    fn funding_events_from_utxos_keeps_operator_funding_outputs() {
        let funding_addresses = test_funding_addresses();
        let first = funding_addresses[0].clone();
        let utxo = tx_unspent_at(
            first,
            "78c669207607a5007f3201b0f31aea3375c9de2a12ca2b7172af484ccc9f2504",
            0,
            50_000_000,
        );

        let events = funding_events_from_utxos(&funding_addresses, unrelated_ref(), 50_000_000, vec![utxo]);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, 0);
        assert!(matches!(events[0].1, FundingEvent::Produced(_)));
    }

    #[test]
    fn funding_events_from_utxos_skips_collateral_ref() {
        let funding_addresses = test_funding_addresses();
        let first = funding_addresses[0].clone();
        let collateral_ref = ref_from_hex(
            "78c669207607a5007f3201b0f31aea3375c9de2a12ca2b7172af484ccc9f2504",
            0,
        );
        let utxo = tx_unspent_at(
            first,
            collateral_ref.tx_hash().to_hex(),
            collateral_ref.index(),
            50_000_000,
        );

        let events = funding_events_from_utxos(&funding_addresses, collateral_ref, 50_000_000, vec![utxo]);

        assert!(events.is_empty());
    }

    #[test]
    fn funding_events_from_utxos_skips_low_value_funding() {
        let funding_addresses = test_funding_addresses();
        let first = funding_addresses[0].clone();
        let utxo = tx_unspent_at(
            first,
            "3db70e29807b91be97e9f95eb399f8364e5392e2f985c31c1a6c5eba11c5fa12",
            0,
            2_000_001,
        );

        let events = funding_events_from_utxos(&funding_addresses, unrelated_ref(), 50_000_000, vec![utxo]);

        assert!(events.is_empty());
    }

    fn test_funding_addresses() -> FundingAddresses<4> {
        [
            Address::from_bech32(FUNDING_ADDRESS_1).unwrap(),
            Address::from_bech32(FUNDING_ADDRESS_2).unwrap(),
            Address::from_bech32(FUNDING_ADDRESS_3).unwrap(),
            Address::from_bech32(FUNDING_ADDRESS_4).unwrap(),
        ]
        .into()
    }

    fn tx_unspent_at(
        address: Address,
        tx_hash: impl AsRef<str>,
        output_index: u64,
        lovelace: u64,
    ) -> TransactionUnspentOutput {
        TransactionUnspentOutput::new(
            TransactionInput::new(TransactionHash::from_hex(tx_hash.as_ref()).unwrap(), output_index),
            TransactionOutput::new(address, Value::new(lovelace, MultiAsset::new()), None, None),
        )
    }

    fn ref_from_hex(tx_hash: impl AsRef<str>, output_index: u64) -> OutputRef {
        OutputRef::new(TransactionHash::from_hex(tx_hash.as_ref()).unwrap(), output_index)
    }

    fn unrelated_ref() -> OutputRef {
        ref_from_hex(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            0,
        )
    }
}
