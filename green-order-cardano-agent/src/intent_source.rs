use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use futures::channel::mpsc;
use futures::SinkExt;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpListener;

use bloom_offchain::execution_engine::bundled::Bundled;
use bloom_offchain_cardano::event_sink::handler::LedgerCx;
use bloom_offchain_cardano::orders::green::{
    AccountId, AlephAccountUtxo, GreenAuth, GreenIntention, GreenOrder, GreenOrderId, ALEPH_ACCOUNT_VALIDATOR,
};
use spectrum_cardano_lib::output::FinalizedTxOut;
use spectrum_cardano_lib::transaction::TransactionOutputExtension;
use spectrum_cardano_lib::value::ValueExtension;
use spectrum_cardano_lib::AssetClass;
use spectrum_offchain::data::ior::Ior;
use spectrum_offchain::domain::event::{Channel, Transition};
use spectrum_offchain::domain::{Baked, Has, Tradable};
use spectrum_offchain::partitioning::Partitioned;
use spectrum_offchain_cardano::data::pair::PairId;
use spectrum_offchain_cardano::deployment::DeployedScriptInfo;

use crate::account_index::AccountIndex;
use crate::entity::EvolvingCardanoEntity;

pub type GreenIntentEvent = (PairId, Channel<Transition<EvolvingCardanoEntity>, LedgerCx>);

