use std::cmp::Ordering;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};

use bloom_offchain::execution_engine::funding_effect::FundingIO;
use bloom_offchain::execution_engine::liquidity_book::types::Lovelace;
use cml_chain::builders::input_builder::SingleInputBuilder;
use cml_chain::builders::output_builder::SingleOutputBuilderResult;
use cml_chain::builders::redeemer_builder::RedeemerWitnessKey;
use cml_chain::builders::tx_builder::{TransactionBuilder, TransactionUnspentOutput};
use cml_chain::builders::withdrawal_builder::SingleWithdrawalBuilder;
use cml_chain::builders::witness_builder::{PartialPlutusWitness, PlutusScriptWitness};
use cml_chain::certs::Credential;
use cml_chain::plutus::{PlutusData, RedeemerTag};
use cml_chain::transaction::{TransactionInput, TransactionOutput};
use cml_chain::{RequiredSigners, Value};
use cml_core::serialization::RawBytesEncoding;
use either::Either;
use log::trace;
use spectrum_cardano_lib::output::FinalizedTxOut;
use spectrum_cardano_lib::plutus_data::IntoPlutusData;
use spectrum_cardano_lib::transaction::TransactionOutputExtension;
use spectrum_cardano_lib::value::ValueExtension;
use spectrum_cardano_lib::{AssetClass, NetworkId, OutputRef};
use spectrum_offchain_cardano::constants::MIN_SAFE_LOVELACE_VALUE;
use spectrum_offchain_cardano::creds::OperatorRewardAddress;
use spectrum_offchain_cardano::deployment::DeployedValidatorErased;
use spectrum_offchain_cardano::script::{
    DelayedRedeemer, ScriptContextPreview, ScriptWitness, TxInputsOrdering,
};

use crate::orders::green::{AlephAuthorizedIntention, AlephBatchRedeemer};

pub struct ScriptInputBlueprint {
    pub reference: OutputRef,
    pub utxo: TransactionOutput,
    pub script: ScriptWitness,
    pub redeemer: DelayedRedeemer,
    pub required_signers: RequiredSigners,
}

pub type ScalingFactor = u64;

/// Blueprint of a DEX transaction.
/// Accumulates information that is later used to create real transaction
/// that executes a batch of DEX operations.
pub struct TxBlueprint {
    pub script_io: Vec<(ScriptInputBlueprint, TransactionOutput)>,
    account_script_refs: HashSet<OutputRef>,
    pub reference_inputs: HashSet<(TransactionInput, TransactionOutput)>,
    pub witness_scripts: HashMap<DeployedValidatorErased, (PlutusData, ScalingFactor)>,
    aleph_batch_witness: Option<(
        DeployedValidatorErased,
        Vec<(OutputRef, AlephAuthorizedIntention)>,
    )>,
}

impl Display for TxBlueprint {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("TxBlueprint(io: ")?;
        for (ix, (i, o)) in self.script_io.iter().enumerate() {
            f.write_str("[")?;
            f.write_str(
                format!(
                    "Input{}(ref: {}, addr: {}, value: {}), ",
                    ix,
                    i.reference,
                    i.utxo
                        .address()
                        .to_bech32(None)
                        .unwrap_or_else(|_| "_".to_string()),
                    serde_json::to_string(i.utxo.value()).unwrap_or_else(|_| "-".to_string())
                )
                .as_str(),
            )?;
            f.write_str(
                format!(
                    "Output{}(addr: {}, value: {})",
                    ix,
                    o.address().to_bech32(None).unwrap_or_else(|_| "-".to_string()),
                    serde_json::to_string(o.value()).unwrap_or_else(|_| "-".to_string())
                )
                .as_str(),
            )?;
            f.write_str("]")?;
        }
        f.write_str(")")
    }
}

impl TxBlueprint {
    pub fn new() -> Self {
        Self {
            script_io: Vec::new(),
            account_script_refs: HashSet::new(),
            reference_inputs: HashSet::new(),
            witness_scripts: HashMap::new(),
            aleph_batch_witness: None,
        }
    }

    pub fn add_io(&mut self, input: ScriptInputBlueprint, output: TransactionOutput) {
        self.script_io.push((input, output));
    }

    pub fn add_account_io(&mut self, input: ScriptInputBlueprint, output: TransactionOutput) {
        self.account_script_refs.insert(input.reference);
        self.script_io.push((input, output));
    }

    pub fn add_ref_input(&mut self, utxo: TransactionUnspentOutput) {
        self.reference_inputs.insert((utxo.input, utxo.output));
    }

    pub fn add_witness(&mut self, wit: DeployedValidatorErased, redeemer: PlutusData) {
        match self.witness_scripts.entry(wit) {
            Entry::Occupied(mut e) => {
                let (_, sf) = e.get_mut();
                *sf += 1;
            }
            Entry::Vacant(e) => {
                e.insert((redeemer, 1));
            }
        }
    }

    pub fn add_aleph_batch_intention(
        &mut self,
        witness: DeployedValidatorErased,
        account_ref: OutputRef,
        intention: AlephAuthorizedIntention,
    ) {
        match &mut self.aleph_batch_witness {
            Some((existing, intentions)) if *existing == witness => intentions.push((account_ref, intention)),
            Some((existing, _)) => panic!(
                "multiple Aleph batch witness validators in one transaction: {} and {}",
                existing, witness
            ),
            None => self.aleph_batch_witness = Some((witness, vec![(account_ref, intention)])),
        }
    }

    pub fn forces_operator_funding_input(&self) -> bool {
        self.aleph_batch_witness.is_some()
    }

