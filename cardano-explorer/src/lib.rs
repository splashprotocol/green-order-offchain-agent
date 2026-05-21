use crate::config::ExplorerConfig;
use crate::constants::{MAINNET_PREFIX, PREPROD_PREFIX};
use crate::Network::{Mainnet, Preprod};
use async_trait::async_trait;
use blockfrost::{BlockFrostSettings, BlockfrostAPI, Order, Pagination};
use blockfrost_openapi::models::{
    AddressUtxoContentInner, TxContentOutputAmountInner, TxContentUtxoOutputsInner,
};
use cml_chain::address::Address;
use cml_chain::builders::tx_builder::TransactionUnspentOutput;
use cml_chain::plutus::{PlutusData, PlutusV1Script, PlutusV2Script, PlutusV3Script};
use cml_chain::transaction::{DatumOption, TransactionInput, TransactionOutput};
use cml_chain::{Script, Value};
use cml_core::serialization::Deserialize;
use cml_crypto::{DatumHash, TransactionHash};
use futures::future::join_all;
use log::{trace, warn};
use maestro_rust_sdk::client::maestro;
use maestro_rust_sdk::models::addresses::UtxosAtAddress;
use maestro_rust_sdk::models::transactions::RedeemerEvaluation;
use maestro_rust_sdk::utils::Parameters;
use serde::Deserialize as SerdeDeserialize;
use serde_json::json;
use spectrum_cardano_lib::value::ValueExtension;
use spectrum_cardano_lib::AssetClass::{Native, Token};
use spectrum_cardano_lib::Token as RawToken;
use spectrum_cardano_lib::{NetworkId, OutputRef, PaymentCredential};
use std::collections::HashMap;
use std::io::Error;
use std::path::Path;
use std::string::ToString;
use tokio::fs;

pub mod client;

pub mod config;
pub mod constants;
pub mod data;
pub mod retry;

#[derive(serde::Deserialize)]
pub enum Network {
    Preprod,
    Mainnet,
}

impl From<NetworkId> for Network {
    fn from(value: NetworkId) -> Self {
        match <u8>::from(value) {
            0 => Preprod,
            _ => Mainnet,
        }
    }
}

impl From<Network> for String {
    fn from(value: Network) -> Self {
        match value {
            Preprod => PREPROD_PREFIX.to_string(),
            Mainnet => MAINNET_PREFIX.to_string(),
        }
    }
}

#[async_trait]
pub trait CardanoNetwork: Sized {
    async fn utxo_by_ref(&self, oref: OutputRef) -> Option<TransactionUnspentOutput>;
    async fn utxos_by_pay_cred(
        &self,
        payment_credential: PaymentCredential,
        offset: u32,
        limit: u16,
    ) -> Vec<TransactionUnspentOutput>;
    async fn utxos_by_address(
        &self,
        address: Address,
        offset: u32,
        limit: u16,
    ) -> Vec<TransactionUnspentOutput>;
}

#[async_trait]
impl<T: CardanoNetwork + Sync> CardanoNetwork for Box<T> {
    async fn utxo_by_ref(&self, oref: OutputRef) -> Option<TransactionUnspentOutput> {
        self.as_ref().utxo_by_ref(oref).await
    }

    async fn utxos_by_pay_cred(
        &self,
        payment_credential: PaymentCredential,
        offset: u32,
        limit: u16,
    ) -> Vec<TransactionUnspentOutput> {
        self.as_ref()
            .utxos_by_pay_cred(payment_credential, offset, limit)
            .await
    }

    async fn utxos_by_address(
        &self,
        address: Address,
        offset: u32,
        limit: u16,
    ) -> Vec<TransactionUnspentOutput> {
        self.as_ref().utxos_by_address(address, offset, limit).await
    }
}

pub trait ExtendedCardanoNetwork: CardanoNetwork {
    async fn slot_indexed_utxos_by_address(&self, address: Address, offset: u32, limit: u16)
        -> Vec<UTxOInfo>;
    async fn submit_tx(&self, cbor: &[u8]) -> Result<(), Box<dyn std::error::Error>>;
    async fn chain_tip_slot_number(&self) -> Result<u64, Box<dyn std::error::Error>>;
    async fn wait_for_transaction_confirmation(
        &self,
        tx_id: TransactionHash,
    ) -> Result<(), Box<dyn std::error::Error>>;
    async fn evaluate_tx(&self, cbor: &str) -> Result<Vec<RedeemerEvaluation>, Box<dyn std::error::Error>>;
}