#[derive(Debug, Clone)]
pub struct RawGreenIntent {
    pub account_id: AccountId,
    pub original_intent_digest: [u8; 32],
    pub intention: GreenIntention,
    pub auth: GreenAuth,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct GreenOrdersConfig {
    pub allow_partial: bool,
    pub intent_source: IntentSourceConfig,
}

impl Default for GreenOrdersConfig {
    fn default() -> Self {
        Self {
            allow_partial: false,
            intent_source: IntentSourceConfig::default(),
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
pub struct IntentSourceConfig {
    pub listen_addr: Option<SocketAddr>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum AdmissionError {
    PartialNotAllowed,
    MissingAccount,
    AccountParseFailed,
    StaleNonce,
    NonAdaLeavingAsset,
    InsufficientAccountBalance,
}

pub fn admit_green_intent<C>(
    raw: RawGreenIntent,
    index: &AccountIndex,
    ctx: &C,
    config: GreenOrdersConfig,
) -> Result<Bundled<GreenOrder, FinalizedTxOut>, AdmissionError>
where
    C: Has<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>>,
{
    if matches!(raw.auth, GreenAuth::Path { .. }) && !config.allow_partial {
        return Err(AdmissionError::PartialNotAllowed);
    }
    if raw.intention.input_asset != AssetClass::Native {
        return Err(AdmissionError::NonAdaLeavingAsset);
    }

    let account_bearer = index
        .current(raw.account_id)
        .ok_or(AdmissionError::MissingAccount)?;
    let account = AlephAccountUtxo::try_parse(account_bearer.reference(), account_bearer.0.clone(), ctx)
        .ok_or(AdmissionError::AccountParseFailed)?;
    let current_nonce = account
        .state
        .nonce
        .get(raw.intention.target_nonce_slot as usize)
        .ok_or(AdmissionError::StaleNonce)?;
    if *current_nonce > raw.intention.target_nonce_value as i64 {
        return Err(AdmissionError::StaleNonce);
    }

    let required_lovelace = raw
        .intention
        .leaving_amount
        .checked_add(raw.intention.fee_lovelace)
        .ok_or(AdmissionError::InsufficientAccountBalance)?;
    let available_lovelace = account.output.value().amount_of(AssetClass::Native).unwrap_or(0);
    if available_lovelace < required_lovelace {
        return Err(AdmissionError::InsufficientAccountBalance);
    }

    let order = GreenOrder {
        id: GreenOrderId::new(
            raw.account_id,
            raw.intention.target_nonce_slot,
            raw.intention.target_nonce_value,
            raw.original_intent_digest,
        ),
        account_id: raw.account_id,
        current_remainder: raw.intention.leaving_amount,
        accumulated_output: 0,
        intention: raw.intention,
        auth: raw.auth,
    };

    Ok(Bundled(order, account.finalized_output()))
}

pub fn admitted_intent_to_event(admitted: Bundled<GreenOrder, FinalizedTxOut>) -> GreenIntentEvent {
    let pair = admitted.0.pair_id();
    let version = admitted.1.reference();
    let entity = EvolvingCardanoEntity(Bundled(
        either::Either::Left(Baked::new(admitted.0, version)),
        admitted.1,
    ));
    (
        pair,
        Channel::local_tx_submit(Transition::Forward(Ior::Right(entity))),
    )
}

pub async fn run_tcp_intent_source<C>(
    listen_addr: SocketAddr,
    account_index: Arc<Mutex<AccountIndex>>,
    ctx: C,
    config: GreenOrdersConfig,
    events: Partitioned<4, PairId, mpsc::Sender<GreenIntentEvent>>,
) where
    C: Has<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>> + Clone + Send + Sync + 'static,
{
    let listener = TcpListener::bind(listen_addr)
        .await
        .expect("failed to bind green intent source");
    loop {
        let (socket, _) = listener
            .accept()
            .await
            .expect("failed to accept green intent connection");
        let account_index = Arc::clone(&account_index);
        let ctx = ctx.clone();
        let config = config;
        let mut partitioned_events = events.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(socket).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(wire) = serde_json::from_str::<WireGreenIntent>(&line) else {
                    continue;
                };
                let Ok(raw) = RawGreenIntent::try_from(wire) else {
                    continue;
                };
                let admitted = {
                    let index = account_index.lock().expect("account index lock poisoned");
                    admit_green_intent(raw, &index, &ctx, config)
                };
                let Ok(admitted) = admitted else {
                    continue;
                };
                let event = admitted_intent_to_event(admitted);
                let pair = event.0;
                let _ = partitioned_events.get_mut(pair).send(event).await;
            }
        });
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireGreenIntent {
    account_id: String,
    original_intent_digest: String,
    input_asset: AssetClass,
    output_asset: AssetClass,
    leaving_amount: u64,
    expected_arriving_amount: u64,
    fee_lovelace: u64,
    target_nonce_slot: u16,
    target_nonce_value: u64,
    operator_key_hash: String,
    auth: WireGreenAuth,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum WireGreenAuth {
    Sig {
        prefix: String,
        postfix: String,
        signature: String,
        #[serde(default)]
        update_proof: String,
    },
    Path {
        proof: String,
    },
}

impl TryFrom<WireGreenIntent> for RawGreenIntent {
    type Error = ();

    fn try_from(value: WireGreenIntent) -> Result<Self, Self::Error> {
        let account_id_bytes = hex::decode(value.account_id).map_err(|_| ())?;
        let account_id = AccountId::try_from_slice(&account_id_bytes).map_err(|_| ())?;
        let original_intent_digest = <[u8; 32]>::try_from(
            hex::decode(value.original_intent_digest)
                .map_err(|_| ())?
                .as_slice(),
        )
        .map_err(|_| ())?;
        let operator_key_hash =
            GreenIntention::try_operator_key_hash(hex::decode(value.operator_key_hash).map_err(|_| ())?)
                .map_err(|_| ())?;
        let auth = match value.auth {
            WireGreenAuth::Sig {
                prefix,
                postfix,
                signature,
                update_proof,
            } => GreenAuth::new_sig(
                hex::decode(prefix).map_err(|_| ())?,
                hex::decode(postfix).map_err(|_| ())?,
                hex::decode(signature).map_err(|_| ())?,
                hex::decode(update_proof).map_err(|_| ())?,
            )
            .map_err(|_| ())?,
            WireGreenAuth::Path { proof } => GreenAuth::Path {
                proof: hex::decode(proof).map_err(|_| ())?,
            },
        };
        Ok(Self {
            account_id,
            original_intent_digest,
            intention: GreenIntention {
                input_asset: value.input_asset,
                output_asset: value.output_asset,
                leaving_amount: value.leaving_amount,
                expected_arriving_amount: value.expected_arriving_amount,
                fee_lovelace: value.fee_lovelace,
                target_nonce_slot: value.target_nonce_slot,
                target_nonce_value: value.target_nonce_value,
                operator_key_hash,
            },
            auth,
        })
    }
}

impl<'de> Deserialize<'de> for IntentSourceConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Repr {
            listen_addr: Option<SocketAddr>,
        }
        let repr = Repr::deserialize(deserializer)?;
        Ok(Self {
            listen_addr: repr.listen_addr,
        })
    }
}

impl<'de> Deserialize<'de> for GreenOrdersConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Repr {
            #[serde(default)]
            allow_partial: bool,
            #[serde(default)]
            intent_source: IntentSourceConfig,
        }
        let repr = Repr::deserialize(deserializer)?;
        Ok(Self {
            allow_partial: repr.allow_partial,
            intent_source: repr.intent_source,
        })
    }
}

#[cfg(test)]
mod tests {
    use bloom_offchain_cardano::orders::green::{AlephAccountState, ALEPH_ACCOUNT_VALIDATOR};
    use cml_chain::address::{Address, EnterpriseAddress};
    use cml_chain::assets::AssetBundle;
    use cml_chain::certs::Credential;
    use cml_chain::transaction::{ConwayFormatTxOut, DatumOption, TransactionOutput};
    use cml_chain::Value;
    use cml_crypto::{ScriptHash, TransactionHash};
    use spectrum_cardano_lib::ex_units::ExUnits;
    use spectrum_cardano_lib::plutus_data::IntoPlutusData;
    use spectrum_cardano_lib::{AssetClass, AssetName, OutputRef, Token};
    use spectrum_offchain::domain::Has;
    use spectrum_offchain_cardano::deployment::DeployedScriptInfo;
    use type_equalities::IsEqual;