    fn cmp_script_refs(&self, lh: &OutputRef, rh: &OutputRef) -> Ordering {
        lh.cmp(rh)
    }

    pub fn project_onto_builder(
        self,
        mut txb: TransactionBuilder,
        network_id: NetworkId,
        operator_address: OperatorRewardAddress,
        operator_funding: FinalizedTxOut,
        operator_interest: u64,
        operator_funding_fee_reserve: u64,
    ) -> (TransactionBuilder, FundingIO<FinalizedTxOut, TransactionOutput>) {
        let TxBlueprint {
            script_io,
            account_script_refs,
            reference_inputs,
            mut witness_scripts,
            aleph_batch_witness,
        } = self;
        let force_operator_funding_input = aleph_batch_witness.is_some();
        let mut all_io = script_io.into_iter().map(Either::Left).collect::<Vec<_>>();
        let funding_io = if operator_interest > 0 {
            if operator_interest >= MIN_SAFE_LOVELACE_VALUE && !force_operator_funding_input {
                let reward_dest_address = operator_address.address();
                let reward_dest_coincides_with_funding = reward_dest_address == *operator_funding.0.address();
                let operator_output =
                    TransactionOutput::new(reward_dest_address, Value::from(operator_interest), None, None);
                all_io.push(Either::Right((None, operator_output.clone())));
                if reward_dest_coincides_with_funding {
                    // If reward dest address coincides with funding address, then it can be used as funding.
                    FundingIO::Added(operator_funding, operator_output)
                } else {
                    FundingIO::NotUsed(operator_funding)
                }
            } else if force_operator_funding_input {
                let mut operator_output = operator_funding.0.clone();
                if operator_funding_fee_reserve > 0 {
                    operator_output
                        .value_mut()
                        .sub_unsafe(AssetClass::Native, operator_funding_fee_reserve);
                }
                if operator_interest < MIN_SAFE_LOVELACE_VALUE {
                    operator_output
                        .value_mut()
                        .add_unsafe(AssetClass::Native, operator_interest);
                }
                all_io.push(Either::Right((
                    Some(operator_funding.clone()),
                    operator_output.clone(),
                )));
                if operator_interest >= MIN_SAFE_LOVELACE_VALUE {
                    let reward_output = TransactionOutput::new(
                        operator_address.address(),
                        Value::from(operator_interest),
                        None,
                        None,
                    );
                    all_io.push(Either::Right((None, reward_output)));
                }
                FundingIO::Replaced(operator_funding, operator_output)
            } else {
                // If funding utxo has to be used it is replaced with `operator_output`.
                let reward_dest_address = operator_funding.0.address().clone();
                let mut value = operator_funding.0.value().clone();
                value.add_unsafe(AssetClass::Native, operator_interest);
                let operator_output = TransactionOutput::new(reward_dest_address, value, None, None);
                all_io.push(Either::Right((
                    Some(operator_funding.clone()),
                    operator_output.clone(),
                )));
                FundingIO::Replaced(operator_funding, operator_output)
            }
        } else if force_operator_funding_input {
            let mut operator_output = operator_funding.0.clone();
            if operator_funding_fee_reserve > 0 {
                operator_output
                    .value_mut()
                    .sub_unsafe(AssetClass::Native, operator_funding_fee_reserve);
            }
            all_io.push(Either::Right((
                Some(operator_funding.clone()),
                operator_output.clone(),
            )));
            FundingIO::Replaced(operator_funding, operator_output)
        } else {
            FundingIO::NotUsed(operator_funding)
        };
        all_io.sort_by(|lh, rh| {
            let rank = |io: &Either<
                (ScriptInputBlueprint, TransactionOutput),
                (Option<FinalizedTxOut>, TransactionOutput),
            >| {
                if force_operator_funding_input {
                    match io {
                        Either::Left((input, _)) if account_script_refs.contains(&input.reference) => 0,
                        Either::Right((Some(_), _)) => 1,
                        Either::Left(_) => 2,
                        Either::Right((None, _)) => 3,
                    }
                } else {
                    match io {
                        Either::Right((None, _)) => 0,
                        Either::Left(_) | Either::Right((Some(_), _)) => 1,
                    }
                }
            };
            rank(lh).cmp(&rank(rh)).then_with(|| match (lh, rh) {
                (Either::Left((lh_in, _)), Either::Left((rh_in, _))) => lh_in.reference.cmp(&rh_in.reference),
                (Either::Left((lh_in, _)), Either::Right((Some(rh_in), _))) => {
                    lh_in.reference.cmp(&rh_in.reference())
                }
                (Either::Right((Some(lh_in), _)), Either::Left((rh_in, _))) => {
                    lh_in.reference().cmp(&rh_in.reference)
                }
                (Either::Right((Some(lh_in), _)), Either::Right((Some(rh_in), _))) => {
                    lh_in.reference().cmp(&rh_in.reference())
                }
                _ => Ordering::Equal,
            })
        });
        let mut cml_input_refs = all_io
            .iter()
            .filter_map(|io| match io {
                Either::Left((input, _)) => Some(input.reference),
                Either::Right((Some(input), _)) => Some(input.reference()),
                Either::Right((None, _)) => None,
            })
            .collect::<Vec<_>>();
        cml_input_refs.sort();
        let cml_spend_indexes = cml_input_refs
            .into_iter()
            .enumerate()
            .map(|(ix, reference)| (reference, ix))
            .collect::<HashMap<_, _>>();
        let required_signers = all_io
            .iter()
            .filter_map(|io| match io {
                Either::Left((input, _)) if account_script_refs.contains(&input.reference) => {
                    Some(input.required_signers.iter().cloned().collect::<Vec<_>>())
                }
                _ => None,
            })
            .flatten()
            .collect::<HashSet<_>>();
        for signer in required_signers {
            txb.add_required_signer(signer);
        }
        let enumerated_io = all_io.into_iter().enumerate().collect::<Vec<_>>();
        let inputs_ordering = TxInputsOrdering::new(HashMap::from_iter(
            cml_spend_indexes.iter().map(|(reference, ix)| (*reference, *ix)),
        ));
        if let Some((wit, mut intentions)) = aleph_batch_witness {
            intentions.sort_by_key(|(account_ref, _)| inputs_ordering.index_of(account_ref));
            let scaling_factor = intentions.len() as u64;
            let intentions = intentions
                .into_iter()
                .map(|(_, intention)| intention)
                .collect::<Vec<_>>();
            witness_scripts.insert(wit, (AlephBatchRedeemer { intentions }.into_pd(), scaling_factor));
        }
        for (ref_in, ref_utxo) in reference_inputs {
            let script_ref_info = ref_utxo
                .script_ref()
                .map(|script_ref| {
                    let hash = script_ref.hash();
                    let raw_len = script_ref
                        .raw_plutus_bytes()
                        .map(|bytes| bytes.len())
                        .unwrap_or_default();
                    format!("script_hash={}, raw_plutus_bytes={}", hash, raw_len)
                })
                .unwrap_or_else(|| "no script_ref".to_string());
            trace!(
                "Adding reference input {}#{} ({})",
                ref_in.transaction_id,
                ref_in.index,
                script_ref_info
            );
            txb.add_reference_input(TransactionUnspentOutput::new(ref_in, ref_utxo));
        }
        let mut pending_spend_exunits = Vec::new();
        for (_, io) in enumerated_io {
            match io {
                Either::Left((
                    ScriptInputBlueprint {
                        reference,
                        utxo,
                        script,
                        redeemer,
                        required_signers,
                    },
                    output,
                )) => {
                    let cml_script = PartialPlutusWitness::new(
                        PlutusScriptWitness::Ref(script.hash),
                        redeemer.compute(&inputs_ordering),
                    );
                    let input = SingleInputBuilder::new(reference.into(), utxo)
                        .plutus_script_inline_datum(cml_script, required_signers)
                        .unwrap();
                    let output = SingleOutputBuilderResult::new(output);
                    txb.add_input(input).expect("add script input ok");
                    trace!(
                        "Adding output: {} -> {}",
                        serde_json::to_string(output.output.value()).unwrap(),
                        output
                            .output
                            .address()
                            .to_bech32(None)
                            .unwrap_or_else(|_| "_".to_string())
                    );
                    txb.add_output(output).expect("add script output ok");
                    let cml_spend_ix = *cml_spend_indexes
                        .get(&reference)
                        .expect("script input must have CML spend index");
                    let ctx = ScriptContextPreview {
                        self_index: cml_spend_ix,
                    };
                    pending_spend_exunits.push((
                        RedeemerWitnessKey::new(RedeemerTag::Spend, cml_spend_ix as u64),
                        script.cost.compute(&ctx).into(),
                    ));
                }
                Either::Right((maybe_funding_input, funding_output)) => {
                    if let Some(FinalizedTxOut(utxo, reference)) = maybe_funding_input {
                        let input = SingleInputBuilder::new(reference.into(), utxo)
                            .payment_key()
                            .unwrap();
                        txb.add_input(input).expect("add funding input ok");
                    }
                    let output = SingleOutputBuilderResult::new(funding_output);
                    txb.add_output(output).expect("add funding output ok");
                }
            }
        }
        for (key, ex_units) in pending_spend_exunits {
            txb.set_exunits(key, ex_units);
        }
        // Project common witness scripts.
        let mut witness_scripts = witness_scripts.into_iter().collect::<Vec<_>>();
        witness_scripts.sort_by(|(lh, _), (rh, _)| {
            lh.hash
                .to_raw_bytes()
                .cmp(&rh.hash.to_raw_bytes())
                .then_with(|| {
                    lh.reference_utxo
                        .input
                        .transaction_id
                        .cmp(&rh.reference_utxo.input.transaction_id)
                })
                .then_with(|| lh.reference_utxo.input.index.cmp(&rh.reference_utxo.input.index))
        });
        let mut reward_addresses = witness_scripts
            .iter()
            .map(|(wit, _)| {
                cml_chain::address::RewardAddress::new(network_id.into(), Credential::new_script(wit.hash))
            })
            .collect::<Vec<_>>();
        reward_addresses.sort();
        let mut pending_reward_exunits = Vec::new();
        for (wit, (rdmr, scaling_factor)) in witness_scripts {
            let reward_address =
                cml_chain::address::RewardAddress::new(network_id.into(), Credential::new_script(wit.hash));
            let reward_ix = reward_addresses
                .iter()
                .position(|addr| addr == &reward_address)
                .expect("reward address must have CML reward index") as u64;
            let partial_witness = PartialPlutusWitness::new(PlutusScriptWitness::Ref(wit.hash), rdmr);
            let withdrawal_result = SingleWithdrawalBuilder::new(reward_address.clone(), 0)
                .plutus_script(partial_witness, vec![].into())
                .unwrap();
            txb.add_reference_input(wit.reference_utxo);
            txb.add_withdrawal(withdrawal_result);
            trace!(
                "Witness: Base cost: {:?}, marginal cost {:?}, scaling factor: {}",
                wit.ex_budget,
                wit.marginal_cost,
                scaling_factor
            );
            let ex_units = wit.ex_budget + wit.marginal_cost.scale(scaling_factor);
            pending_reward_exunits.push((
                RedeemerWitnessKey::new(RedeemerTag::Reward, reward_ix as u64),
                ex_units.into(),
            ));
        }
        for (key, ex_units) in pending_reward_exunits {
            txb.set_exunits(key, ex_units);
        }
        (txb, funding_io)
    }
}

