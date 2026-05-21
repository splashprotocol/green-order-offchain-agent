use cml_chain::plutus::PlutusData;
use cml_chain::transaction::TransactionOutput;
use cml_core::serialization::RawBytesEncoding;
use cml_crypto::Ed25519KeyHash;
use log::trace;

use bloom_offchain::execution_engine::batch_exec::BatchExec;
use bloom_offchain::execution_engine::bundled::Bundled;
use bloom_offchain::execution_engine::execution_effect::ExecutionEff;
use bloom_offchain::execution_engine::liquidity_book::core::{Make, Next, Take, Trans};
use bloom_offchain::execution_engine::liquidity_book::market_taker::MarketTaker;
use spectrum_cardano_lib::output::FinalizedTxOut;
use spectrum_cardano_lib::plutus_data::IntoPlutusData;
use spectrum_cardano_lib::transaction::TransactionOutputExtension;
use spectrum_cardano_lib::{AssetClass, NetworkId};
use spectrum_offchain::domain::Has;
use spectrum_offchain_cardano::creds::OperatorCred;
use spectrum_offchain_cardano::data::balance_pool::{BalancePool, BalancePoolRedeemer};
use spectrum_offchain_cardano::data::cfmm_pool::{CFMMPoolRedeemer, ConstFnPool};
use spectrum_offchain_cardano::data::pool::{AnyPool, CFMMPoolAction, PoolAssetMapping};
use spectrum_offchain_cardano::data::quadratic_pool::QuadraticPoolVer::V1T2T;
use spectrum_offchain_cardano::data::quadratic_pool::{QuadraticPool, QuadraticPoolRedeemer};
use spectrum_offchain_cardano::data::stable_pool_t2t::{StablePoolRedeemer, StablePoolT2T};
use spectrum_offchain_cardano::data::{balance_pool, quadratic_pool, stable_pool_t2t};
use spectrum_offchain_cardano::deployment::ProtocolValidator::{
    BalanceFnPoolV1, BalanceFnPoolV2, ConstFnPoolFeeSwitch, ConstFnPoolFeeSwitchBiDirFee,
    ConstFnPoolFeeSwitchV2, ConstFnPoolV1, ConstFnPoolV2, DegenQuadraticPoolV1, DegenQuadraticPoolV1T2T,
    GridOrderNative, InstantOrderV1, InstantOrderWitnessV1, LimitOrderV1, LimitOrderWitnessV1, RoyaltyPoolV1,
    RoyaltyPoolV1LedgerFixed, RoyaltyPoolV2, StableFnPoolT2T,
};
use spectrum_offchain_cardano::deployment::{
    DeployedScriptInfo, DeployedValidator, DeployedValidatorErased, RequiresValidator,
};
use spectrum_offchain_cardano::script::{
    delayed_cost, delayed_redeemer, ready_cost, ready_redeemer, ScriptWitness,
};

use crate::execution_engine::execution_state::{ExecutionState, ScriptInputBlueprint};
use crate::orders::adhoc::{AdhocFeeStructure, AdhocOrder};
use crate::orders::green::{
    apply_full_fill_to_account_output, AlephAccountAction, AlephAccountUtxo, AlephAuthorizedIntention,
    GreenAccountLookup, GreenAuth, GreenIntention, GreenOrder, GreenPartialPolicy, GreenStorePlanner,
    ALEPH_ACCOUNT_VALIDATOR, ALEPH_BATCH_WITNESS_VALIDATOR,
};
use crate::orders::grid::GridOrder;
use crate::orders::limit::LimitOrder;
use crate::orders::{grid, instant, limit, AnyOrder};
use crate::pools::classified::ClassifiedPool;

fn aleph_delegate_index_for_witness(allowlist: &[[u8; 28]], witness_hash: [u8; 28]) -> u64 {
    allowlist
        .iter()
        .position(|hash| *hash == witness_hash)
        .map(|index| index as u64)
        .expect("Aleph batch witness script is not present in account allowlist")
}
use crate::pools::royalty_v1::RoyaltyV1PoolOnly;

/// Magnet for local instances.
#[repr(transparent)]
pub struct Magnet<T>(pub T);

pub type EffectPreview<T> = ExecutionEff<Bundled<T, TransactionOutput>, Bundled<T, FinalizedTxOut>>;
pub type FinalizedEffect<T> = ExecutionEff<Bundled<T, FinalizedTxOut>, Bundled<T, FinalizedTxOut>>;

impl<Ctx> BatchExec<ExecutionState, EffectPreview<AnyOrder>, Ctx> for Magnet<Take<AnyOrder, FinalizedTxOut>>
where
    Ctx: Has<NetworkId>
        + Has<OperatorCred>
        + Has<DeployedValidator<{ GridOrderNative as u8 }>>
        + Has<DeployedValidator<{ LimitOrderV1 as u8 }>>
        + Has<DeployedValidator<{ LimitOrderWitnessV1 as u8 }>>,
{
    fn exec(self, state: ExecutionState, context: Ctx) -> (ExecutionState, EffectPreview<AnyOrder>, Ctx) {
        match self {
            Magnet(Trans {
                target: Bundled(AnyOrder::Limit(o), src),
                result,
            }) => {
                let (st, res, ctx) = Magnet(Trans {
                    target: Bundled(o, src),
                    result: result.map_succ(|ord| match ord {
                        AnyOrder::Limit(o2) => o2,
                        _ => unreachable!(),
                    }),
                })
                .exec(state, context);
                (
                    st,
                    res.bimap(|u| u.map(AnyOrder::Limit), |e| e.map(AnyOrder::Limit)),
                    ctx,
                )
            }
            Magnet(Trans {
                target: Bundled(AnyOrder::Grid(o), src),
                result,
            }) => {
                let (st, res, ctx) = Magnet(Trans {
                    target: Bundled(o, src),
                    result: result.map_succ(|ord| match ord {
                        AnyOrder::Grid(o2) => o2,
                        _ => unreachable!(),
                    }),
                })
                .exec(state, context);
                (
                    st,
                    res.bimap(|u| u.map(AnyOrder::Grid), |e| e.map(AnyOrder::Grid)),
                    ctx,
                )
            }
        }
    }
}