    use super::*;

    #[derive(Copy, Clone)]
    struct AccountCtx {
        account: DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>,
    }

    impl Has<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>> for AccountCtx {
        fn select<U: IsEqual<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>>>(
            &self,
        ) -> DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }> {
            self.account
        }
    }

    fn account_id(byte: u8) -> AccountId {
        AccountId::try_from_slice(&[byte; 32]).unwrap()
    }

    fn token(seed: u8) -> Token {
        Token(
            cml_chain::PolicyId::from([seed; 28]),
            AssetName::from_utf8(format!("t{seed}")),
        )
    }

    fn account_state(nonce: i64) -> AlephAccountState {
        AlephAccountState {
            magic: b"green-test".to_vec(),
            allowlist: vec![],
            nonce: vec![nonce],
            main_key: vec![2; 33],
            co_key: None,
            cold_key_hash: [13; 28],
            store_root: [14; 32],
        }
    }

    fn account_output(script_hash: ScriptHash, lovelace: u64, nonce: i64) -> TransactionOutput {
        TransactionOutput::new_conway_format_tx_out(ConwayFormatTxOut {
            address: Address::Enterprise(EnterpriseAddress::new(0, Credential::new_script(script_hash))),
            amount: Value::new(lovelace, AssetBundle::new()),
            datum_option: Some(DatumOption::Datum {
                datum: account_state(nonce).into_pd(),
                len_encoding: Default::default(),
                tag_encoding: None,
                datum_tag_encoding: None,
                datum_bytes_encoding: Default::default(),
            }),
            script_reference: None,
            encodings: None,
        })
    }

    fn ctx() -> AccountCtx {
        AccountCtx {
            account: DeployedScriptInfo {
                script_hash: ScriptHash::from([9; 28]),
                marginal_cost: ExUnits { mem: 0, steps: 0 },
            },
        }
    }