const LOVELACE: &str = "lovelace";

pub struct Blockfrost(BlockfrostAPI);

impl Blockfrost {
    pub async fn new<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let project_id = fs::read_to_string(path).await?.replace("\n", "");
        let settings = BlockFrostSettings::new();
        let blockfrost_client = BlockfrostAPI::new(project_id.as_str(), settings);

        Ok(Blockfrost(blockfrost_client))
    }

    async fn parse_blockfrost_output(
        &self,
        tx_hash: String,
        output_idx: u64,
        address: String,
        output_amount: Vec<TxContentOutputAmountInner>,
        inline_datum: Option<String>,
        datum_hash: Option<String>,
        ref_script_hash_opt: Option<String>,
    ) -> Option<TransactionUnspentOutput> {
        let mut script = None;
        if let Some(ref_script_hash) = ref_script_hash_opt {
            script = self
                .0
                .scripts_hash_cbor(ref_script_hash.as_str())
                .await
                .ok()
                .and_then(|opt_value| opt_value.cbor)
                .and_then(|script| PlutusV2Script::from_cbor_bytes(script.as_ref()).ok())
                .map(Script::new_plutus_v2)
        }

        let mut value = Value::zero();
        output_amount.into_iter().for_each(|token_info| {
            token_info
                .quantity
                .parse::<u64>()
                .into_iter()
                .for_each(|token_qty| match token_info.clone().unit.as_str() {
                    LOVELACE => value.add_unsafe(Native, token_qty),
                    csWithTn => RawToken::try_from_raw_string(csWithTn)
                        .into_iter()
                        .for_each(|token| value.add_unsafe(Token(token), token_qty)),
                })
        });

        let datum: Option<DatumOption> = inline_datum
            .clone()
            .and_then(|datum| {
                PlutusData::from_cbor_bytes(datum.as_ref())
                    .ok()
                    .map(DatumOption::new_datum)
            })
            .or(datum_hash.and_then(|datum_hash| {
                DatumHash::from_hex(datum_hash.as_str())
                    .ok()
                    .map(DatumOption::new_hash)
            }));

        Some(TransactionUnspentOutput {
            input: TransactionInput::new(TransactionHash::from_hex(tx_hash.as_str()).ok()?, output_idx),
            output: TransactionOutput::new(
                Address::from_bech32(address.as_str()).ok()?,
                value,
                datum,
                script,
            ),
        })
    }

    async fn blockfrost_address_utxo_to_tx_unspent_output(
        &self,
        utxo: AddressUtxoContentInner,
    ) -> Option<TransactionUnspentOutput> {
        self.parse_blockfrost_output(
            utxo.tx_hash,
            utxo.output_index as u64,
            utxo.address,
            utxo.amount,
            utxo.inline_datum,
            utxo.data_hash,
            utxo.reference_script_hash,
        )
        .await
    }

    async fn blockfrost_tx_utxo_to_tx_unspent_output(
        &self,
        output_ref: OutputRef,
        utxo: TxContentUtxoOutputsInner,
    ) -> Option<TransactionUnspentOutput> {
        self.parse_blockfrost_output(
            output_ref.tx_hash().to_hex(),
            output_ref.index(),
            utxo.address,
            utxo.amount,
            utxo.inline_datum,
            utxo.data_hash,
            utxo.reference_script_hash,
        )
        .await
    }
}

#[async_trait]
impl CardanoNetwork for Blockfrost {
    async fn utxo_by_ref(&self, oref: OutputRef) -> Option<TransactionUnspentOutput> {
        let transaction_outputs = self
            .0
            .transactions_utxos(oref.tx_hash().to_hex().as_str())
            .await
            .ok()?;

        if let Some(output) = transaction_outputs
            .outputs
            .into_iter()
            .find(|output| output.output_index as u64 == oref.index())
        {
            return self.blockfrost_tx_utxo_to_tx_unspent_output(oref, output).await;
        };

        None
    }