impl<Ctx> BatchExec<ExecutionState, EffectPreview<LimitOrder>, Ctx>
    for Magnet<Take<LimitOrder, FinalizedTxOut>>
where
    Ctx: Has<NetworkId>
        + Has<OperatorCred>
        + Has<DeployedValidator<{ LimitOrderV1 as u8 }>>
        + Has<DeployedValidator<{ LimitOrderWitnessV1 as u8 }>>,
{
    fn exec(
        self,
        mut state: ExecutionState,
        context: Ctx,
    ) -> (ExecutionState, EffectPreview<LimitOrder>, Ctx) {
        let Magnet(trans) = self;
        trace!("Running transition: {}", trans);
        let removed_input = trans.removed_input();
        let added_output = trans.added_output();
        let consumed_budget = trans.consumed_budget();
        let consumed_fee = trans.consumed_fee();
        trace!(
            "LimitOrder::exec(removed_input={}, added_output={}, consumed_budget={}, consumed_fee={})",
            removed_input,
            added_output,
            consumed_budget,
            consumed_fee
        );
        let Trans {
            target: Bundled(ord, FinalizedTxOut(consumed_out, in_ref)),
            result,
        } = trans;
        let DeployedValidatorErased {
            reference_utxo,
            hash,
            ex_budget,
            ..
        } = context
            .select::<DeployedValidator<{ LimitOrderV1 as u8 }>>()
            .erased();
        let input = ScriptInputBlueprint {
            reference: in_ref,
            utxo: consumed_out.clone(),
            script: ScriptWitness {
                hash,
                cost: ready_cost(ex_budget),
            },
            redeemer: ready_redeemer(limit::EXEC_REDEEMER),
            required_signers: if ord.requires_executor_sig {
                vec![Ed25519KeyHash::from(context.select::<OperatorCred>())].into()
            } else {
                vec![].into()
            },
        };
        let mut candidate = consumed_out.clone();
        // Subtract budget + fee used to facilitate execution.
        candidate.sub_asset(ord.fee_asset, consumed_budget + consumed_fee);
        // Subtract tradable input used in exchange.
        candidate.sub_asset(ord.input_asset, removed_input);
        // Add output resulted from exchange.
        candidate.add_asset(ord.output_asset, added_output);
        let consumed_bundle = Bundled(ord, FinalizedTxOut(consumed_out, in_ref));
        let (residual_order, effect) = match result {
            Next::Succ(next) => {
                if let Some(data) = candidate.data_mut() {
                    limit::unsafe_update_datum(data, next.input_amount, next.fee);
                }
                (
                    candidate.clone(),
                    ExecutionEff::Updated(consumed_bundle, Bundled(next, candidate)),
                )
            }
            Next::Term(_) => {
                candidate.null_datum();
                candidate.update_address(ord.redeemer_address.to_address(context.select::<NetworkId>()));
                (candidate, ExecutionEff::Eliminated(consumed_bundle))
            }
        };
        let witness = context.select::<DeployedValidator<{ LimitOrderWitnessV1 as u8 }>>();
        state.add_tx_fee(consumed_budget);
        state.add_operator_interest(consumed_fee);
        state
            .tx_blueprint
            .add_witness(witness.erased(), PlutusData::new_list(vec![]));
        state.tx_blueprint.add_io(input, residual_order);
        state.tx_blueprint.add_ref_input(reference_utxo);
        (state, effect, context)
    }
}

impl<Ctx> BatchExec<ExecutionState, EffectPreview<AdhocOrder>, Ctx>
    for Magnet<Take<AdhocOrder, FinalizedTxOut>>