#[cfg(test)]
impl TxBlueprint {
    fn add_default_ordered_ref_for_test(&mut self, reference: OutputRef) {
        self.script_io.push((
            ScriptInputBlueprint {
                reference,
                utxo: test_output(),
                script: ScriptWitness {
                    hash: cml_crypto::ScriptHash::from([1; 28]),
                    cost: spectrum_offchain_cardano::script::ready_cost(
                        spectrum_cardano_lib::ex_units::ExUnits { mem: 0, steps: 0 },
                    ),
                },
                redeemer: spectrum_offchain_cardano::script::ready_redeemer(PlutusData::new_integer(
                    0.into(),
                )),
                required_signers: vec![].into(),
            },
            test_output(),
        ));
    }

    fn add_account_ordered_ref_for_test(&mut self, reference: OutputRef) {
        self.account_script_refs.insert(reference);
        self.add_default_ordered_ref_for_test(reference);
    }

    fn ordered_refs_for_test(&self) -> Vec<OutputRef> {
        let mut refs = self
            .script_io
            .iter()
            .map(|(input, _)| input.reference)
            .collect::<Vec<_>>();
        refs.sort_by(|lh, rh| self.cmp_script_refs(lh, rh));
        refs
    }

    fn add_script_ref_with_cost_for_test(
        &mut self,
        reference: OutputRef,
        is_account: bool,
        cost: spectrum_cardano_lib::ex_units::ExUnits,
    ) {
        if is_account {
            self.account_script_refs.insert(reference);
        }
        let cml_script = cml_chain::plutus::PlutusV2Script::new(vec![reference.index() as u8]);
        let hash = cml_script.hash();
        let utxo = TransactionOutput::new(
            cml_chain::address::Address::Enterprise(cml_chain::address::EnterpriseAddress::new(
                spectrum_cardano_lib::NetworkId::PREPROD.into(),
                Credential::new_script(hash),
            )),
            Value::from(3_000_000),
            None,
            None,
        );
        let output = TransactionOutput::new(utxo.address().clone(), Value::from(2_000_000), None, None);
        self.script_io.push((
            ScriptInputBlueprint {
                reference,
                utxo: utxo.clone(),
                script: ScriptWitness {
                    hash,
                    cost: spectrum_offchain_cardano::script::ready_cost(cost),
                },
                redeemer: spectrum_offchain_cardano::script::ready_redeemer(PlutusData::new_integer(
                    reference.index().into(),
                )),
                required_signers: if is_account {
                    vec![cml_crypto::Ed25519KeyHash::from([0xaa; 28])].into()
                } else {
                    vec![].into()
                },
            },
            output,
        ));
        self.reference_inputs.insert((
            OutputRef::new(
                cml_crypto::TransactionHash::from([reference.index() as u8; 32]),
                0,
            )
            .into(),
            TransactionOutput::new(
                test_output().address().clone(),
                Value::from(2_000_000),
                None,
                Some(cml_chain::Script::new_plutus_v2(cml_script)),
            ),
        ));
    }
}

