use std::sync::{Arc, Mutex};

use axum::extract::Query;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use cml_crypto::RawBytesEncoding;
use futures::channel::mpsc;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use spectrum_offchain::domain::Has;
use spectrum_offchain::partitioning::Partitioned;
use spectrum_offchain_cardano::data::pair::PairId;
use spectrum_offchain_cardano::deployment::DeployedScriptInfo;

use bloom_offchain_cardano::orders::green::{AccountId, ALEPH_ACCOUNT_VALIDATOR};

use crate::account_index::{AccountBindingError, AccountIndex};
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
        .route("/accounts/bind", post(post_bind_account::<C>))
        .route("/accounts/status", get(get_account_status::<C>))
        .with_state(state)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BindAccountRequest {
    account_id: String,
    tx_hash: String,
    output_index: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BindAccountResponse {
    pub status: &'static str,
    pub reason: Option<&'static str>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountStatusQuery {
    account_id: String,
    tx_hash: String,
    output_index: u64,
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
    info!("Accepted green intent for pair {}", pair);
    if state.events.get_mut(pair).try_send(event).is_err() {
        warn!("Green intent event channel is unavailable for pair {}", pair);
        return rejected(StatusCode::SERVICE_UNAVAILABLE, "eventChannelUnavailable");
    }
    info!("Queued green intent for pair {}", pair);

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

async fn post_bind_account<C>(
    State(state): State<HttpIntentState<C>>,
    Json(req): Json<BindAccountRequest>,
) -> (StatusCode, Json<BindAccountResponse>)
where
    C: Has<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>> + Clone + Send + Sync + 'static,
{
    let Ok((account_id, output_ref)) = parse_bind_account_request(req) else {
        return bind_rejected(StatusCode::BAD_REQUEST, "malformedBinding");
    };
    let result = state
        .account_index
        .lock()
        .expect("account index lock poisoned")
        .bind_external_account_id_once(account_id, output_ref);
    match result {
        Ok(_) => (
            StatusCode::OK,
            Json(BindAccountResponse {
                status: "bound",
                reason: None,
            }),
        ),
        Err(err) => bind_rejected(status_for_binding_error(err), reason_for_binding_error(err)),
    }
}

async fn get_account_status<C>(
    State(state): State<HttpIntentState<C>>,
    Query(req): Query<AccountStatusQuery>,
) -> (StatusCode, Json<serde_json::Value>)
where
    C: Has<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>> + Clone + Send + Sync + 'static,
{
    let Ok((account_id, output_ref)) = parse_account_status_query(req) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"status": "rejected", "reason": "malformedStatusQuery"})),
        );
    };
    let status = state
        .account_index
        .lock()
        .expect("account index lock poisoned")
        .account_status(account_id, output_ref);
    (StatusCode::OK, Json(serde_json::json!({"status": status})))
}

fn parse_account_status_query(
    req: AccountStatusQuery,
) -> Result<(AccountId, spectrum_cardano_lib::OutputRef), ()> {
    parse_account_ref(req.account_id, req.tx_hash, req.output_index)
}

fn parse_bind_account_request(
    req: BindAccountRequest,
) -> Result<(AccountId, spectrum_cardano_lib::OutputRef), ()> {
    parse_account_ref(req.account_id, req.tx_hash, req.output_index)
}

fn parse_account_ref(
    account_id: String,
    tx_hash: String,
    output_index: u64,
) -> Result<(AccountId, spectrum_cardano_lib::OutputRef), ()> {
    let account_id_bytes = hex::decode(account_id).map_err(|_| ())?;
    let account_id = AccountId::try_from_slice(&account_id_bytes).map_err(|_| ())?;
    let tx_hash_bytes = hex::decode(tx_hash).map_err(|_| ())?;
    let tx_hash = cml_crypto::TransactionHash::from_raw_bytes(&tx_hash_bytes).map_err(|_| ())?;
    Ok((
        account_id,
        spectrum_cardano_lib::OutputRef::new(tx_hash, output_index),
    ))
}

fn bind_rejected(status: StatusCode, reason: &'static str) -> (StatusCode, Json<BindAccountResponse>) {
    (
        status,
        Json(BindAccountResponse {
            status: "rejected",
            reason: Some(reason),
        }),
    )
}