where
    Ctx: Has<NetworkId>
        + Has<OperatorCred>
        + Has<DeployedValidator<{ InstantOrderV1 as u8 }>>
        + Has<DeployedValidator<{ InstantOrderWitnessV1 as u8 }>>
        + Has<AdhocFeeStructure>,
{
    fn exec(
        self,
        mut state: ExecutionState,
        context: Ctx,
    ) -> (ExecutionState, EffectPreview<AdhocOrder>, Ctx) {
        let Magnet(trans) = self;
        trace!("Running transition: {}", trans);
        let removed_input = trans.removed_input();
        let added_output = trans.added_output();
        let consumed_budget = trans.consumed_budget();
        let consumed_fee = trans.consumed_fee();
        trace!(
            "AdhocOrder::exec(removed_input={}, added_output={}, consumed_budget={}, consumed_fee={})",
            removed_input,
            added_output,
            consumed_budget,
            consumed_fee
        );
        let Trans {
            target: Bundled(AdhocOrder(ord, adhoc_fee_input), FinalizedTxOut(consumed_utxo, in_ref)),
            result,
        } = trans;
        let DeployedValidatorErased {
            reference_utxo,
            hash,
            ex_budget,
            ..
        } = context
            .select::<DeployedValidator<{ InstantOrderV1 as u8 }>>()
            .erased();
        let input = ScriptInputBlueprint {
            reference: in_ref,
            utxo: consumed_utxo.clone(),
            script: ScriptWitness {
                hash,
                cost: ready_cost(ex_budget),
            },
            redeemer: ready_redeemer(instant::EXEC_REDEEMER),
            required_signers: vec![Ed25519KeyHash::from(context.select::<OperatorCred>())].into(),
        };
        let full_adhoc_fee = match (ord.input_asset, ord.output_asset) {
            (AssetClass::Native, _) => adhoc_fee_input,
            (_, AssetClass::Native) => context.select::<AdhocFeeStructure>().fee(added_output),
            _ => 0,
        };
        let proportional_fee = (full_adhoc_fee as u128 * removed_input as u128 / ord.input() as u128) as u64;
        trace!(
            "consumed_budget: {}, consumed_fee: {}, full_adhoc_fee: {}, proportional_fee: {}",
            consumed_budget,
            consumed_fee,
            full_adhoc_fee,
            proportional_fee
        );
        let mut candidate = consumed_utxo.clone();
        // Subtract tradable input used in exchange.
        candidate.sub_asset(ord.input_asset, removed_input);
        // Add output resulted from exchange.
        candidate.add_asset(ord.output_asset, added_output);
        // Subtract budget + fee used to facilitate execution + adhoc fee.
        candidate.sub_asset(ord.fee_asset, consumed_budget + consumed_fee + proportional_fee);
        let consumed_bundle = Bundled(
            AdhocOrder(ord, adhoc_fee_input),
            FinalizedTxOut(consumed_utxo, in_ref),
        );
        let (residual_order, effect) = match result {
            Next::Term(_) => {
                candidate.null_datum();
                candidate.update_address(ord.redeemer_address.to_address(context.select::<NetworkId>()));
                (candidate, ExecutionEff::Eliminated(consumed_bundle))
            }
            // Adhoc orders must always be terminated.
            Next::Succ(AdhocOrder(_, _)) => {
                unreachable!()
            }
        };
        let witness = context.select::<DeployedValidator<{ InstantOrderWitnessV1 as u8 }>>();
        state.add_tx_fee(consumed_budget);
        state.add_operator_interest(consumed_fee + proportional_fee);
        state
            .tx_blueprint
            .add_witness(witness.erased(), PlutusData::new_list(vec![]));
        state.tx_blueprint.add_io(input, residual_order);
        state.tx_blueprint.add_ref_input(reference_utxo);
        (state, effect, context)
    }
}

impl<Ctx> BatchExec<ExecutionState, EffectPreview<GridOrder>, Ctx> for Magnet<Take<GridOrder, FinalizedTxOut>>
where
    Ctx: Has<NetworkId> + Has<DeployedValidator<{ GridOrderNative as u8 }>>,
{
    fn exec(
        self,
        mut state: ExecutionState,
        context: Ctx,
    ) -> (ExecutionState, EffectPreview<GridOrder>, Ctx) {
        let Magnet(trans) = self;
        trace!("Running transition: {}", trans);
        let removed_input = trans.removed_input();
        let added_output = trans.added_output();
        let consumed_budget = trans.consumed_budget();
        let consumed_fee = trans.consumed_fee();
        trace!(
            "GridOrder::exec(removed_input={}, added_output={}, consumed_budget={}, consumed_fee={})",
            removed_input,
            added_output,
            consumed_budget,
            consumed_fee
        );
        let Trans {
            target: Bundled(ord, FinalizedTxOut(consumed_out, in_ref)),
            result,
        } = trans;
        let DeployedValidatorErased {
            reference_utxo,
            hash,
            ex_budget,
            ..
        } = context
            .select::<DeployedValidator<{ GridOrderNative as u8 }>>()
            .erased();
        let input = ScriptInputBlueprint {
            reference: in_ref,
            utxo: consumed_out.clone(),
            script: ScriptWitness {
                hash,
                cost: ready_cost(ex_budget),
            },
            redeemer: ready_redeemer(limit::EXEC_REDEEMER),
            required_signers: vec![].into(),
        };
        let mut candidate = consumed_out.clone();
        let (input_asset, output_asset) = ord.absolute_io();
        // Subtract budget + fee used to facilitate execution.
        candidate.sub_asset(AssetClass::Native, consumed_budget + consumed_fee);
        // Subtract tradable input used in exchange.
        candidate.sub_asset(input_asset, removed_input);
        // Add output resulted from exchange.
        candidate.add_asset(output_asset, added_output);
        let consumed_bundle = Bundled(ord, FinalizedTxOut(consumed_out, in_ref));
        let (residual_order, effect) = match result {
            Next::Succ(next) => {
                if let Some(data) = candidate.data_mut() {
                    grid::unsafe_update_datum(data, next.quote_offer, next.price, next.side);
                }
                (
                    candidate.clone(),
                    ExecutionEff::Updated(consumed_bundle, Bundled(next, candidate)),
                )
            }
            Next::Term(_) => {
                candidate.null_datum();
                candidate.update_address(ord.redeemer_address.to_address(context.select::<NetworkId>()));
                (candidate, ExecutionEff::Eliminated(consumed_bundle))
            }
        };
        state.add_tx_fee(consumed_budget);
        state.add_operator_interest(consumed_fee);
        state.tx_blueprint.add_io(input, residual_order);
        state.tx_blueprint.add_ref_input(reference_utxo);
        (state, effect, context)
    }
}