#[cfg(test)]
fn test_output() -> TransactionOutput {
    TransactionOutput::new(
        cml_chain::address::Address::from_bech32(
            "addr1z8d70g7c58vznyye9guwagdza74x36f3uff0eyk2zwpcpx6c96rgsm7p0hmwrj8e28qny5yxwya63e8gjj8s2ugfglhsxedx9j",
        )
        .unwrap(),
        Value::from(2_000_000),
        None,
        None,
    )
}

#[cfg(test)]
fn key_payment_output(lovelace: u64) -> TransactionOutput {
    TransactionOutput::new(
        cml_chain::address::Address::Enterprise(cml_chain::address::EnterpriseAddress::new(
            spectrum_cardano_lib::NetworkId::PREPROD.into(),
            Credential::new_pub_key(cml_crypto::Ed25519KeyHash::from([0x11; 28])),
        )),
        Value::from(lovelace),
        None,
        None,
    )
}

pub struct ExecutionState {
    pub tx_blueprint: TxBlueprint,
    pub reserved_tx_fee: Lovelace,
    pub operator_interest: Lovelace,
}

impl ExecutionState {
    pub fn new() -> Self {
        Self {
            tx_blueprint: TxBlueprint::new(),
            reserved_tx_fee: 0,
            operator_interest: 0,
        }
    }

    pub fn add_tx_fee(&mut self, amount: Lovelace) {
        self.reserved_tx_fee += amount;
    }

    pub fn add_operator_interest(&mut self, amount: Lovelace) {
        self.operator_interest += amount;
    }
}

#[cfg(test)]
mod test {
    use bloom_offchain::execution_engine::funding_effect::FundingIO;
    use cml_chain::plutus::PlutusV2Script;
    use cml_chain::transaction::TransactionOutput;
    use cml_chain::Value;
    use cml_core::serialization::Deserialize;
    use cml_crypto::TransactionHash;
    use spectrum_cardano_lib::ex_units::ExUnits;
    use spectrum_cardano_lib::output::FinalizedTxOut;
    use spectrum_cardano_lib::protocol_params::constant_tx_builder;
    use spectrum_cardano_lib::transaction::TransactionOutputExtension;
    use spectrum_cardano_lib::value::ValueExtension;
    use spectrum_cardano_lib::OutputRef;
    use spectrum_offchain_cardano::creds::OperatorRewardAddress;
    use spectrum_offchain_cardano::deployment::DeployedValidatorErased;

