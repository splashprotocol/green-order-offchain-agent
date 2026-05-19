use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use futures::channel::mpsc;
use serde::Serialize;
use spectrum_offchain::domain::Has;
use spectrum_offchain::partitioning::Partitioned;
use spectrum_offchain_cardano::data::pair::PairId;
use spectrum_offchain_cardano::deployment::DeployedScriptInfo;

use bloom_offchain_cardano::orders::green::ALEPH_ACCOUNT_VALIDATOR;

use crate::account_index::AccountIndex;
use crate::intent_source::{
    admit_green_intent, admitted_intent_to_event, parse_wire_intent, AdmissionError, GreenIntentEvent,
    GreenOrdersConfig, WireGreenIntent,
};

#[derive(Clone)]
pub struct HttpIntentState<C> {
    pub account_index: Arc<Mutex<AccountIndex>>,
    pub ctx: C,
    pub config: GreenOrdersConfig,
    pub events: Partitioned<4, PairId, mpsc::Sender<GreenIntentEvent>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentResponse {
    pub status: &'static str,
    pub reason: Option<&'static str>,
}

pub fn router<C>(state: HttpIntentState<C>) -> Router
where
    C: Has<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>> + Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/intents", post(post_intent::<C>))
        .with_state(state)
}

async fn post_intent<C>(
    State(mut state): State<HttpIntentState<C>>,
    Json(wire): Json<WireGreenIntent>,
) -> (StatusCode, Json<IntentResponse>)
where
    C: Has<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>> + Clone + Send + Sync + 'static,
{
    let Ok(raw) = parse_wire_intent(wire) else {
        return rejected(StatusCode::BAD_REQUEST, "malformedIntent");
    };

    let admitted = {
        let index = state.account_index.lock().expect("account index lock poisoned");
        admit_green_intent(raw, &index, &state.ctx, state.config)
    };
    let admitted = match admitted {
        Ok(admitted) => admitted,
        Err(err) => return rejected(status_for_admission_error(err), reason_for_admission_error(err)),
    };

    let event = admitted_intent_to_event(admitted);
    let pair = event.0;
    if state.events.get_mut(pair).try_send(event).is_err() {
        return rejected(StatusCode::SERVICE_UNAVAILABLE, "eventChannelUnavailable");
    }

    (
        StatusCode::ACCEPTED,
        Json(IntentResponse {
            status: "accepted",
            reason: None,
        }),
    )
}

fn rejected(status: StatusCode, reason: &'static str) -> (StatusCode, Json<IntentResponse>) {
    (
        status,
        Json(IntentResponse {
            status: "rejected",
            reason: Some(reason),
        }),
    )
}

fn status_for_admission_error(err: AdmissionError) -> StatusCode {
    match err {
        AdmissionError::MissingAccount | AdmissionError::StaleNonce | AdmissionError::AccountParseFailed => {
            StatusCode::CONFLICT
        }
        AdmissionError::PartialNotAllowed
        | AdmissionError::NonAdaLeavingAsset
        | AdmissionError::InsufficientAccountBalance => StatusCode::UNPROCESSABLE_ENTITY,
    }
}