/// Batch execution routing for [AnyPool].
impl<Ctx> BatchExec<ExecutionState, EffectPreview<AnyPool>, Ctx> for Magnet<Make<AnyPool, FinalizedTxOut>>
where
    Ctx: Has<DeployedValidator<{ ConstFnPoolV1 as u8 }>>
        + Has<DeployedValidator<{ ConstFnPoolV2 as u8 }>>
        + Has<DeployedValidator<{ ConstFnPoolFeeSwitch as u8 }>>
        + Has<DeployedValidator<{ ConstFnPoolFeeSwitchV2 as u8 }>>
        + Has<DeployedValidator<{ ConstFnPoolFeeSwitchBiDirFee as u8 }>>
        + Has<DeployedValidator<{ BalanceFnPoolV1 as u8 }>>
        + Has<DeployedValidator<{ BalanceFnPoolV2 as u8 }>>
        + Has<DeployedValidator<{ StableFnPoolT2T as u8 }>>
        + Has<DeployedValidator<{ RoyaltyPoolV1 as u8 }>>
        + Has<DeployedValidator<{ RoyaltyPoolV1LedgerFixed as u8 }>>
        + Has<DeployedValidator<{ RoyaltyPoolV2 as u8 }>>,
{
    fn exec(self, state: ExecutionState, context: Ctx) -> (ExecutionState, EffectPreview<AnyPool>, Ctx) {
        match self.0 {
            Trans {
                target: Bundled(AnyPool::PureCFMM(p), src),
                result: Next::Succ(AnyPool::PureCFMM(p2)),
            } => {
                let (st, res, ctx) = Magnet(Trans {
                    target: Bundled(p, src),
                    result: Next::Succ(p2),
                })
                .exec(state, context);
                (
                    st,
                    res.bimap(|c| c.map(AnyPool::PureCFMM), |p| p.map(AnyPool::PureCFMM)),
                    ctx,
                )
            }
            Trans {
                target: Bundled(AnyPool::BalancedCFMM(p), src),
                result: Next::Succ(AnyPool::BalancedCFMM(p2)),
            } => {
                let (st, res, ctx) = Magnet(Trans {
                    target: Bundled(p, src),
                    result: Next::Succ(p2),
                })
                .exec(state, context);
                (
                    st,
                    res.bimap(|c| c.map(AnyPool::BalancedCFMM), |p| p.map(AnyPool::BalancedCFMM)),
                    ctx,
                )
            }
            Trans {
                target: Bundled(AnyPool::StableCFMM(p), src),
                result: Next::Succ(AnyPool::StableCFMM(p2)),
            } => {
                let (st, res, ctx) = Magnet(Trans {
                    target: Bundled(p, src),
                    result: Next::Succ(p2),
                })
                .exec(state, context);
                (
                    st,
                    res.bimap(|c| c.map(AnyPool::StableCFMM), |p| p.map(AnyPool::StableCFMM)),
                    ctx,
                )
            }
            _ => unreachable!(),
        }
    }
}

impl<Ctx> BatchExec<ExecutionState, EffectPreview<ClassifiedPool>, Ctx>
    for Magnet<Make<ClassifiedPool, FinalizedTxOut>>
where
    Ctx: Has<DeployedValidator<{ ConstFnPoolV1 as u8 }>>
        + Has<DeployedValidator<{ ConstFnPoolV2 as u8 }>>
        + Has<DeployedValidator<{ ConstFnPoolFeeSwitch as u8 }>>
        + Has<DeployedValidator<{ ConstFnPoolFeeSwitchV2 as u8 }>>
        + Has<DeployedValidator<{ ConstFnPoolFeeSwitchBiDirFee as u8 }>>
        + Has<DeployedValidator<{ BalanceFnPoolV1 as u8 }>>
        + Has<DeployedValidator<{ BalanceFnPoolV2 as u8 }>>
        + Has<DeployedValidator<{ StableFnPoolT2T as u8 }>>
        + Has<DeployedValidator<{ RoyaltyPoolV1 as u8 }>>
        + Has<DeployedValidator<{ RoyaltyPoolV1LedgerFixed as u8 }>>
        + Has<DeployedValidator<{ RoyaltyPoolV2 as u8 }>>,
{
    fn exec(
        self,
        mut state: ExecutionState,
        context: Ctx,
    ) -> (ExecutionState, EffectPreview<ClassifiedPool>, Ctx) {
        let Trans {
            target: Bundled(pool, src),
            result,
        } = self.0;
        let next_pool = match result {
            Next::Succ(next_pool) => next_pool,
            Next::Term(_) => unreachable!("Splash pool trades do not terminate pools"),
        };
        let pending_operator_fee = next_pool.pending_operator_fee;
        let (state_after_pool, effect, context) = Magnet(Trans {
            target: Bundled(pool.inner, src),
            result: Next::Succ(next_pool.inner),
        })
        .exec(state, context);

        state = state_after_pool;
        state.add_operator_interest(pending_operator_fee);
        let consumed_next_pool = ClassifiedPool {
            pending_operator_fee: 0,
            ..next_pool
        };
        (
            state,
            effect.bimap(
                |updated| updated.map(|_| consumed_next_pool),
                |consumed| consumed.map(|_| pool),
            ),
            context,
        )
    }
}

impl<Ctx> BatchExec<ExecutionState, EffectPreview<GreenOrder>, Ctx>
    for Magnet<Take<GreenOrder, FinalizedTxOut>>