    #[test]
    fn hash_script_cml() {
        let script = PlutusV2Script::from_cbor_bytes(&*hex::decode(SCRIPT).unwrap()).unwrap();
        let sh = script.hash();
        println!("{}", sh)
    }

    #[test]
    fn script_inputs_are_ordered_by_canonical_reference() {
        let mut blueprint = super::TxBlueprint::new();
        let pool_ref = OutputRef::new(TransactionHash::from([2; 32]), 0);
        let account_ref = OutputRef::new(TransactionHash::from([9; 32]), 0);

        blueprint.add_default_ordered_ref_for_test(pool_ref);
        blueprint.add_account_ordered_ref_for_test(account_ref);

        assert_eq!(vec![pool_ref, account_ref], blueprint.ordered_refs_for_test());
    }

    #[test]
    fn canonical_cml_spend_indexes_receive_exunits() {
        let mut blueprint = super::TxBlueprint::new();
        let pool_ref = OutputRef::new(TransactionHash::from([2; 32]), 2);
        let account_ref = OutputRef::new(TransactionHash::from([9; 32]), 1);

        blueprint.add_script_ref_with_cost_for_test(pool_ref, false, ExUnits { mem: 30, steps: 40 });
        blueprint.add_script_ref_with_cost_for_test(account_ref, true, ExUnits { mem: 10, steps: 20 });

        let operator_funding = FinalizedTxOut(
            super::test_output(),
            OutputRef::new(TransactionHash::from([0xaa; 32]), 0),
        );
        let operator_address = OperatorRewardAddress(super::test_output().address().clone());

        let (tx_builder, _) = blueprint.project_onto_builder(
            constant_tx_builder(),
            spectrum_cardano_lib::NetworkId::PREPROD,
            operator_address,
            operator_funding,
            0,
            0,
        );
        let redeemers = tx_builder
            .build_for_evaluation(
                cml_chain::builders::tx_builder::ChangeSelectionAlgo::Default,
                super::test_output().address(),
            )
            .unwrap()
            .build()
            .unwrap();
        let cml_chain::plutus::Redeemers::ArrLegacyRedeemer {
            arr_legacy_redeemer: redeemers,
            ..
        } = redeemers
        else {
            panic!("expected legacy redeemers");
        };

        assert_eq!(2, redeemers.len());
        assert_eq!(cml_chain::plutus::RedeemerTag::Spend, redeemers[0].tag);
        assert_eq!(0, redeemers[0].index);
        assert_eq!(30, redeemers[0].ex_units.mem);
        assert_eq!(40, redeemers[0].ex_units.steps);
        assert_eq!(cml_chain::plutus::RedeemerTag::Spend, redeemers[1].tag);
        assert_eq!(1, redeemers[1].index);
        assert_eq!(10, redeemers[1].ex_units.mem);
        assert_eq!(20, redeemers[1].ex_units.steps);
    }

    #[test]
    fn aleph_batch_forces_operator_funding_replacement_even_for_large_interest() {
        let mut blueprint = super::TxBlueprint::new();
        let account_ref = OutputRef::new(TransactionHash::from([0x31; 32]), 0);
        blueprint.add_script_ref_with_cost_for_test(account_ref, true, ExUnits { mem: 0, steps: 0 });
        let witness_script = cml_chain::plutus::PlutusV2Script::new(vec![0x42]);
        let witness_hash = witness_script.hash();
        let witness = DeployedValidatorErased {
            reference_utxo: cml_chain::builders::tx_builder::TransactionUnspentOutput::new(
                OutputRef::new(TransactionHash::from([0xf0; 32]), 0).into(),
                TransactionOutput::new(
                    super::test_output().address().clone(),
                    Value::from(2_000_000),
                    None,
                    Some(cml_chain::Script::new_plutus_v2(witness_script)),
                ),
            ),
            hash: witness_hash,
            ex_budget: ExUnits { mem: 0, steps: 0 },
            marginal_cost: ExUnits { mem: 0, steps: 0 },
        };
        blueprint.add_aleph_batch_intention(
            witness,
            account_ref,
            crate::orders::green::AlephAuthorizedIntention {
                intent: crate::orders::green::AlephIntention {
                    abi: crate::orders::green::AlephAccountAbi::Current,
                    target_nonce_index: 0,
                    target_nonce_value: 1,
                    leaving_asset: spectrum_cardano_lib::AssetClass::Native,
                    leaving_amount: 0,
                    arriving_asset: spectrum_cardano_lib::AssetClass::Native,
                    expected_arriving_amount: 0,
                    fee_lovelace: 0,
                    operator: [0; 28],
                },
                remainder: 0,
                auth: crate::orders::green::GreenAuth::new_sig(vec![], vec![], vec![0; 64], vec![]).unwrap(),
            },
        );

        let operator_funding = FinalizedTxOut(
            super::key_payment_output(3_000_000),
            OutputRef::new(TransactionHash::from([0x40; 32]), 0),
        );
        let operator_address = OperatorRewardAddress(super::key_payment_output(3_000_000).address().clone());

        let (_, funding_io) = blueprint.project_onto_builder(
            constant_tx_builder(),
            spectrum_cardano_lib::NetworkId::PREPROD,
            operator_address,
            operator_funding,
            2_000_000,
            spectrum_offchain_cardano::constants::MIN_SAFE_LOVELACE_VALUE,
        );

        assert!(matches!(funding_io, FundingIO::Replaced(_, _)));
    }

