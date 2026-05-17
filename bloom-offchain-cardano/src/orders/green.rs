use std::cmp::Ordering;
use std::fmt::{Display, Formatter};

use bloom_offchain::execution_engine::liquidity_book::core::{Next, TerminalTake, Unit};
use bloom_offchain::execution_engine::liquidity_book::market_taker::{MarketTaker, TakerBehaviour};
use bloom_offchain::execution_engine::liquidity_book::side::Side;
use bloom_offchain::execution_engine::liquidity_book::time::TimeBounds;
use bloom_offchain::execution_engine::liquidity_book::types::{
    AbsolutePrice, FeeAsset, InputAsset, OutputAsset, RelativePrice,
};
use cml_chain::plutus::utils::ConstrPlutusDataEncoding;
use cml_chain::plutus::{ConstrPlutusData, PlutusData};
use cml_chain::transaction::TransactionOutput;
use cml_chain::PolicyId;
use cml_core::serialization::{LenEncoding, RawBytesEncoding, Serialize};
use cml_crypto::{blake2b224, blake2b256};
use spectrum_cardano_lib::ex_units::ExUnits;
use spectrum_cardano_lib::output::FinalizedTxOut;
use spectrum_cardano_lib::plutus_data::{ConstrPlutusDataExtension, IntoPlutusData, PlutusDataExtension};
use spectrum_cardano_lib::transaction::TransactionOutputExtension;
use spectrum_cardano_lib::types::TryFromPData;
use spectrum_cardano_lib::{AssetClass, AssetName, Token};
use spectrum_offchain::domain::{Stable, Tradable};
use spectrum_offchain_cardano::data::pair::{side_of, PairId};
use spectrum_offchain_cardano::deployment::{test_address, DeployedScriptInfo};

pub const ALEPH_ACCOUNT_VALIDATOR: u8 = 200;
pub const ALEPH_BATCH_WITNESS_VALIDATOR: u8 = 201;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct AccountId([u8; 32]);

impl AccountId {
    pub fn try_from_slice(bytes: &[u8]) -> Result<Self, GreenOrderValidationError> {
        let account_id = <[u8; 32]>::try_from(bytes)
            .map_err(|_| GreenOrderValidationError::InvalidAccountIdLength(bytes.len()))?;
        Ok(Self(account_id))
    }