where
    Ctx: GreenAccountLookup
        + GreenStorePlanner
        + GreenPartialPolicy
        + Has<OperatorCred>
        + Has<DeployedValidator<{ ALEPH_ACCOUNT_VALIDATOR }>>
        + Has<DeployedValidator<{ ALEPH_BATCH_WITNESS_VALIDATOR }>>
        + Has<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>>,
{
    fn exec(
        self,
        mut state: ExecutionState,
        context: Ctx,
    ) -> (ExecutionState, EffectPreview<GreenOrder>, Ctx) {
        let Magnet(trans) = self;
        trace!("Running transition: {}", trans);
        let removed_input = trans.removed_input();
        let added_output = trans.added_output();
        let consumed_budget = trans.consumed_budget();
        let consumed_fee = trans.consumed_fee();
        trace!(
            "GreenOrder::exec(removed_input={}, added_output={}, consumed_budget={}, consumed_fee={})",
            removed_input,
            added_output,
            consumed_budget,
            consumed_fee
        );

        let Trans {
            target: Bundled(ord, _),
            result,
        } = trans;
        let leaving_remainder = match &result {
            Next::Term(_) => 0,
            Next::Succ(next) => next.current_remainder,
        };
        if consumed_budget != 0 {
            panic!("GreenOrder execution budget must be zero in phase 1")
        }
        if ord.intention.input_asset != AssetClass::Native {
            panic!("GreenOrder phase-1 execution supports only ADA-leaving orders")
        }
        let operator = Ed25519KeyHash::from(context.select::<OperatorCred>());
        let operator_hash =
            <[u8; 28]>::try_from(operator.to_raw_bytes()).expect("operator hash must be 28 bytes");
        if ord.intention.operator_key_hash != operator_hash {
            panic!("GreenOrder operator does not match runtime operator credential")
        }
        if removed_input > ord.intention.leaving_amount {
            panic!(
                "GreenOrder execution consumed more than declared input: consumed {}, expected at most {}",
                removed_input, ord.intention.leaving_amount
            )
        }
        if leaving_remainder == 0 && added_output < ord.intention.expected_arriving_amount {
            panic!(
                "GreenOrder execution produced insufficient output: produced {}, expected at least {}",
                added_output, ord.intention.expected_arriving_amount
            )
        }
        if leaving_remainder > 0 && !context.allow_green_partial() {
            panic!("GreenOrder partial execution is disabled by runtime configuration")
        }

        let account_bearer = context
            .current_account(ord.account_id)
            .expect("GreenOrder account is not indexed or is currently locked");
        let account =
            AlephAccountUtxo::try_parse(account_bearer.reference(), account_bearer.0.clone(), &context)
                .expect("indexed GreenOrder account UTxO does not match Aleph account validator");
        let mut next_state = match ord.auth {
            GreenAuth::Sig { .. } => account
                .state
                .clone()
                .with_sig_full_fill_nonce(ord.intention.target_nonce_slot, ord.intention.target_nonce_value)
                .expect("GreenOrder target nonce cannot be applied to current account state"),
            GreenAuth::Path { .. } => account.state.clone(),
        };

        let account_validator = context
            .select::<DeployedValidator<{ ALEPH_ACCOUNT_VALIDATOR }>>()
            .erased();
        let batch_witness = context
            .select::<DeployedValidator<{ ALEPH_BATCH_WITNESS_VALIDATOR }>>()
            .erased();
        let witness_hash =
            <[u8; 28]>::try_from(batch_witness.hash.to_raw_bytes()).expect("script hash must be 28 bytes");
        let delegate_index = if account.state.abi == crate::orders::green::AlephAccountAbi::Current {
            aleph_delegate_index_for_witness(&account.state.allowlist, witness_hash)
        } else {
            0
        };

        let mut aleph_intention = ord.aleph_intention();
        aleph_intention.abi = account.state.abi.clone();
        let intent_key = aleph_intention.intent_key();
        let intent_digest = aleph_intention.digest();
        let mut authorized_auth = ord.auth.clone();
        let mut next_order = None;

        if leaving_remainder > 0 {
            let updated_intent = aleph_intention
                .remaining_after_fill(removed_input, added_output)
                .expect("partial GreenOrder execution must produce a remaining intent");
            match &ord.auth {
                GreenAuth::Sig {
                    prefix,
                    postfix,
                    signature,
                    ..
                } => {
                    let planned = context
                        .plan_sig_insert(
                            ord.account_id,
                            account.output_ref,
                            ord.id,
                            intent_key.clone(),
                            updated_intent.clone(),
                        )
                        .expect("failed to plan green Sig store insert");
                    next_state.store_root = planned.new_root;
                    authorized_auth = GreenAuth::Sig {
                        prefix: prefix.clone(),
                        postfix: postfix.clone(),
                        signature: signature.clone(),
                        update_proof: planned.proof.clone(),
                    };
                    next_order = Some(continuation_order(&ord, updated_intent, planned.proof));
                }
                GreenAuth::Path { .. } => {
                    let planned = context
                        .plan_path_update(
                            ord.account_id,
                            account.output_ref,
                            intent_key.clone(),
                            intent_digest,
                            updated_intent.clone(),
                        )
                        .expect("failed to plan green Path store update");
                    next_state.store_root = planned.new_root;
                    authorized_auth = GreenAuth::Path {
                        proof: planned.proof.clone(),
                    };
                    next_order = Some(continuation_order(&ord, updated_intent, planned.proof));
                }
            }
        } else if matches!(ord.auth, GreenAuth::Path { .. }) {
            let planned = context
                .plan_path_completion(ord.account_id, account.output_ref, intent_key, intent_digest)
                .expect("failed to plan green Path store completion");
            next_state.store_root = planned.root;
            authorized_auth = GreenAuth::Path { proof: planned.proof };
        }

        let mut account_output = account.output.clone();
        apply_full_fill_to_account_output(
            &mut account_output,
            &ord,
            removed_input,
            added_output,
            consumed_fee,
        )
        .expect("GreenOrder account value transition failed");
        let datum = account_output
            .data_mut()
            .expect("Aleph account output must carry inline account datum");
        *datum = next_state.into_pd();

        let account_action = match account.state.abi {
            crate::orders::green::AlephAccountAbi::Legacy => AlephAccountAction::Direct,
            crate::orders::green::AlephAccountAbi::Current => AlephAccountAction::Delegate(delegate_index),
        };

        let input = ScriptInputBlueprint {
            reference: account.output_ref,
            utxo: account.output.clone(),
            script: ScriptWitness {
                hash: account_validator.hash,
                cost: delayed_cost(move |ctx| {
                    account_validator.ex_budget + account_validator.marginal_cost.scale(ctx.self_index as u64)
                }),
            },
            redeemer: ready_redeemer(account_action.into_pd()),
            required_signers: vec![operator].into(),
        };

        let authorized = AlephAuthorizedIntention {
            intent: aleph_intention,
            remainder: leaving_remainder,
            auth: authorized_auth,
        };
        let consumed_bundle = Bundled(ord.clone(), account.finalized_output());

        state.add_tx_fee(consumed_budget);
        state.add_operator_interest(consumed_fee);
        state.tx_blueprint.add_account_io(input, account_output.clone());
        state.tx_blueprint.add_ref_input(account_validator.reference_utxo);
        state
            .tx_blueprint
            .add_aleph_batch_intention(batch_witness, account.output_ref, authorized);

        let effect = if let Some(next_order) = next_order {
            ExecutionEff::Updated(consumed_bundle, Bundled(next_order, account_output))
        } else {
            ExecutionEff::Eliminated(consumed_bundle)
        };

        (state, effect, context)
    }
}