    #[test]
    fn aleph_batch_places_account_output_before_funding_and_other_script_outputs() {
        let mut blueprint = super::TxBlueprint::new();
        let account_ref = OutputRef::new(TransactionHash::from([0x10; 32]), 0);
        let funding_ref = OutputRef::new(TransactionHash::from([0x20; 32]), 0);
        let pool_ref = OutputRef::new(TransactionHash::from([0x31; 32]), 0);

        blueprint.add_script_ref_with_cost_for_test(pool_ref, false, ExUnits { mem: 0, steps: 0 });
        blueprint.add_script_ref_with_cost_for_test(account_ref, true, ExUnits { mem: 0, steps: 0 });

        let witness_script = cml_chain::plutus::PlutusV2Script::new(vec![0x42]);
        let witness = DeployedValidatorErased {
            reference_utxo: cml_chain::builders::tx_builder::TransactionUnspentOutput::new(
                OutputRef::new(TransactionHash::from([0xf0; 32]), 0).into(),
                TransactionOutput::new(
                    super::test_output().address().clone(),
                    Value::from(2_000_000),
                    None,
                    Some(cml_chain::Script::new_plutus_v2(witness_script.clone())),
                ),
            ),
            hash: witness_script.hash(),
            ex_budget: ExUnits { mem: 0, steps: 0 },
            marginal_cost: ExUnits { mem: 0, steps: 0 },
        };
        blueprint.add_aleph_batch_intention(
            witness,
            account_ref,
            crate::orders::green::AlephAuthorizedIntention {
                intent: crate::orders::green::AlephIntention {
                    abi: crate::orders::green::AlephAccountAbi::Current,
                    target_nonce_index: 0,
                    target_nonce_value: 1,
                    leaving_asset: spectrum_cardano_lib::AssetClass::Native,
                    leaving_amount: 0,
                    arriving_asset: spectrum_cardano_lib::AssetClass::Native,
                    expected_arriving_amount: 0,
                    fee_lovelace: 0,
                    operator: [0; 28],
                },
                remainder: 0,
                auth: crate::orders::green::GreenAuth::new_sig(vec![], vec![], vec![0; 64], vec![]).unwrap(),
            },
        );

        let operator_funding = FinalizedTxOut(super::key_payment_output(3_000_000), funding_ref);
        let operator_address = OperatorRewardAddress(super::key_payment_output(3_000_000).address().clone());
        let (tx_builder, funding_io) = blueprint.project_onto_builder(
            constant_tx_builder(),
            spectrum_cardano_lib::NetworkId::PREPROD,
            operator_address,
            operator_funding,
            0,
            spectrum_offchain_cardano::constants::MIN_SAFE_LOVELACE_VALUE,
        );

        assert!(matches!(funding_io, FundingIO::Replaced(_, _)));

        let inputs = tx_builder.get_inputs();
        assert_eq!(3, inputs.len());
        assert_eq!(account_ref, inputs[0].input.clone().into());
        assert_eq!(funding_ref, inputs[1].input.clone().into());
        assert_eq!(pool_ref, inputs[2].input.clone().into());

        let draft_body = tx_builder
            .build_for_evaluation(
                cml_chain::builders::tx_builder::ChangeSelectionAlgo::Default,
                super::key_payment_output(3_000_000).address(),
            )
            .unwrap()
            .draft_body();
        let body_inputs = draft_body
            .inputs
            .iter()
            .map(|input| OutputRef::new(input.transaction_id, input.index))
            .collect::<Vec<_>>();
        assert_eq!(vec![account_ref, funding_ref, pool_ref], body_inputs);

        let outputs = tx_builder.get_outputs();

        assert_eq!(3, outputs.len());
        assert_eq!(
            super::key_payment_output(3_000_000).address(),
            outputs[1].address()
        );
        assert_eq!(
            Some(3_000_000 - spectrum_offchain_cardano::constants::MIN_SAFE_LOVELACE_VALUE),
            outputs[1]
                .value()
                .amount_of(spectrum_cardano_lib::AssetClass::Native)
        );
        assert_ne!(
            super::key_payment_output(3_000_000).address(),
            outputs[0].address()
        );
        assert_ne!(
            super::key_payment_output(3_000_000).address(),
            outputs[2].address()
        );
    }