fn status_for_binding_error(err: AccountBindingError) -> StatusCode {
    match err {
        AccountBindingError::AlreadyBound | AccountBindingError::OutputAlreadyBound => StatusCode::CONFLICT,
        AccountBindingError::OutputNotObserved => StatusCode::NOT_FOUND,
        AccountBindingError::NonEmptyUnplannedRoot => StatusCode::UNPROCESSABLE_ENTITY,
    }
}

fn reason_for_binding_error(err: AccountBindingError) -> &'static str {
    match err {
        AccountBindingError::AlreadyBound => "alreadyBound",
        AccountBindingError::OutputAlreadyBound => "outputAlreadyBound",
        AccountBindingError::OutputNotObserved => "outputNotObserved",
        AccountBindingError::NonEmptyUnplannedRoot => "nonEmptyUnplannedRoot",
    }
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
        AccountId, AlephAccountAbi, AlephAccountState, AlephAccountUtxo, ALEPH_ACCOUNT_VALIDATOR,
    };
    use cml_chain::address::{Address, EnterpriseAddress};
    use cml_chain::assets::AssetBundle;
    use cml_chain::certs::Credential;
    use cml_chain::transaction::{ConwayFormatTxOut, DatumOption, TransactionOutput};
    use cml_chain::Value;
    use cml_crypto::{RawBytesEncoding, ScriptHash, TransactionHash};
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

    fn account_state_with_store_root(nonce: i64, store_root: [u8; 32]) -> AlephAccountState {
        AlephAccountState {
            abi: AlephAccountAbi::Current,
            magic: b"green-test".to_vec(),
            allowlist: vec![],
            nonce: vec![nonce],
            main_key: vec![2; 33],
            co_key: None,
            cold_key_hash: [13; 28],
            store_root,
        }
    }

    fn account_output_with_store_root(
        script_hash: ScriptHash,
        lovelace: u64,
        nonce: i64,
        store_root: [u8; 32],
    ) -> TransactionOutput {
        TransactionOutput::new_conway_format_tx_out(ConwayFormatTxOut {
            address: Address::Enterprise(EnterpriseAddress::new(0, Credential::new_script(script_hash))),
            amount: Value::new(lovelace, AssetBundle::new()),
            datum_option: Some(DatumOption::Datum {
                datum: account_state_with_store_root(nonce, store_root).into_pd(),
                len_encoding: Default::default(),
                tag_encoding: None,
                datum_tag_encoding: None,
                datum_bytes_encoding: Default::default(),
            }),
            script_reference: None,
            encodings: None,
        })
    }

    fn account_output(script_hash: ScriptHash, lovelace: u64, nonce: i64) -> TransactionOutput {
        account_output_with_store_root(script_hash, lovelace, nonce, [0; 32])
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

    fn indexed_unbound_account(
        _id: AccountId,
        lovelace: u64,
        nonce: i64,
    ) -> (AccountIndex, AccountCtx, OutputRef) {
        let ctx = ctx();
        let out_ref = OutputRef::new(TransactionHash::from([31; 32]), 0);
        let output = account_output(ctx.account.script_hash, lovelace, nonce);
        let account = AlephAccountUtxo::try_parse(out_ref, output, &ctx).unwrap();
        let mut index = AccountIndex::default();
        index.observe_created_or_updated(account);
        (index, ctx, out_ref)
    }

    fn test_state(index: AccountIndex, ctx: AccountCtx) -> HttpIntentState<AccountCtx> {
        let (events, _) = event_channels(4);
        HttpIntentState {
            account_index: Arc::new(Mutex::new(index)),
            ctx,
            config: GreenOrdersConfig::default(),
            events,
        }
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

    #[tokio::test]
    async fn post_bind_account_binds_observed_empty_root_account() {
        let id = account_id(31);
        let (index, ctx, out_ref) = indexed_unbound_account(id, 2_000_000, 0);
        let state = test_state(index, ctx);

        let response = post_bind_account(
            State(state.clone()),
            Json(BindAccountRequest {
                account_id: hex::encode(id.bytes()),
                tx_hash: hex::encode(out_ref.tx_hash().to_raw_bytes()),
                output_index: out_ref.index(),
            }),
        )
        .await;

        assert_eq!(response.0, StatusCode::OK);
        assert_eq!(response.1 .0.status, "bound");
    }

    #[tokio::test]
    async fn post_bind_account_rejects_second_bind() {
        let id = account_id(32);
        let (mut index, ctx, out_ref) = indexed_unbound_account(id, 2_000_000, 0);
        index.bind_external_account_id_once(id, out_ref).unwrap();
        let state = test_state(index, ctx);

        let response = post_bind_account(
            State(state),
            Json(BindAccountRequest {
                account_id: hex::encode(id.bytes()),
                tx_hash: hex::encode(out_ref.tx_hash().to_raw_bytes()),
                output_index: out_ref.index(),
            }),
        )
        .await;

        assert_eq!(response.0, StatusCode::CONFLICT);
        assert_eq!(response.1 .0.reason, Some("alreadyBound"));
    }

    #[tokio::test]
    async fn post_bind_account_rejects_malformed_account_id() {
        let id = account_id(34);
        let (index, ctx, out_ref) = indexed_unbound_account(id, 2_000_000, 0);
        let response = post_bind_account(
            State(test_state(index, ctx)),
            Json(BindAccountRequest {
                account_id: "not-hex".to_string(),
                tx_hash: hex::encode(out_ref.tx_hash().to_raw_bytes()),
                output_index: out_ref.index(),
            }),
        )
        .await;
        assert_eq!(response.0, StatusCode::BAD_REQUEST);
        assert_eq!(response.1 .0.reason, Some("malformedBinding"));
    }

    #[tokio::test]
    async fn post_bind_account_rejects_malformed_tx_hash() {
        let id = account_id(35);
        let (index, ctx, out_ref) = indexed_unbound_account(id, 2_000_000, 0);
        let response = post_bind_account(
            State(test_state(index, ctx)),
            Json(BindAccountRequest {
                account_id: hex::encode(id.bytes()),
                tx_hash: "abcd".to_string(),
                output_index: out_ref.index(),
            }),
        )
        .await;
        assert_eq!(response.0, StatusCode::BAD_REQUEST);
        assert_eq!(response.1 .0.reason, Some("malformedBinding"));
    }

    #[tokio::test]
    async fn post_bind_account_rejects_unknown_output() {
        let id = account_id(36);
        let state = test_state(AccountIndex::default(), ctx());
        let response = post_bind_account(
            State(state),
            Json(BindAccountRequest {
                account_id: hex::encode(id.bytes()),
                tx_hash: hex::encode([36u8; 32]),
                output_index: 0,
            }),
        )
        .await;
        assert_eq!(response.0, StatusCode::NOT_FOUND);
        assert_eq!(response.1 .0.reason, Some("outputNotObserved"));
    }

    #[tokio::test]
    async fn post_bind_account_rejects_non_empty_unplanned_root() {
        let id = account_id(37);
        let ctx = ctx();
        let out_ref = OutputRef::new(TransactionHash::from([37; 32]), 0);
        let output = account_output_with_store_root(ctx.account.script_hash, 2_000_000, 0, [9; 32]);
        let account = AlephAccountUtxo::try_parse(out_ref, output, &ctx).unwrap();
        let mut index = AccountIndex::default();
        index.observe_created_or_updated(account);

        let response = post_bind_account(
            State(test_state(index, ctx)),
            Json(BindAccountRequest {
                account_id: hex::encode(id.bytes()),
                tx_hash: hex::encode(out_ref.tx_hash().to_raw_bytes()),
                output_index: out_ref.index(),
            }),
        )
        .await;
        assert_eq!(response.0, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(response.1 .0.reason, Some("nonEmptyUnplannedRoot"));
    }

    #[tokio::test]
    async fn bound_account_can_accept_intent_after_single_binding() {
        let id = account_id(33);
        let (index, ctx, out_ref) = indexed_unbound_account(id, 5_000_000, 0);
        let (events, _receivers) = event_channels(4);
        let state = HttpIntentState {
            account_index: Arc::new(Mutex::new(index)),
            ctx,
            config: GreenOrdersConfig::default(),
            events,
        };

        let bind = post_bind_account(
            State(state.clone()),
            Json(BindAccountRequest {
                account_id: hex::encode(id.bytes()),
                tx_hash: hex::encode(out_ref.tx_hash().to_raw_bytes()),
                output_index: out_ref.index(),
            }),
        )
        .await;
        assert_eq!(bind.0, StatusCode::OK);

        let intent = post_intent(State(state), Json(wire_intent(id))).await;
        assert_eq!(intent.0, StatusCode::ACCEPTED);
    }
}