fn continuation_order(
    original: &GreenOrder,
    intent: crate::orders::green::AlephIntention,
    proof: Vec<u8>,
) -> GreenOrder {
    GreenOrder {
        id: original.id,
        account_id: original.account_id,
        intention: GreenIntention {
            input_asset: intent.leaving_asset,
            output_asset: intent.arriving_asset,
            leaving_amount: intent.leaving_amount,
            expected_arriving_amount: intent.expected_arriving_amount,
            fee_lovelace: intent.fee_lovelace,
            target_nonce_slot: intent.target_nonce_index,
            target_nonce_value: intent.target_nonce_value,
            operator_key_hash: intent.operator,
        },
        accumulated_output: 0,
        current_remainder: intent.leaving_amount,
        auth: GreenAuth::Path { proof },
    }
}

impl<Ctx> BatchExec<ExecutionState, EffectPreview<RoyaltyV1PoolOnly>, Ctx>
    for Magnet<Make<RoyaltyV1PoolOnly, FinalizedTxOut>>
where
    Ctx: Has<DeployedValidator<{ RoyaltyPoolV1 as u8 }>>
        + Has<DeployedValidator<{ RoyaltyPoolV1LedgerFixed as u8 }>>,
{
    fn exec(
        self,
        mut state: ExecutionState,
        context: Ctx,
    ) -> (ExecutionState, EffectPreview<RoyaltyV1PoolOnly>, Ctx) {
        let Magnet(trans) = self;
        let side = trans.trade_side().expect("Empty swaps aren't allowed");
        let removed_liquidity = trans.loss().expect("Something must be removed");
        let added_liquidity = trans.gain().expect("Something must be added");
        let Trans {
            target: Bundled(pool, FinalizedTxOut(consumed_out, in_ref)),
            result,
        } = trans;
        let mut produced_out = consumed_out.clone();
        let PoolAssetMapping {
            asset_to_deduct_from,
            asset_to_add_to,
        } = ConstFnPool::Royalty(pool.into_inner()).asset_mapping(side);
        trace!("RoyaltyV1PoolOnly::exec(side={}, removed_liq={}, added_liq={}, asset_to_deduct_from={}, asset_to_add_to={})", side, removed_liquidity, added_liquidity, asset_to_deduct_from, asset_to_add_to);
        produced_out.sub_asset(asset_to_deduct_from, removed_liquidity);
        produced_out.add_asset(asset_to_add_to, added_liquidity);

        let DeployedValidatorErased {
            reference_utxo,
            hash,
            ex_budget,
            marginal_cost,
        } = pool.get_validator(&context);
        let input = ScriptInputBlueprint {
            reference: in_ref,
            utxo: consumed_out.clone(),
            script: ScriptWitness {
                hash,
                cost: delayed_cost(move |ctx| ex_budget + marginal_cost.scale(ctx.self_index as u64)),
            },
            redeemer: delayed_redeemer(move |ordering| {
                CFMMPoolRedeemer {
                    pool_input_index: ordering.index_of(&in_ref) as u64,
                    action: CFMMPoolAction::Swap,
                }
                .to_plutus_data()
            }),
            required_signers: vec![].into(),
        };

        let Next::Succ(transition) = result else {
            panic!("Royalty V1 pool isn't supposed to terminate in result of a trade")
        };

        if let Some(data) = produced_out.data_mut() {
            ConstFnPool::Royalty(transition.into_inner()).unsafe_datum_update(data);
        }

        let updated_output = produced_out.clone();
        let consumed = Bundled(pool, FinalizedTxOut(consumed_out, in_ref));
        let produced = Bundled(transition, updated_output.clone());
        let trans = ExecutionEff::Updated(consumed, produced);

        state.tx_blueprint.add_io(input, updated_output);
        state.tx_blueprint.add_ref_input(reference_utxo);
        (state, trans, context)
    }
}

/// Batch execution logic for [ConstFnPool].
impl<Ctx> BatchExec<ExecutionState, EffectPreview<ConstFnPool>, Ctx>
    for Magnet<Make<ConstFnPool, FinalizedTxOut>>