    #[test]
    fn aleph_batch_keeps_large_operator_reward_separate_from_funding_barrier() {
        let mut blueprint = super::TxBlueprint::new();
        let account_ref = OutputRef::new(TransactionHash::from([0x10; 32]), 0);
        let funding_ref = OutputRef::new(TransactionHash::from([0x20; 32]), 0);
        let pool_ref = OutputRef::new(TransactionHash::from([0x31; 32]), 0);

        blueprint.add_script_ref_with_cost_for_test(pool_ref, false, ExUnits { mem: 0, steps: 0 });
        blueprint.add_script_ref_with_cost_for_test(account_ref, true, ExUnits { mem: 0, steps: 0 });

        let witness_script = cml_chain::plutus::PlutusV2Script::new(vec![0x42]);
        let witness = DeployedValidatorErased {
            reference_utxo: cml_chain::builders::tx_builder::TransactionUnspentOutput::new(
                OutputRef::new(TransactionHash::from([0xf0; 32]), 0).into(),
                TransactionOutput::new(
                    super::test_output().address().clone(),
                    Value::from(2_000_000),
                    None,
                    Some(cml_chain::Script::new_plutus_v2(witness_script.clone())),
                ),
            ),
            hash: witness_script.hash(),
            ex_budget: ExUnits { mem: 0, steps: 0 },
            marginal_cost: ExUnits { mem: 0, steps: 0 },
        };
        blueprint.add_aleph_batch_intention(
            witness,
            account_ref,
            crate::orders::green::AlephAuthorizedIntention {
                intent: crate::orders::green::AlephIntention {
                    abi: crate::orders::green::AlephAccountAbi::Current,
                    target_nonce_index: 0,
                    target_nonce_value: 1,
                    leaving_asset: spectrum_cardano_lib::AssetClass::Native,
                    leaving_amount: 0,
                    arriving_asset: spectrum_cardano_lib::AssetClass::Native,
                    expected_arriving_amount: 0,
                    fee_lovelace: 0,
                    operator: [0; 28],
                },
                remainder: 0,
                auth: crate::orders::green::GreenAuth::new_sig(vec![], vec![], vec![0; 64], vec![]).unwrap(),
            },
        );

        let funding_output = super::key_payment_output(3_000_000);
        let reward_output = super::test_output();
        let operator_funding = FinalizedTxOut(funding_output.clone(), funding_ref);
        let operator_address = OperatorRewardAddress(reward_output.address().clone());
        let (tx_builder, funding_io) = blueprint.project_onto_builder(
            constant_tx_builder(),
            spectrum_cardano_lib::NetworkId::PREPROD,
            operator_address,
            operator_funding,
            2_000_000,
            spectrum_offchain_cardano::constants::MIN_SAFE_LOVELACE_VALUE,
        );

        let FundingIO::Replaced(_, funding_barrier_output) = funding_io else {
            panic!("expected funding replacement");
        };
        assert_eq!(
            Some(3_000_000 - spectrum_offchain_cardano::constants::MIN_SAFE_LOVELACE_VALUE),
            funding_barrier_output
                .value()
                .amount_of(spectrum_cardano_lib::AssetClass::Native)
        );

        let outputs = tx_builder.get_outputs();
        assert_eq!(4, outputs.len());
        assert_eq!(funding_output.address(), outputs[1].address());
        assert_eq!(
            Some(3_000_000 - spectrum_offchain_cardano::constants::MIN_SAFE_LOVELACE_VALUE),
            outputs[1]
                .value()
                .amount_of(spectrum_cardano_lib::AssetClass::Native)
        );
        assert_eq!(reward_output.address(), outputs[3].address());
        assert_eq!(&Value::from(2_000_000), outputs[3].value());
    }

    #[test]
    fn aleph_batch_folds_small_operator_reward_into_forced_funding_output() {
        let mut blueprint = super::TxBlueprint::new();
        let account_ref = OutputRef::new(TransactionHash::from([0x10; 32]), 0);
        let funding_ref = OutputRef::new(TransactionHash::from([0x20; 32]), 0);

        blueprint.add_script_ref_with_cost_for_test(account_ref, true, ExUnits { mem: 0, steps: 0 });

        let witness_script = cml_chain::plutus::PlutusV2Script::new(vec![0x42]);
        let witness = DeployedValidatorErased {
            reference_utxo: cml_chain::builders::tx_builder::TransactionUnspentOutput::new(
                OutputRef::new(TransactionHash::from([0xf0; 32]), 0).into(),
                TransactionOutput::new(
                    super::test_output().address().clone(),
                    Value::from(2_000_000),
                    None,
                    Some(cml_chain::Script::new_plutus_v2(witness_script.clone())),
                ),
            ),
            hash: witness_script.hash(),
            ex_budget: ExUnits { mem: 0, steps: 0 },
            marginal_cost: ExUnits { mem: 0, steps: 0 },
        };
        blueprint.add_aleph_batch_intention(
            witness,
            account_ref,
            crate::orders::green::AlephAuthorizedIntention {
                intent: crate::orders::green::AlephIntention {
                    abi: crate::orders::green::AlephAccountAbi::Current,
                    target_nonce_index: 0,
                    target_nonce_value: 1,
                    leaving_asset: spectrum_cardano_lib::AssetClass::Native,
                    leaving_amount: 0,
                    arriving_asset: spectrum_cardano_lib::AssetClass::Native,
                    expected_arriving_amount: 0,
                    fee_lovelace: 0,
                    operator: [0; 28],
                },
                remainder: 0,
                auth: crate::orders::green::GreenAuth::new_sig(vec![], vec![], vec![0; 64], vec![]).unwrap(),
            },
        );

        let operator_interest = 678_702;
        let funding_output = super::key_payment_output(3_000_000);
        let operator_funding = FinalizedTxOut(funding_output.clone(), funding_ref);
        let operator_address = OperatorRewardAddress(super::test_output().address().clone());
        let (tx_builder, funding_io) = blueprint.project_onto_builder(
            constant_tx_builder(),
            spectrum_cardano_lib::NetworkId::PREPROD,
            operator_address,
            operator_funding,
            operator_interest,
            spectrum_offchain_cardano::constants::MIN_SAFE_LOVELACE_VALUE,
        );

        let FundingIO::Replaced(_, funding_barrier_output) = funding_io else {
            panic!("expected funding replacement");
        };
        assert_eq!(
            Some(
                3_000_000 - spectrum_offchain_cardano::constants::MIN_SAFE_LOVELACE_VALUE + operator_interest
            ),
            funding_barrier_output
                .value()
                .amount_of(spectrum_cardano_lib::AssetClass::Native)
        );

        let outputs = tx_builder.get_outputs();
        assert_eq!(2, outputs.len());
        assert_eq!(
            Some(
                3_000_000 - spectrum_offchain_cardano::constants::MIN_SAFE_LOVELACE_VALUE + operator_interest
            ),
            outputs[1]
                .value()
                .amount_of(spectrum_cardano_lib::AssetClass::Native)
        );
    }