    async fn utxos_by_pay_cred(
        &self,
        payment_credential: PaymentCredential,
        offset: u32,
        limit: u16,
    ) -> Vec<TransactionUnspentOutput> {
        // blockfrost pagination starts from page 1 and we should increment quotient
        if let Some(page_size) = offset.checked_div(limit as u32).map(|page| page + 1) {
            let outputs = self
                .0
                .addresses_utxos(
                    String::from(payment_credential.clone()).as_str(),
                    Pagination {
                        fetch_all: false,
                        count: limit as usize,
                        page: page_size as usize,
                        order: Order::Asc,
                    },
                )
                .await
                .unwrap_or(vec![]);

            let parsed_outputs: Vec<_> = outputs
                .into_iter()
                .map(|output| async move { self.blockfrost_address_utxo_to_tx_unspent_output(output).await })
                .collect();

            return join_all(parsed_outputs).await.into_iter().flatten().collect();
        }

        vec![]
    }

    async fn utxos_by_address(
        &self,
        address: Address,
        offset: u32,
        limit: u16,
    ) -> Vec<TransactionUnspentOutput> {
        // blockfrost pagination starts from page 1 and we should increment quotient
        if let Some(page_size) = offset.checked_div(limit as u32).map(|page| page + 1) {
            let outputs = self
                .0
                .addresses_utxos(
                    String::from(address.to_bech32(None).unwrap().as_str()).as_str(),
                    Pagination {
                        fetch_all: false,
                        count: limit as usize,
                        page: page_size as usize,
                        order: Order::Asc,
                    },
                )
                .await
                .unwrap_or(vec![]);

            let parsed_outputs: Vec<_> = outputs
                .into_iter()
                .map(|output| async move { self.blockfrost_address_utxo_to_tx_unspent_output(output).await })
                .collect();

            return join_all(parsed_outputs).await.into_iter().flatten().collect();
        };

        vec![]
    }
}

pub struct Maestro(maestro::Maestro);

impl Maestro {
    pub async fn new<P: AsRef<Path>>(path: P, network: Network) -> Result<Self, Error> {
        let token = fs::read_to_string(path).await?.replace("\n", "");
        Ok(Self(maestro::Maestro::new(token, network.into())))
    }
}

#[async_trait]
impl CardanoNetwork for Maestro {
    async fn utxo_by_ref(&self, oref: OutputRef) -> Option<TransactionUnspentOutput> {
        let params = Some(HashMap::from([(
            "with_cbor".to_lowercase(),
            "true".to_lowercase(),
        )]));
        retry!(self
            .0
            .transaction_output_from_reference(
                oref.tx_hash().to_hex().as_str(),
                oref.index() as i32,
                params.clone()
            )
            .await
            .ok())
        .and_then(|tx_out| {
            let tx_out =
                TransactionOutput::from_cbor_bytes(&hex::decode(tx_out.data.tx_out_cbor?).ok()?).ok()?;
            Some(TransactionUnspentOutput::new(oref.into(), tx_out))
        })
    }

    async fn utxos_by_pay_cred(
        &self,
        payment_credential: PaymentCredential,
        offset: u32,
        limit: u16,
    ) -> Vec<TransactionUnspentOutput> {
        let mut params = Parameters::new();
        params.with_cbor();
        params.from(offset as i64);
        params.count(limit as i32);
        retry!(self
            .0
            .utxos_by_payment_credential(
                String::from(payment_credential.clone()).as_str(),
                Some(params.clone())
            )
            .await
            .ok())
        .and_then(|utxos| read_maestro_utxos(utxos).ok())
        .unwrap_or(vec![])
    }

    async fn utxos_by_address(
        &self,
        address: Address,
        offset: u32,
        limit: u16,
    ) -> Vec<TransactionUnspentOutput> {
        let mut params = Parameters::new();
        params.with_cbor();
        params.from(offset as i64);
        params.count(limit as i32);
        retry!(self
            .0
            .utxos_at_address(address.to_bech32(None).unwrap().as_str(), Some(params.clone()))
            .await
            .ok())
        .and_then(|utxos| read_maestro_utxos(utxos).ok())
        .unwrap_or(vec![])
    }
}

impl ExtendedCardanoNetwork for Maestro {
    async fn slot_indexed_utxos_by_address(
        &self,
        address: Address,
        offset: u32,
        limit: u16,
    ) -> Vec<UTxOInfo> {
        let utxos = self.utxos_by_address(address, offset, limit).await;
        let mut res = vec![];

        for utxo in utxos {
            let tx_details = self
                .0
                .transaction_details(&utxo.input.transaction_id.to_hex())
                .await
                .unwrap();
            let info = UTxOInfo {
                utxo,
                slot: tx_details.data.block_absolute_slot as u64,
                metadata_json: tx_details.data.metadata,
            };
            res.push(info);
        }
        res
    }