where
    Ctx: Has<DeployedValidator<{ ConstFnPoolV1 as u8 }>>
        + Has<DeployedValidator<{ ConstFnPoolV2 as u8 }>>
        + Has<DeployedValidator<{ ConstFnPoolFeeSwitch as u8 }>>
        + Has<DeployedValidator<{ ConstFnPoolFeeSwitchV2 as u8 }>>
        + Has<DeployedValidator<{ ConstFnPoolFeeSwitchBiDirFee as u8 }>>
        + Has<DeployedValidator<{ RoyaltyPoolV1 as u8 }>>
        + Has<DeployedValidator<{ RoyaltyPoolV1LedgerFixed as u8 }>>
        + Has<DeployedValidator<{ RoyaltyPoolV2 as u8 }>>,
{
    fn exec(
        self,
        mut state: ExecutionState,
        context: Ctx,
    ) -> (ExecutionState, EffectPreview<ConstFnPool>, Ctx) {
        let Magnet(trans) = self;
        let side = trans.trade_side().expect("Empty swaps aren't allowed");
        let removed_liquidity = trans.loss().expect("Something must be removed");
        let added_liquidity = trans.gain().expect("Something must be added");
        let Trans {
            target: Bundled(pool, FinalizedTxOut(consumed_out, in_ref)),
            result,
        } = trans;
        let mut produced_out = consumed_out.clone();
        let PoolAssetMapping {
            asset_to_deduct_from,
            asset_to_add_to,
        } = pool.asset_mapping(side);
        trace!("ConstFnPool::exec(side={}, removed_liq={}, added_liq={}, asset_to_deduct_from={}, asset_to_add_to={})", side, removed_liquidity, added_liquidity, asset_to_deduct_from, asset_to_add_to);
        produced_out.sub_asset(asset_to_deduct_from, removed_liquidity);
        produced_out.add_asset(asset_to_add_to, added_liquidity);

        let DeployedValidatorErased {
            reference_utxo,
            hash,
            ex_budget,
            marginal_cost,
        } = pool.get_validator(&context);
        let input = ScriptInputBlueprint {
            reference: in_ref,
            utxo: consumed_out.clone(),
            script: ScriptWitness {
                hash,
                cost: delayed_cost(move |ctx| ex_budget + marginal_cost.scale(ctx.self_index as u64)),
            },
            redeemer: delayed_redeemer(move |ordering| {
                CFMMPoolRedeemer {
                    pool_input_index: ordering.index_of(&in_ref) as u64,
                    action: CFMMPoolAction::Swap,
                }
                .to_plutus_data()
            }),
            required_signers: vec![].into(),
        };

        let Next::Succ(transition) = result else {
            panic!("ConstFn pool isn't supposed to terminate in result of a trade")
        };

        if let Some(data) = produced_out.data_mut() {
            transition.unsafe_datum_update(data);
        }

        let updated_output = produced_out.clone();

        let consumed = Bundled(pool, FinalizedTxOut(consumed_out, in_ref));
        let produced = Bundled(transition, updated_output.clone());
        let trans = ExecutionEff::Updated(consumed, produced);

        state.tx_blueprint.add_io(input, updated_output);
        state.tx_blueprint.add_ref_input(reference_utxo);
        (state, trans, context)
    }
}

impl<Ctx> BatchExec<ExecutionState, EffectPreview<BalancePool>, Ctx>
    for Magnet<Make<BalancePool, FinalizedTxOut>>
where
    Ctx: Has<DeployedValidator<{ BalanceFnPoolV1 as u8 }>>,
    Ctx: Has<DeployedValidator<{ BalanceFnPoolV2 as u8 }>>,
{
    fn exec(
        self,
        mut state: ExecutionState,
        context: Ctx,
    ) -> (ExecutionState, EffectPreview<BalancePool>, Ctx) {
        let Magnet(trans) = self;
        let side = trans.trade_side().expect("Empty swaps aren't allowed");
        let removed_liquidity = trans.loss().expect("Something must be removed");
        let added_liquidity = trans.gain().expect("Something must be added");
        let Trans {
            target: Bundled(pool, FinalizedTxOut(consumed_out, in_ref)),
            result,
        } = trans;
        let mut produced_out = consumed_out.clone();
        let PoolAssetMapping {
            asset_to_deduct_from,
            asset_to_add_to,
        } = pool.get_asset_deltas(side);
        produced_out.sub_asset(asset_to_deduct_from, removed_liquidity);
        produced_out.add_asset(asset_to_add_to, added_liquidity);

        let Next::Succ(transition) = result else {
            panic!("Balance pool isn't supposed to terminate in result of a trade")
        };

        let DeployedValidatorErased {
            reference_utxo,
            hash,
            ex_budget,
            marginal_cost,
        } = pool.get_validator(&context);
        let input = ScriptInputBlueprint {
            reference: in_ref,
            utxo: consumed_out.clone(),
            script: ScriptWitness {
                hash,
                cost: delayed_cost(move |ctx| ex_budget + marginal_cost.scale(ctx.self_index as u64)),
            },
            redeemer: delayed_redeemer(move |ordering| {
                BalancePoolRedeemer {
                    pool_input_index: ordering.index_of(&in_ref) as u64,
                    action: CFMMPoolAction::Swap,
                    new_pool_state: transition,
                    prev_pool_state: pool,
                }
                .to_plutus_data()
            }),
            required_signers: vec![].into(),
        };

        if let Some(data) = produced_out.data_mut() {
            balance_pool::unsafe_update_datum(
                data,
                transition.treasury_x.untag(),
                transition.treasury_y.untag(),
            );
        }

        let consumed = Bundled(pool, FinalizedTxOut(consumed_out, in_ref));
        let produced = Bundled(transition, produced_out.clone());
        let effect = ExecutionEff::Updated(consumed, produced);

        state.tx_blueprint.add_io(input, produced_out);
        state.tx_blueprint.add_ref_input(reference_utxo);
        (state, effect, context)
    }
}

impl<Ctx> BatchExec<ExecutionState, EffectPreview<StablePoolT2T>, Ctx>
    for Magnet<Make<StablePoolT2T, FinalizedTxOut>>