    fn indexed_account(id: AccountId, lovelace: u64, nonce: i64) -> (AccountIndex, AccountCtx) {
        let ctx = ctx();
        let out_ref = OutputRef::new(TransactionHash::from([1; 32]), 0);
        let output = account_output(ctx.account.script_hash, lovelace, nonce);
        let account = AlephAccountUtxo::try_parse(out_ref, output, &ctx).unwrap();
        let mut index = AccountIndex::default();
        index.observe_created_or_updated(account);
        index.bind_external_account_id(id, out_ref).unwrap();
        (index, ctx)
    }

    fn raw_intent(account_id: AccountId) -> RawGreenIntent {
        RawGreenIntent {
            account_id,
            original_intent_digest: [8; 32],
            intention: GreenIntention {
                input_asset: AssetClass::Native,
                output_asset: AssetClass::Token(token(1)),
                leaving_amount: 1_000_000,
                expected_arriving_amount: 900_000,
                fee_lovelace: 100_000,
                target_nonce_slot: 0,
                target_nonce_value: 10,
                operator_key_hash: [7; 28],
            },
            auth: GreenAuth::new_sig(vec![], vec![], vec![3; 64], vec![]).unwrap(),
        }
    }

    #[test]
    fn rejects_path_auth_when_partial_disabled() {
        let id = account_id(1);
        let (index, ctx) = indexed_account(id, 2_000_000, 0);
        let mut raw = raw_intent(id);
        raw.auth = GreenAuth::Path { proof: vec![1] };

        let err = admit_green_intent(raw, &index, &ctx, GreenOrdersConfig::default()).unwrap_err();

        assert_eq!(AdmissionError::PartialNotAllowed, err);
    }

    #[test]
    fn rejects_missing_account() {
        let ctx = ctx();
        let index = AccountIndex::default();
        let raw = raw_intent(account_id(2));

        let err = admit_green_intent(raw, &index, &ctx, GreenOrdersConfig::default()).unwrap_err();

        assert_eq!(AdmissionError::MissingAccount, err);
    }

    #[test]
    fn rejects_stale_nonce() {
        let id = account_id(3);
        let (index, ctx) = indexed_account(id, 2_000_000, 11);
        let raw = raw_intent(id);

        let err = admit_green_intent(raw, &index, &ctx, GreenOrdersConfig::default()).unwrap_err();

        assert_eq!(AdmissionError::StaleNonce, err);
    }

    #[test]
    fn rejects_non_ada_leaving_asset() {
        let id = account_id(4);
        let (index, ctx) = indexed_account(id, 2_000_000, 0);
        let mut raw = raw_intent(id);
        raw.intention.input_asset = AssetClass::Token(token(2));

        let err = admit_green_intent(raw, &index, &ctx, GreenOrdersConfig::default()).unwrap_err();

        assert_eq!(AdmissionError::NonAdaLeavingAsset, err);
    }

    #[test]
    fn rejects_account_without_full_fill_balance() {
        let id = account_id(5);
        let (index, ctx) = indexed_account(id, 500_000, 0);
        let raw = raw_intent(id);

        let err = admit_green_intent(raw, &index, &ctx, GreenOrdersConfig::default()).unwrap_err();

        assert_eq!(AdmissionError::InsufficientAccountBalance, err);
    }

    #[test]
    fn injects_accepted_intent_as_green_taker_event() {
        let id = account_id(6);
        let (index, ctx) = indexed_account(id, 2_000_000, 0);
        let raw = raw_intent(id);

        let admitted = admit_green_intent(raw, &index, &ctx, GreenOrdersConfig::default()).unwrap();
        let (pair, event) = admitted_intent_to_event(admitted);

        assert!(matches!(
            event,
            Channel::LocalTxSubmit(spectrum_offchain::domain::event::Predicted(Transition::Forward(
                Ior::Right(_)
            )))
        ));
        assert_eq!(
            pair,
            PairId::canonical(AssetClass::Native, AssetClass::Token(token(1)))
        );
    }
}