fn reason_for_admission_error(err: AdmissionError) -> &'static str {
    match err {
        AdmissionError::PartialNotAllowed => "partialNotAllowed",
        AdmissionError::MissingAccount => "missingAccount",
        AdmissionError::AccountParseFailed => "accountParseFailed",
        AdmissionError::StaleNonce => "staleNonce",
        AdmissionError::NonAdaLeavingAsset => "nonAdaLeavingAsset",
        AdmissionError::InsufficientAccountBalance => "insufficientAccountBalance",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::Json;
    use bloom_offchain_cardano::orders::green::{
        AccountId, AlephAccountState, AlephAccountUtxo, ALEPH_ACCOUNT_VALIDATOR,
    };
    use cml_chain::address::{Address, EnterpriseAddress};
    use cml_chain::assets::AssetBundle;
    use cml_chain::certs::Credential;
    use cml_chain::transaction::{ConwayFormatTxOut, DatumOption, TransactionOutput};
    use cml_chain::Value;
    use cml_crypto::{ScriptHash, TransactionHash};
    use futures::channel::mpsc;
    use futures::FutureExt;
    use futures::StreamExt;
    use spectrum_cardano_lib::ex_units::ExUnits;
    use spectrum_cardano_lib::plutus_data::IntoPlutusData;
    use spectrum_cardano_lib::{AssetClass, AssetName, OutputRef, Token};
    use spectrum_offchain::domain::Has;
    use spectrum_offchain::partitioning::Partitioned;
    use spectrum_offchain_cardano::deployment::DeployedScriptInfo;
    use type_equalities::IsEqual;

    use crate::account_index::AccountIndex;
    use crate::intent_source::{GreenIntentEvent, GreenOrdersConfig, WireGreenIntent};

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
            store_root: [0; 32],
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

    fn wire_intent(account_id: AccountId) -> WireGreenIntent {
        serde_json::from_value(serde_json::json!({
            "accountId": hex::encode(account_id.bytes()),
            "originalIntentDigest": hex::encode([2u8; 32]),
            "inputAsset": "00",
            "outputAsset": hex::encode(AssetClass::Token(token(1)).to_bytes()),
            "leavingAmount": 1_000_000,
            "expectedArrivingAmount": 900_000,
            "feeLovelace": 100_000,
            "targetNonceSlot": 0,
            "targetNonceValue": 10,
            "operatorKeyHash": hex::encode([7u8; 28]),
            "auth": {
                "type": "sig",
                "prefix": "",
                "postfix": "",
                "signature": hex::encode([3u8; 64]),
                "updateProof": ""
            }
        }))
        .unwrap()
    }

    fn path_auth_wire_intent(account_id: AccountId) -> WireGreenIntent {
        serde_json::from_value(serde_json::json!({
            "accountId": hex::encode(account_id.bytes()),
            "originalIntentDigest": hex::encode([2u8; 32]),
            "inputAsset": "00",
            "outputAsset": hex::encode(AssetClass::Token(token(1)).to_bytes()),
            "leavingAmount": 1_000_000,
            "expectedArrivingAmount": 900_000,
            "feeLovelace": 100_000,
            "targetNonceSlot": 0,
            "targetNonceValue": 10,
            "operatorKeyHash": hex::encode([7u8; 28]),
            "auth": {
                "type": "path",
                "proof": "01"
            }
        }))
        .unwrap()
    }

    fn event_channels(
        capacity: usize,
    ) -> (
        Partitioned<4, spectrum_offchain_cardano::data::pair::PairId, mpsc::Sender<GreenIntentEvent>>,
        Vec<mpsc::Receiver<GreenIntentEvent>>,
    ) {
        let (snd_1, recv_1) = mpsc::channel(capacity);
        let (snd_2, recv_2) = mpsc::channel(capacity);
        let (snd_3, recv_3) = mpsc::channel(capacity);
        let (snd_4, recv_4) = mpsc::channel(capacity);
        (
            Partitioned::new([snd_1, snd_2, snd_3, snd_4]),
            vec![recv_1, recv_2, recv_3, recv_4],
        )
    }

    fn any_event_ready(receivers: &mut [mpsc::Receiver<GreenIntentEvent>]) -> bool {
        receivers
            .iter_mut()
            .any(|recv| matches!(recv.next().now_or_never(), Some(Some(_))))
    }

    #[tokio::test]
    async fn post_intent_accepts_valid_full_fill_intent() {
        let id = account_id(1);
        let (index, ctx) = indexed_account(id, 2_000_000, 0);
        let (events, mut receivers) = event_channels(4);
        let state = HttpIntentState {
            account_index: Arc::new(Mutex::new(index)),
            ctx,
            config: GreenOrdersConfig::default(),
            events,
        };

        let (status, _) = post_intent(State(state), Json(wire_intent(id))).await;

        assert_eq!(StatusCode::ACCEPTED, status);
        assert!(any_event_ready(&mut receivers));
    }

    #[tokio::test]
    async fn post_intent_rejects_missing_account() {
        let (events, mut receivers) = event_channels(4);
        let state = HttpIntentState {
            account_index: Arc::new(Mutex::new(AccountIndex::default())),
            ctx: ctx(),
            config: GreenOrdersConfig::default(),
            events,
        };

        let (status, _) = post_intent(State(state), Json(wire_intent(account_id(2)))).await;

        assert_eq!(StatusCode::CONFLICT, status);
        assert!(!any_event_ready(&mut receivers));
    }

    #[tokio::test]
    async fn post_intent_rejects_path_auth_when_partial_disabled() {
        let id = account_id(3);
        let (index, ctx) = indexed_account(id, 2_000_000, 0);
        let (events, mut receivers) = event_channels(4);
        let state = HttpIntentState {
            account_index: Arc::new(Mutex::new(index)),
            ctx,
            config: GreenOrdersConfig::default(),
            events,
        };

        let (status, _) = post_intent(State(state), Json(path_auth_wire_intent(id))).await;

        assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status);
        assert!(!any_event_ready(&mut receivers));
    }

    #[tokio::test]
    async fn post_intent_returns_503_when_event_channel_is_unavailable() {
        let id = account_id(4);
        let (index, ctx) = indexed_account(id, 2_000_000, 0);
        let (events, receivers) = event_channels(1);
        drop(receivers);
        let state = HttpIntentState {
            account_index: Arc::new(Mutex::new(index)),
            ctx,
            config: GreenOrdersConfig::default(),
            events,
        };

        let (status, _) = post_intent(State(state), Json(wire_intent(id))).await;

        assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status);
    }
}