where
    Ctx: Has<DeployedValidator<{ StableFnPoolT2T as u8 }>>,
{
    fn exec(
        self,
        mut state: ExecutionState,
        context: Ctx,
    ) -> (ExecutionState, EffectPreview<StablePoolT2T>, Ctx) {
        let Magnet(trans) = self;
        let side = trans.trade_side().expect("Empty swaps aren't allowed");
        let removed_liquidity = trans.loss().expect("Something must be removed");
        let added_liquidity = trans.gain().expect("Something must be added");
        let Trans {
            target: Bundled(pool, FinalizedTxOut(consumed_out, in_ref)),
            result,
        } = trans;
        let mut produced_out = consumed_out.clone();
        let PoolAssetMapping {
            asset_to_deduct_from,
            asset_to_add_to,
        } = pool.get_asset_deltas(side);
        produced_out.sub_asset(asset_to_deduct_from, removed_liquidity);
        produced_out.add_asset(asset_to_add_to, added_liquidity);

        let Next::Succ(transition) = result else {
            panic!("Stable pool isn't supposed to terminate in result of a trade")
        };

        let DeployedValidatorErased {
            reference_utxo,
            hash,
            ex_budget,
            marginal_cost,
        } = pool.get_validator(&context);
        let input = ScriptInputBlueprint {
            reference: in_ref,
            utxo: consumed_out.clone(),
            script: ScriptWitness {
                hash,
                cost: delayed_cost(move |ctx| ex_budget + marginal_cost.scale(ctx.self_index as u64)),
            },
            redeemer: delayed_redeemer(move |ordering| {
                let pool_index = ordering.index_of(&in_ref) as u64;
                StablePoolRedeemer {
                    pool_input_index: pool_index,
                    pool_output_index: pool_index,
                    action: CFMMPoolAction::Swap,
                    new_pool_state: transition,
                    prev_pool_state: pool,
                }
                .to_plutus_data()
            }),
            required_signers: vec![].into(),
        };

        if let Some(data) = produced_out.data_mut() {
            stable_pool_t2t::unsafe_update_datum(
                data,
                transition.treasury_x.untag(),
                transition.treasury_y.untag(),
            );
        }

        let consumed = Bundled(pool, FinalizedTxOut(consumed_out, in_ref));
        let produced = Bundled(transition, produced_out.clone());
        let effect = ExecutionEff::Updated(consumed, produced);

        state.tx_blueprint.add_io(input, produced_out);
        state.tx_blueprint.add_ref_input(reference_utxo);
        (state, effect, context)
    }
}

#[cfg(test)]
mod tests {
    use super::aleph_delegate_index_for_witness;

    #[test]
    fn aleph_delegate_index_uses_batch_witness_allowlist_position() {
        let witness_hash = [6; 28];
        let allowlist = [[4; 28], [5; 28], witness_hash, [7; 28]];

        assert_eq!(2, aleph_delegate_index_for_witness(&allowlist, witness_hash));
    }

    #[test]
    #[should_panic(expected = "Aleph batch witness script is not present in account allowlist")]
    fn aleph_delegate_index_rejects_missing_batch_witness_hash() {
        aleph_delegate_index_for_witness(&[[4; 28], [5; 28]], [6; 28]);
    }
}

impl<Ctx> BatchExec<ExecutionState, EffectPreview<QuadraticPool>, Ctx>
    for Magnet<Make<QuadraticPool, FinalizedTxOut>>
where
    Ctx: Has<DeployedValidator<{ DegenQuadraticPoolV1 as u8 }>>,
    Ctx: Has<DeployedValidator<{ DegenQuadraticPoolV1T2T as u8 }>>,
{
    fn exec(
        self,
        mut state: ExecutionState,
        context: Ctx,
    ) -> (ExecutionState, EffectPreview<QuadraticPool>, Ctx) {
        let Magnet(trans) = self;
        let side = trans.trade_side().expect("Empty swaps aren't allowed");
        let removed_liquidity = trans.loss().expect("Something must be removed");
        let added_liquidity = trans.gain().expect("Something must be added");
        let Trans {
            target: Bundled(pool, FinalizedTxOut(consumed_out, in_ref)),
            result,
        } = trans;
        let mut produced_out = consumed_out.clone();
        let PoolAssetMapping {
            asset_to_deduct_from,
            asset_to_add_to,
        } = pool.asset_mapping(side);
        produced_out.sub_asset(asset_to_deduct_from, removed_liquidity);
        produced_out.add_asset(asset_to_add_to, added_liquidity);

        let Next::Succ(transition) = result else {
            panic!("Degen pool isn't supposed to terminate in result of a trade")
        };

        if transition.ver == V1T2T {
            if let Some(data) = produced_out.data_mut() {
                quadratic_pool::unsafe_update_t2t_pd(data, transition.accumulated_x_fee);
            }
        }

        let DeployedValidatorErased {
            reference_utxo,
            hash,
            ex_budget,
            marginal_cost,
        } = pool.get_validator(&context);
        let input = ScriptInputBlueprint {
            reference: in_ref,
            utxo: consumed_out.clone(),
            script: ScriptWitness {
                hash,
                cost: delayed_cost(move |ctx| ex_budget + marginal_cost.scale(ctx.self_index as u64)),
            },
            redeemer: delayed_redeemer(move |ordering| {
                let pool_index = ordering.index_of(&in_ref) as u64;
                QuadraticPoolRedeemer {
                    pool_input_index: pool_index,
                    pool_output_index: pool_index,
                    action: CFMMPoolAction::Swap,
                }
                .to_plutus_data()
            }),
            required_signers: vec![].into(),
        };

        let consumed = Bundled(pool, FinalizedTxOut(consumed_out, in_ref));
        let produced = Bundled(transition, produced_out.clone());
        let effect = ExecutionEff::Updated(consumed, produced);

        state.tx_blueprint.add_io(input, produced_out);
        state.tx_blueprint.add_ref_input(reference_utxo);
        (state, effect, context)
    }
}