    #[test]
    fn aleph_batch_adds_operator_to_extra_signatories() {
        let mut blueprint = super::TxBlueprint::new();
        let account_ref = OutputRef::new(TransactionHash::from([0x10; 32]), 0);
        let funding_ref = OutputRef::new(TransactionHash::from([0x20; 32]), 0);
        let operator = cml_crypto::Ed25519KeyHash::from([0xaa; 28]);

        blueprint.add_script_ref_with_cost_for_test(account_ref, true, ExUnits { mem: 0, steps: 0 });

        let witness_script = cml_chain::plutus::PlutusV2Script::new(vec![0x42]);
        let witness = DeployedValidatorErased {
            reference_utxo: cml_chain::builders::tx_builder::TransactionUnspentOutput::new(
                OutputRef::new(TransactionHash::from([0xf0; 32]), 0).into(),
                TransactionOutput::new(
                    super::test_output().address().clone(),
                    Value::from(2_000_000),
                    None,
                    Some(cml_chain::Script::new_plutus_v2(witness_script.clone())),
                ),
            ),
            hash: witness_script.hash(),
            ex_budget: ExUnits { mem: 0, steps: 0 },
            marginal_cost: ExUnits { mem: 0, steps: 0 },
        };
        blueprint.add_aleph_batch_intention(
            witness,
            account_ref,
            crate::orders::green::AlephAuthorizedIntention {
                intent: crate::orders::green::AlephIntention {
                    abi: crate::orders::green::AlephAccountAbi::Current,
                    target_nonce_index: 0,
                    target_nonce_value: 1,
                    leaving_asset: spectrum_cardano_lib::AssetClass::Native,
                    leaving_amount: 0,
                    arriving_asset: spectrum_cardano_lib::AssetClass::Native,
                    expected_arriving_amount: 0,
                    fee_lovelace: 0,
                    operator: [0xaa; 28],
                },
                remainder: 0,
                auth: crate::orders::green::GreenAuth::new_sig(vec![], vec![], vec![0; 64], vec![]).unwrap(),
            },
        );

        let (tx_builder, _) = blueprint.project_onto_builder(
            constant_tx_builder(),
            spectrum_cardano_lib::NetworkId::PREPROD,
            OperatorRewardAddress(super::test_output().address().clone()),
            FinalizedTxOut(super::key_payment_output(3_000_000), funding_ref),
            0,
            0,
        );
        let draft_body = tx_builder
            .build_for_evaluation(
                cml_chain::builders::tx_builder::ChangeSelectionAlgo::Default,
                super::key_payment_output(3_000_000).address(),
            )
            .unwrap()
            .draft_body();

        assert_eq!(
            Some(vec![operator]),
            draft_body
                .required_signers
                .map(|signers| signers.into_iter().collect())
        );
    }

    const SCRIPT: &str = "59041459041101000033232323232323232322222323253330093232533300b003132323300100100222533301100114a02646464a66602266ebc0380045288998028028011808801180a80118098009bab301030113011301130113011301130090011323232533300e3370e900118068008991919299980899b8748000c0400044c8c8c8c8c94ccc0594ccc05802c400852808008a503375e601860260046034603660366036603660366036603660366036602602266ebcc020c048c020c048008c020c048004c060dd6180c180c980c9808804980b80098078008b19191980080080111299980b0008a60103d87a80001323253330153375e6018602600400c266e952000330190024bd70099802002000980d001180c0009bac3007300e0063014001300c001163001300b0072301230130013322323300100100322533301200114a026464a66602266e3c008014528899802002000980b0011bae3014001375860206022602260226022602260226022602260120026eb8c040c044c044c044c044c044c044c044c044c044c044c02401cc004c0200108c03c004526136563370e900118049baa003323232533300a3370e90000008991919191919191919191919191919191919191919191919299981298140010991919191924c646600200200c44a6660560022930991980180198178011bae302d0013253330263370e9000000899191919299981698180010991924c64a66605866e1d20000011323253330313034002132498c94ccc0bccdc3a400000226464a666068606e0042649318150008b181a80098168010a99981799b87480080044c8c8c8c8c8c94ccc0e0c0ec00852616375a607200260720046eb4c0dc004c0dc008dd6981a80098168010b18168008b181900098150018a99981619b874800800454ccc0bcc0a800c5261616302a002302300316302e001302e002302c00130240091630240083253330253370e9000000899191919299981618178010a4c2c6eb4c0b4004c0b4008dd6981580098118060b1811805980d806180d0098b1bac30260013026002375c60480026048004604400260440046eb4c080004c080008c078004c078008c070004c070008dd6980d000980d0011bad30180013018002375a602c002602c004602800260280046eb8c048004c048008dd7180800098040030b1804002919299980519b87480000044c8c8c8c94ccc044c05000852616375c602400260240046eb8c040004c02000858c0200048c94ccc024cdc3a400000226464a66601c60220042930b1bae300f0013007002153330093370e900100089919299980718088010a4c2c6eb8c03c004c01c00858c01c0048c014dd5000918019baa0015734aae7555cf2ab9f5740ae855d126126d8799fd87a9f581ce7feddaece029040c973d5bf806fa9497314c0a63dfdc47fc47ac557ffff0001";
}