    async fn submit_tx(&self, cbor_bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let result = self.0.tx_manager_submit(cbor_bytes.to_vec()).await?;
        trace!("TX submit result: {}", result);
        Ok(())
    }

    async fn chain_tip_slot_number(&self) -> Result<u64, Box<dyn std::error::Error>> {
        let r = self.0.chain_tip().await?;
        Ok(r.last_updated.block_slot as u64)
    }

    async fn evaluate_tx(&self, cbor: &str) -> Result<Vec<RedeemerEvaluation>, Box<dyn std::error::Error>> {
        self.0.evaluate_tx(cbor, vec![]).await
    }

    async fn wait_for_transaction_confirmation(
        &self,
        tx_id: TransactionHash,
    ) -> Result<(), Box<dyn std::error::Error>> {
        while self
            .0
            .transaction_cbor(&tx_id.to_hex())
            .await
            .map(|_| ())
            .is_err()
        {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
        Ok(())
    }
}

pub struct UTxOInfo {
    pub utxo: TransactionUnspentOutput,
    pub slot: u64,
    /// Maestro-formatted JSON...
    pub metadata_json: serde_json::Value,
}

pub struct Koios {
    client: reqwest::Client,
    base_url: String,
}

#[derive(SerdeDeserialize)]
struct KoiosUtxo {
    tx_hash: String,
    tx_index: u64,
    address: String,
    value: String,
    #[serde(default)]
    datum_hash: Option<String>,
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

impl Koios {
    pub fn new(base_url: String) -> Self {
        Self {
            client: reqwest::Client::builder()
                .no_proxy()
                .build()
                .expect("Koios HTTP client configuration must be valid"),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    pub fn parse_utxo_json(row: serde_json::Value) -> Option<TransactionUnspentOutput> {
        Self::parse_output(serde_json::from_value(row).ok()?)
    }

    async fn post_utxos(&self, endpoint: &str, body: serde_json::Value) -> Vec<TransactionUnspentOutput> {
        let response = match self
            .client
            .post(format!("{}/{}", self.base_url, endpoint.trim_start_matches('/')))
            .json(&body)
            .send()
            .await
        {
            Ok(response) => match response.json::<Vec<KoiosUtxo>>().await {
                Ok(rows) => Some(rows),
                Err(error) => {
                    warn!("Koios {} response decode failed: {}", endpoint, error);
                    None
                }
            },
            Err(error) => {
                warn!("Koios {} request failed: {}", endpoint, error);
                None
            }
        };

        response
            .unwrap_or_default()
            .into_iter()
            .filter_map(Self::parse_output)
            .collect()
    }

    fn parse_output(utxo: KoiosUtxo) -> Option<TransactionUnspentOutput> {
        let mut value = Value::zero();
        value.add_unsafe(Native, utxo.value.parse::<u64>().ok()?);

        for asset in utxo.asset_list {
            let raw_token = format!("{}.{}", asset.policy_id, asset.asset_name);
            let token = RawToken::try_from_raw_string(raw_token.as_str())?;
            value.add_unsafe(Token(token), asset.quantity.parse::<u64>().ok()?);
        }

        let datum = utxo
            .inline_datum
            .and_then(|datum| {
                let bytes = hex::decode(datum.bytes).ok()?;
                PlutusData::from_cbor_bytes(bytes.as_slice())
                    .ok()
                    .map(DatumOption::new_datum)
            })
            .or_else(|| {
                utxo.datum_hash.and_then(|datum_hash| {
                    DatumHash::from_hex(datum_hash.as_str())
                        .ok()
                        .map(DatumOption::new_hash)
                })
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

        Some(TransactionUnspentOutput {
            input: TransactionInput::new(
                TransactionHash::from_hex(utxo.tx_hash.as_str()).ok()?,
                utxo.tx_index,
            ),
            output: TransactionOutput::new(
                Address::from_bech32(utxo.address.as_str()).ok()?,
                value,
                datum,
                script,
            ),
        })
    }
}

#[async_trait]
impl CardanoNetwork for Koios {
    async fn utxo_by_ref(&self, oref: OutputRef) -> Option<TransactionUnspentOutput> {
        let body = json!({
            "_utxo_refs": [format!("{}#{}", oref.tx_hash().to_hex(), oref.index())],
            "_extended": true
        });
        for attempt in 0..5 {
            let found = self.post_utxos("utxo_info", body.clone()).await.into_iter().next();
            if found.is_some() {
                return found;
            }
            warn!("Koios utxo_info did not return {}; retry {}/5", oref, attempt + 1);
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        None
    }

    async fn utxos_by_pay_cred(
        &self,
        _payment_credential: PaymentCredential,
        _offset: u32,
        _limit: u16,
    ) -> Vec<TransactionUnspentOutput> {
        vec![]
    }

    async fn utxos_by_address(
        &self,
        address: Address,
        offset: u32,
        limit: u16,
    ) -> Vec<TransactionUnspentOutput> {
        self.post_utxos(
            "address_utxos",
            json!({
                "_addresses": [address.to_bech32(None).unwrap()],
                "_extended": true
            }),
        )
        .await
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect()
    }
}

pub enum AnyExplorer {
    Blockfrost(Blockfrost),
    Maestro(Maestro),
    Koios(Koios),
}

impl AnyExplorer {
    pub async fn new(config: &ExplorerConfig, network_id: NetworkId) -> Result<Self, Error> {
        match config {
            ExplorerConfig::MaestroKeyPath(maestro_key_path) => {
                Maestro::new(maestro_key_path, network_id.into())
                    .await
                    .map(AnyExplorer::Maestro)
            }
            ExplorerConfig::BlockfrostKeyPath(blockfrost_key_path) => Blockfrost::new(blockfrost_key_path)
                .await
                .map(AnyExplorer::Blockfrost),
            ExplorerConfig::KoiosBaseUrl(koios_base_url) => {
                Ok(AnyExplorer::Koios(Koios::new(koios_base_url.clone())))
            }
        }
    }
}

#[async_trait]
impl CardanoNetwork for AnyExplorer {
    async fn utxo_by_ref(&self, oref: OutputRef) -> Option<TransactionUnspentOutput> {
        match self {
            AnyExplorer::Blockfrost(blockfrost) => blockfrost.utxo_by_ref(oref).await,
            AnyExplorer::Maestro(maestro) => maestro.utxo_by_ref(oref).await,
            AnyExplorer::Koios(koios) => koios.utxo_by_ref(oref).await,
        }
    }

    async fn utxos_by_pay_cred(
        &self,
        payment_credential: PaymentCredential,
        offset: u32,
        limit: u16,
    ) -> Vec<TransactionUnspentOutput> {
        match self {
            AnyExplorer::Blockfrost(blockfrost) => {
                blockfrost
                    .utxos_by_pay_cred(payment_credential, offset, limit)
                    .await
            }
            AnyExplorer::Maestro(maestro) => {
                maestro.utxos_by_pay_cred(payment_credential, offset, limit).await
            }
            AnyExplorer::Koios(koios) => koios.utxos_by_pay_cred(payment_credential, offset, limit).await,
        }
    }

    async fn utxos_by_address(
        &self,
        address: Address,
        offset: u32,
        limit: u16,
    ) -> Vec<TransactionUnspentOutput> {
        match self {
            AnyExplorer::Blockfrost(blockfrost) => blockfrost.utxos_by_address(address, offset, limit).await,
            AnyExplorer::Maestro(maestro) => maestro.utxos_by_address(address, offset, limit).await,
            AnyExplorer::Koios(koios) => koios.utxos_by_address(address, offset, limit).await,
        }
    }
}

fn read_maestro_utxos(
    resp: UtxosAtAddress,
) -> Result<Vec<TransactionUnspentOutput>, Box<dyn std::error::Error>> {
    let mut utxos = vec![];
    for utxo in resp.data {
        let tx_in = TransactionInput::new(
            TransactionHash::from_hex(utxo.tx_hash.as_str()).unwrap(),
            utxo.index as u64,
        );
        let tx_out = TransactionOutput::from_cbor_bytes(&*hex::decode(utxo.tx_out_cbor.unwrap())?)?;
        utxos.push(TransactionUnspentOutput::new(tx_in, tx_out));
    }
    Ok(utxos)
}