    pub fn bytes(&self) -> [u8; 32] {
        self.0
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum GreenAuth {
    Sig {
        prefix: Vec<u8>,
        postfix: Vec<u8>,
        signature: Vec<u8>,
        update_proof: Vec<u8>,
    },
    Path {
        proof: Vec<u8>,
    },
}

impl GreenAuth {
    pub fn new_sig(
        prefix: Vec<u8>,
        postfix: Vec<u8>,
        signature: Vec<u8>,
        update_proof: Vec<u8>,
    ) -> Result<Self, GreenOrderValidationError> {
        if signature.len() != 64 {
            return Err(GreenOrderValidationError::InvalidCompactSignatureLength(
                signature.len(),
            ));
        }
        if !update_proof.is_empty() {
            return Err(GreenOrderValidationError::UnsupportedSigUpdateProof(
                update_proof.len(),
            ));
        }
        Ok(Self::Sig {
            prefix,
            postfix,
            signature,
            update_proof,
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AlephAccountState {
    pub magic: Vec<u8>,
    pub allowlist: Vec<[u8; 28]>,
    pub nonce: Vec<i64>,
    pub main_key: Vec<u8>,
    pub co_key: Option<Vec<u8>>,
    pub cold_key_hash: [u8; 28],
    pub store_root: [u8; 32],
}

impl AlephAccountState {
    pub fn with_sig_full_fill_nonce(
        mut self,
        target_nonce_index: u16,
        target_nonce_value: u64,
    ) -> Option<Self> {
        let nonce = self.nonce.get_mut(target_nonce_index as usize)?;
        if *nonce > target_nonce_value as i64 {
            return None;
        }
        *nonce = target_nonce_value as i64;
        Some(self)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum AlephAccountAction {
    Delegate(u64),
    Direct,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AlephIntention {
    pub target_nonce_index: u16,
    pub target_nonce_value: u64,
    pub leaving_asset: AssetClass,
    pub leaving_amount: u64,
    pub arriving_asset: AssetClass,
    pub expected_arriving_amount: u64,
    pub fee_lovelace: u64,
    pub operator: [u8; 28],
}

impl AlephIntention {
    pub fn digest(&self) -> [u8; 32] {
        blake2b256(self.clone().into_pd().to_cbor_bytes().as_ref())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AlephAuthorizedIntention {
    pub intent: AlephIntention,
    pub remainder: u64,
    pub auth: GreenAuth,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AlephBatchRedeemer {
    pub intentions: Vec<AlephAuthorizedIntention>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AlephAccountUtxo {
    pub output_ref: spectrum_cardano_lib::OutputRef,
    pub output: TransactionOutput,
    pub state: AlephAccountState,
}

impl AlephAccountUtxo {
    pub fn try_parse<C>(
        output_ref: spectrum_cardano_lib::OutputRef,
        output: TransactionOutput,
        ctx: &C,
    ) -> Option<Self>
    where
        C: spectrum_offchain::domain::Has<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>>,
    {
        use spectrum_cardano_lib::plutus_data::DatumExtension;
        use spectrum_cardano_lib::transaction::TransactionOutputExtension;

        if !test_address::<{ ALEPH_ACCOUNT_VALIDATOR }, _>(output.address(), ctx) {
            return None;
        }
        let state = AlephAccountState::try_from_pd(output.clone().into_datum()?.into_pd()?)?;
        Some(Self {
            output_ref,
            output,
            state,
        })
    }

    pub fn finalized_output(&self) -> FinalizedTxOut {
        FinalizedTxOut::new(self.output.clone(), self.output_ref)
    }
}

fn aiken_constr(alternative: u64, fields: Vec<PlutusData>) -> PlutusData {
    PlutusData::new_constr_plutus_data(ConstrPlutusData {
        alternative,
        fields,
        encodings: Some(ConstrPlutusDataEncoding {
            len_encoding: LenEncoding::Indefinite,
            prefer_compact: true,
            tag_encoding: None,
            alternative_encoding: None,
            fields_encoding: LenEncoding::Indefinite,
        }),
    })
}

fn asset_class_into_pd(asset: AssetClass) -> PlutusData {
    match asset {
        AssetClass::Native => aiken_constr(0, vec![Vec::<u8>::new().into_pd(), Vec::<u8>::new().into_pd()]),
        AssetClass::Token(Token(policy, asset_name)) => aiken_constr(
            0,
            vec![
                policy.to_raw_bytes().to_vec().into_pd(),
                asset_name.as_bytes().to_vec().into_pd(),
            ],
        ),
    }
}

impl IntoPlutusData for GreenAuth {
    fn into_pd(self) -> PlutusData {
        match self {
            GreenAuth::Path { proof } => aiken_constr(0, vec![proof.into_pd()]),
            GreenAuth::Sig {
                prefix,
                postfix,
                signature,
                update_proof,
            } => aiken_constr(
                1,
                vec![
                    prefix.into_pd(),
                    postfix.into_pd(),
                    signature.into_pd(),
                    update_proof.into_pd(),
                ],
            ),
        }
    }
}

impl IntoPlutusData for AlephAccountState {
    fn into_pd(self) -> PlutusData {
        let co_key = self.co_key.unwrap_or_default();
        aiken_constr(
            0,
            vec![
                self.magic.into_pd(),
                self.allowlist.into_pd(),
                self.nonce.into_pd(),
                aiken_constr(0, vec![self.main_key.into_pd(), co_key.into_pd()]),
                self.cold_key_hash.into_pd(),
                self.store_root.into_pd(),
            ],
        )
    }
}

impl IntoPlutusData for AlephAccountAction {
    fn into_pd(self) -> PlutusData {
        match self {
            AlephAccountAction::Delegate(index) => aiken_constr(0, vec![index.into_pd()]),
            AlephAccountAction::Direct => aiken_constr(1, vec![]),
        }
    }
}

impl IntoPlutusData for AlephIntention {
    fn into_pd(self) -> PlutusData {
        aiken_constr(
            0,
            vec![
                aiken_constr(
                    0,
                    vec![
                        u64::from(self.target_nonce_index).into_pd(),
                        self.target_nonce_value.into_pd(),
                    ],
                ),
                asset_class_into_pd(self.leaving_asset),
                self.leaving_amount.into_pd(),
                asset_class_into_pd(self.arriving_asset),
                self.expected_arriving_amount.into_pd(),
                self.fee_lovelace.into_pd(),
                self.operator.into_pd(),
            ],
        )
    }
}

impl IntoPlutusData for AlephAuthorizedIntention {
    fn into_pd(self) -> PlutusData {
        aiken_constr(
            0,
            vec![
                self.intent.into_pd(),
                self.remainder.into_pd(),
                self.auth.into_pd(),
            ],
        )
    }
}

impl IntoPlutusData for AlephBatchRedeemer {
    fn into_pd(self) -> PlutusData {
        aiken_constr(0, vec![self.intentions.into_pd()])
    }
}

impl TryFromPData for AlephAccountState {
    fn try_from_pd(data: PlutusData) -> Option<Self> {
        let mut cpd = data.into_constr_pd()?;
        if cpd.alternative != 0 || cpd.fields.len() != 6 {
            return None;
        }
        let magic = cpd.take_field(0)?.into_bytes()?;
        let allowlist = cpd
            .take_field(1)?
            .into_vec()?
            .into_iter()
            .map(|pd| <[u8; 28]>::try_from(pd.into_bytes()?).ok())
            .collect::<Option<Vec<_>>>()?;
        let nonce = cpd
            .take_field(2)?
            .into_vec()?
            .into_iter()
            .map(|pd| pd.into_i128().and_then(|n| i64::try_from(n).ok()))
            .collect::<Option<Vec<_>>>()?;
        let mut hot_cred = cpd.take_field(3)?.into_constr_pd()?;
        let main_key = hot_cred.take_field(0)?.into_bytes()?;
        if main_key.len() != 33 {
            return None;
        }
        let co_key_bytes = hot_cred.take_field(1)?.into_bytes()?;
        let co_key = if co_key_bytes.is_empty() {
            None
        } else if co_key_bytes.len() == 33 {
            Some(co_key_bytes)
        } else {
            return None;
        };
        let cold_key_hash = <[u8; 28]>::try_from(cpd.take_field(4)?.into_bytes()?).ok()?;
        let store_root = <[u8; 32]>::try_from(cpd.take_field(5)?.into_bytes()?).ok()?;
        Some(Self {
            magic,
            allowlist,
            nonce,
            main_key,
            co_key,
            cold_key_hash,
            store_root,
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct GreenIntention {
    pub input_asset: AssetClass,
    pub output_asset: AssetClass,
    pub leaving_amount: u64,
    pub expected_arriving_amount: u64,
    pub fee_lovelace: u64,
    pub target_nonce_slot: u16,
    pub target_nonce_value: u64,
    pub operator_key_hash: [u8; 28],
}

impl GreenIntention {
    pub fn try_operator_key_hash(bytes: Vec<u8>) -> Result<[u8; 28], GreenOrderValidationError> {
        <[u8; 28]>::try_from(bytes.as_slice())
            .map_err(|_| GreenOrderValidationError::InvalidOperatorKeyHashLength(bytes.len()))
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct GreenOrderId {
    account_id: AccountId,
    target_nonce_slot: u16,
    target_nonce_value: u64,
    original_intent_digest: [u8; 32],
}

impl GreenOrderId {
    pub fn new(
        account_id: AccountId,
        target_nonce_slot: u16,
        target_nonce_value: u64,
        original_intent_digest: [u8; 32],
    ) -> Self {
        Self {
            account_id,
            target_nonce_slot,
            target_nonce_value,
            original_intent_digest,
        }
    }

    pub fn stable_token(&self) -> Token {
        let mut bytes = Vec::with_capacity(32 + 2 + 8 + 32);
        bytes.extend_from_slice(&self.account_id.bytes());
        bytes.extend_from_slice(&self.target_nonce_slot.to_be_bytes());
        bytes.extend_from_slice(&self.target_nonce_value.to_be_bytes());
        bytes.extend_from_slice(&self.original_intent_digest);
        Token(
            PolicyId::from(blake2b224(&bytes)),
            AssetName::from_utf8("green-order".to_string()),
        )
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct GreenOrder {
    pub id: GreenOrderId,
    pub account_id: AccountId,
    pub intention: GreenIntention,
    pub accumulated_output: u64,
    pub current_remainder: u64,
    pub auth: GreenAuth,
}

impl GreenOrder {
    pub fn stable_token(&self) -> Token {
        self.id.stable_token()
    }

    pub fn aleph_intention(&self) -> AlephIntention {
        AlephIntention {
            target_nonce_index: self.intention.target_nonce_slot,
            target_nonce_value: self.intention.target_nonce_value,
            leaving_asset: self.intention.input_asset,
            leaving_amount: self.intention.leaving_amount,
            arriving_asset: self.intention.output_asset,
            expected_arriving_amount: self.intention.expected_arriving_amount,
            fee_lovelace: self.intention.fee_lovelace,
            operator: self.intention.operator_key_hash,
        }
    }
}

pub trait GreenAccountLookup {
    fn current_account(&self, account_id: AccountId) -> Option<FinalizedTxOut>;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum GreenExecutionError {
    NonAdaLeavingAsset,
    InsufficientAdaOutputForFee,
}

pub fn apply_full_fill_to_account_output(
    output: &mut TransactionOutput,
    order: &GreenOrder,
    removed_input: u64,
    added_output: u64,
    consumed_fee: u64,
) -> Result<(), GreenExecutionError> {
    if order.intention.input_asset != AssetClass::Native {
        return Err(GreenExecutionError::NonAdaLeavingAsset);
    }
    output.sub_asset(AssetClass::Native, removed_input + consumed_fee);
    if order.intention.output_asset == AssetClass::Native {
        let net_output = added_output
            .checked_sub(consumed_fee)
            .ok_or(GreenExecutionError::InsufficientAdaOutputForFee)?;
        output.add_asset(AssetClass::Native, net_output);
    } else {
        output.add_asset(order.intention.output_asset, added_output);
    }
    Ok(())
}

impl Display for GreenOrder {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            format!(
                "GreenOrder(pair={}, side={:?}, input={}, output={}, expected={}, id={:?})",
                self.pair_id(),
                self.side(),
                self.current_remainder,
                self.accumulated_output,
                self.intention.expected_arriving_amount,
                self.id,
            )
            .as_str(),
        )
    }
}

impl Ord for GreenOrder {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}

impl PartialOrd for GreenOrder {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Stable for GreenOrder {
    type StableId = Token;

    fn stable_id(&self) -> Self::StableId {
        self.stable_token()
    }

    fn is_quasi_permanent(&self) -> bool {
        false
    }
}

impl Tradable for GreenOrder {
    type PairId = PairId;

    fn pair_id(&self) -> Self::PairId {
        PairId::canonical(self.intention.input_asset, self.intention.output_asset)
    }
}

impl MarketTaker for GreenOrder {
    type U = ExUnits;

    fn side(&self) -> Side {
        side_of(self.intention.input_asset, self.intention.output_asset)
    }

    fn input(&self) -> InputAsset<u64> {
        self.current_remainder
    }

    fn output(&self) -> OutputAsset<u64> {
        self.accumulated_output
    }

    fn price(&self) -> AbsolutePrice {
        AbsolutePrice::from_price(
            self.side(),
            RelativePrice::new(
                self.intention.expected_arriving_amount as u128,
                self.intention.leaving_amount as u128,
            ),
        )
    }

    fn operator_fee(&self, input_consumed: InputAsset<u64>) -> FeeAsset<u64> {
        self.intention
            .fee_lovelace
            .saturating_mul(input_consumed)
            .checked_div(self.intention.leaving_amount)
            .unwrap_or(0)
    }

    fn fee(&self) -> FeeAsset<u64> {
        self.intention.fee_lovelace
    }

    fn budget(&self) -> FeeAsset<u64> {
        0
    }

    fn consumable_budget(&self) -> FeeAsset<u64> {
        0
    }

    fn marginal_cost_hint(&self) -> Self::U {
        ExUnits { mem: 0, steps: 0 }
    }

    fn min_marginal_output(&self) -> OutputAsset<u64> {
        self.intention.expected_arriving_amount
    }

    fn time_bounds(&self) -> TimeBounds<u64> {
        TimeBounds::None
    }
}

impl TakerBehaviour for GreenOrder {
    fn with_updated_time(self, _: u64) -> Next<Self, Unit> {
        Next::Succ(self)
    }

    fn with_applied_trade(
        mut self,
        removed_input: InputAsset<u64>,
        added_output: OutputAsset<u64>,
    ) -> Next<Self, TerminalTake> {
        self.current_remainder = self.current_remainder.saturating_sub(removed_input);
        self.accumulated_output = self.accumulated_output.saturating_add(added_output);
        self.try_terminate()
    }

    fn with_budget_corrected(self, _: i64) -> (i64, Self) {
        (0, self)
    }

    fn with_fee_charged(self, _: u64) -> Self {
        self
    }

    fn with_output_added(mut self, added_output: u64) -> Self {
        self.accumulated_output = self.accumulated_output.saturating_add(added_output);
        self
    }

    fn try_terminate(self) -> Next<Self, TerminalTake> {
        if self.current_remainder == 0 {
            Next::Term(TerminalTake {
                remaining_input: 0,
                accumulated_output: self.accumulated_output,
                remaining_fee: self.intention.fee_lovelace,
                remaining_budget: 0,
            })
        } else {
            Next::Succ(self)
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum GreenOrderValidationError {
    InvalidAccountIdLength(usize),
    InvalidSecp256k1KeyLength(usize),
    InvalidSecp256k1CoKeyLength(usize),
    InvalidCompactSignatureLength(usize),
    UnsupportedSigUpdateProof(usize),
    InvalidOperatorKeyHashLength(usize),
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use bloom_offchain::execution_engine::bundled::Bundled;
    use bloom_offchain::execution_engine::liquidity_book::core::{Next, Trans};
    use bloom_offchain::execution_engine::liquidity_book::market_taker::{MarketTaker, TakerBehaviour};
    use cml_chain::address::{Address, EnterpriseAddress};
    use cml_chain::assets::AssetBundle;
    use cml_chain::certs::Credential;
    use cml_chain::transaction::{ConwayFormatTxOut, DatumOption, TransactionOutput};
    use cml_chain::PolicyId;
    use cml_chain::Value;
    use cml_crypto::{ScriptHash, TransactionHash};
    use spectrum_cardano_lib::ex_units::ExUnits;
    use spectrum_cardano_lib::plutus_data::{ConstrPlutusDataExtension, IntoPlutusData, PlutusDataExtension};
    use spectrum_cardano_lib::types::TryFromPData;
    use spectrum_cardano_lib::{AssetClass, AssetName, OutputRef, Token};
    use spectrum_offchain::domain::{Has, Stable, Tradable};
    use spectrum_offchain_cardano::deployment::DeployedScriptInfo;
    use type_equalities::IsEqual;

    use super::{
        AccountId, AlephAccountAction, AlephAccountState, AlephAccountUtxo, AlephAuthorizedIntention,
        AlephBatchRedeemer, AlephIntention, GreenAuth, GreenIntention, GreenOrder, GreenOrderId,
        GreenOrderValidationError, ALEPH_ACCOUNT_VALIDATOR,
    };

    fn token(seed: u8) -> Token {
        Token(
            PolicyId::from([seed; 28]),
            AssetName::from_utf8(format!("t{seed}")),
        )
    }

    fn intention() -> GreenIntention {
        GreenIntention {
            input_asset: AssetClass::Native,
            output_asset: AssetClass::Token(token(1)),
            leaving_amount: 1_000_000,
            expected_arriving_amount: 900_000,
            fee_lovelace: 2_000_000,
            target_nonce_slot: 2,
            target_nonce_value: 10,
            operator_key_hash: [7; 28],
        }
    }

    fn aleph_intention() -> AlephIntention {
        AlephIntention {
            target_nonce_index: 2,
            target_nonce_value: 10,
            leaving_asset: AssetClass::Native,
            leaving_amount: 1_000_000,
            arriving_asset: AssetClass::Token(token(1)),
            expected_arriving_amount: 900_000,
            fee_lovelace: 2_000_000,
            operator: [7; 28],
        }
    }

    fn account_state() -> AlephAccountState {
        AlephAccountState {
            magic: b"green-test".to_vec(),
            allowlist: vec![[11; 28], [12; 28]],
            nonce: vec![0, 0, 0, 0],
            main_key: vec![2; 33],
            co_key: None,
            cold_key_hash: [13; 28],
            store_root: [14; 32],
        }
    }

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

    #[test]
    fn parses_aleph_account_output() {
        let script_hash = ScriptHash::from([9; 28]);
        let ctx = AccountCtx {
            account: DeployedScriptInfo {
                script_hash,
                marginal_cost: ExUnits { mem: 0, steps: 0 },
            },
        };
        let output_ref = OutputRef::new(TransactionHash::from([1; 32]), 0);
        let output = TransactionOutput::new_conway_format_tx_out(ConwayFormatTxOut {
            address: Address::Enterprise(EnterpriseAddress::new(0, Credential::new_script(script_hash))),
            amount: Value::new(2_000_000, AssetBundle::new()),
            datum_option: Some(DatumOption::Datum {
                datum: account_state().into_pd(),
                len_encoding: Default::default(),
                tag_encoding: None,
                datum_tag_encoding: None,
                datum_bytes_encoding: Default::default(),
            }),
            script_reference: None,
            encodings: None,
        });

        let account = AlephAccountUtxo::try_parse(output_ref, output, &ctx).unwrap();

        assert_eq!([14; 32], account.state.store_root);
        assert_eq!(output_ref, account.output_ref);
    }

    #[test]
    fn aleph_account_action_delegate_encoding() {
        let mut cpd = AlephAccountAction::Delegate(0)
            .into_pd()
            .into_constr_pd()
            .unwrap();

        assert_eq!(0, cpd.alternative);
        assert_eq!(0, cpd.take_field(0).unwrap().into_u64().unwrap());
    }

    #[test]
    fn aleph_account_action_direct_encoding() {
        let cpd = AlephAccountAction::Direct.into_pd().into_constr_pd().unwrap();

        assert_eq!(1, cpd.alternative);
        assert!(cpd.fields.is_empty());
    }

    #[test]
    fn aleph_account_state_roundtrip_and_nonce_update() {
        let state = account_state();
        let parsed = AlephAccountState::try_from_pd(state.clone().into_pd()).unwrap();
        let updated = parsed.with_sig_full_fill_nonce(2, 10).unwrap();

        assert_eq!(state.magic, updated.magic);
        assert_eq!(state.allowlist, updated.allowlist);
        assert_eq!(vec![0, 0, 10, 0], updated.nonce);
        assert_eq!(state.main_key, updated.main_key);
        assert_eq!(state.co_key, updated.co_key);
        assert_eq!(state.cold_key_hash, updated.cold_key_hash);
        assert_eq!(state.store_root, updated.store_root);
    }

    #[test]
    fn aleph_intention_digest_is_stable() {
        let digest = aleph_intention().digest();

        assert_eq!(digest, aleph_intention().digest());
        assert_eq!(32, digest.len());
    }

    #[test]
    fn aleph_batch_redeemer_encoding() {
        let redeemer = AlephBatchRedeemer {
            intentions: vec![AlephAuthorizedIntention {
                intent: aleph_intention(),
                remainder: 0,
                auth: GreenAuth::new_sig(vec![], vec![], vec![3; 64], vec![]).unwrap(),
            }],
        };
        let mut cpd = redeemer.into_pd().into_constr_pd().unwrap();
        let intentions = cpd.take_field(0).unwrap().into_vec().unwrap();

        assert_eq!(0, cpd.alternative);
        assert_eq!(1, intentions.len());
    }

    #[test]
    fn rejects_wrong_account_id_length() {
        assert_eq!(
            AccountId::try_from_slice(&[1; 31]),
            Err(GreenOrderValidationError::InvalidAccountIdLength(31))
        );
        assert_eq!(
            AccountId::try_from_slice(&[1; 33]),
            Err(GreenOrderValidationError::InvalidAccountIdLength(33))
        );
        assert!(AccountId::try_from_slice(&[1; 32]).is_ok());
    }

    #[test]
    fn rejects_wrong_sig_auth_lengths() {
        assert_eq!(
            GreenAuth::new_sig(vec![], vec![], vec![3; 63], vec![]),
            Err(GreenOrderValidationError::InvalidCompactSignatureLength(63))
        );
        assert_eq!(
            GreenAuth::new_sig(vec![], vec![], vec![3; 64], vec![4; 1]),
            Err(GreenOrderValidationError::UnsupportedSigUpdateProof(1))
        );
        assert!(GreenAuth::new_sig(vec![], vec![], vec![3; 64], vec![]).is_ok());
    }

    #[test]
    fn rejects_wrong_operator_key_hash_length() {
        assert_eq!(
            GreenIntention::try_operator_key_hash(vec![1; 27]),
            Err(GreenOrderValidationError::InvalidOperatorKeyHashLength(27))
        );
        assert_eq!(
            GreenIntention::try_operator_key_hash(vec![1; 29]),
            Err(GreenOrderValidationError::InvalidOperatorKeyHashLength(29))
        );
        assert!(GreenIntention::try_operator_key_hash(vec![1; 28]).is_ok());
    }

    #[test]
    fn order_id_stable_token_does_not_change_when_remainder_changes() {
        let account_id = AccountId::try_from_slice(&[9; 32]).unwrap();
        let intent = intention();
        let auth = GreenAuth::new_sig(vec![], vec![], vec![3; 64], vec![]).unwrap();
        let original_digest = [5; 32];
        let id = GreenOrderId::new(
            account_id,
            intent.target_nonce_slot,
            intent.target_nonce_value,
            original_digest,
        );

        let first = GreenOrder {
            id,
            account_id,
            intention: intent.clone(),
            accumulated_output: 0,
            current_remainder: intent.leaving_amount,
            auth: auth.clone(),
        };
        let second = GreenOrder {
            current_remainder: 0,
            ..first.clone()
        };

        assert_eq!(first.stable_token(), second.stable_token());
    }

    #[test]
    fn stable_and_pair_ids_are_exposed_for_book_indexing() {
        let account_id = AccountId::try_from_slice(&[9; 32]).unwrap();
        let intent = intention();
        let id = GreenOrderId::new(
            account_id,
            intent.target_nonce_slot,
            intent.target_nonce_value,
            [5; 32],
        );
        let order = GreenOrder {
            id,
            account_id,
            intention: intent.clone(),
            accumulated_output: 0,
            current_remainder: intent.leaving_amount,
            auth: GreenAuth::new_sig(vec![], vec![], vec![3; 64], vec![]).unwrap(),
        };

        assert_eq!(id.stable_token(), order.stable_id());
        assert_eq!(
            spectrum_offchain_cardano::data::pair::PairId::canonical(intent.input_asset, intent.output_asset),
            order.pair_id()
        );
    }

    #[test]
    fn ordering_ignores_mutable_remainder() {
        let account_id = AccountId::try_from_slice(&[9; 32]).unwrap();
        let intent = intention();
        let id = GreenOrderId::new(
            account_id,
            intent.target_nonce_slot,
            intent.target_nonce_value,
            [5; 32],
        );
        let first = GreenOrder {
            id,
            account_id,
            intention: intent.clone(),
            accumulated_output: 0,
            current_remainder: intent.leaving_amount,
            auth: GreenAuth::new_sig(vec![], vec![], vec![3; 64], vec![]).unwrap(),
        };
        let second = GreenOrder {
            current_remainder: 0,
            accumulated_output: intent.expected_arriving_amount,
            ..first.clone()
        };

        assert_eq!(Ordering::Equal, first.cmp(&second));
    }

    #[test]
    fn full_fill_consumes_green_order_fee_and_zero_budget_after_finalization() {
        let account_id = AccountId::try_from_slice(&[9; 32]).unwrap();
        let intent = intention();
        let id = GreenOrderId::new(
            account_id,
            intent.target_nonce_slot,
            intent.target_nonce_value,
            [5; 32],
        );
        let order = GreenOrder {
            id,
            account_id,
            intention: intent.clone(),
            accumulated_output: intent.expected_arriving_amount,
            current_remainder: 0,
            auth: GreenAuth::new_sig(vec![], vec![], vec![3; 64], vec![]).unwrap(),
        };

        let transition = order
            .clone()
            .with_applied_trade(intent.leaving_amount, intent.expected_arriving_amount);
        let Next::Term(term) = transition else {
            panic!("full-fill green order must terminate")
        };
        assert_eq!(intent.fee_lovelace, term.remaining_fee);

        let mut finalized_term = term;
        finalized_term.remaining_fee -= order.operator_fee(intent.leaving_amount);
        let finalized = Trans::new(Bundled(order, ()), Next::Term(finalized_term));

        assert_eq!(intent.fee_lovelace, finalized.consumed_fee());
        assert_eq!(0, finalized.consumed_budget());
    }
}
