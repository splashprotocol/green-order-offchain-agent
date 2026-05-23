use std::sync::{Arc, Mutex};

use crate::account_index::AccountIndex;
use crate::deployment::GreenProtocolDeployment;
use bloom_offchain::execution_engine::liquidity_book::config::ExecutionConfig;
use bloom_offchain::execution_engine::types::Time;
use bloom_offchain_cardano::orders::green::{
    AccountId, AlephIntention, GreenAccountLookup, GreenPartialPolicy, GreenStorePlanner,
    GreenStorePlanningError, PlannedStoreCompletion, PlannedStoreDelta, ALEPH_ACCOUNT_VALIDATOR,
    ALEPH_BATCH_WITNESS_VALIDATOR,
};
use spectrum_cardano_lib::collateral::Collateral;
use spectrum_cardano_lib::ex_units::ExUnits;
use spectrum_cardano_lib::output::FinalizedTxOut;
use spectrum_cardano_lib::NetworkId;
use spectrum_offchain::backlog::BacklogCapacity;
use spectrum_offchain::domain::Has;
use spectrum_offchain_cardano::creds::{OperatorCred, OperatorRewardAddress};
use spectrum_offchain_cardano::data::dao_request::DAOContext;
use spectrum_offchain_cardano::data::royalty_withdraw_request::RoyaltyWithdrawContext;
use spectrum_offchain_cardano::deployment::ProtocolValidator::{
    BalanceFnPoolDeposit, BalanceFnPoolRedeem, BalanceFnPoolV1, BalanceFnPoolV2, ConstFnFeeSwitchPoolDeposit,
    ConstFnFeeSwitchPoolRedeem, ConstFnFeeSwitchPoolSwap, ConstFnPoolDeposit, ConstFnPoolFeeSwitch,
    ConstFnPoolFeeSwitchBiDirFee, ConstFnPoolFeeSwitchV2, ConstFnPoolRedeem, ConstFnPoolSwap, ConstFnPoolV1,
    ConstFnPoolV2, GridOrderNative, LimitOrderV1, LimitOrderWitnessV1, RoyaltyPoolDAOV1,
    RoyaltyPoolDAOV1Request, RoyaltyPoolRoyaltyWithdraw, RoyaltyPoolRoyaltyWithdrawLedgerFixed,
    RoyaltyPoolRoyaltyWithdrawV2, RoyaltyPoolV1, RoyaltyPoolV1Deposit, RoyaltyPoolV1LedgerFixed,
    RoyaltyPoolV1Redeem, RoyaltyPoolV1RoyaltyWithdrawRequest, RoyaltyPoolV2, RoyaltyPoolV2DAO,
    RoyaltyPoolV2DAOV1Request, RoyaltyPoolV2Deposit, RoyaltyPoolV2Redeem,
    RoyaltyPoolV2RoyaltyWithdrawRequest, StableFnPoolT2T, StableFnPoolT2TDeposit, StableFnPoolT2TRedeem,
};
use spectrum_offchain_cardano::deployment::{DeployedScriptInfo, DeployedValidator};
use type_equalities::IsEqual;

#[derive(Debug, Clone)]
pub struct MakerContext {
    pub time: Time,
    pub execution_conf: ExecutionConfig<ExUnits>,
    pub backlog_capacity: BacklogCapacity,
}

impl Has<BacklogCapacity> for MakerContext {
    fn select<U: IsEqual<BacklogCapacity>>(&self) -> BacklogCapacity {
        self.backlog_capacity
    }
}

impl Has<Time> for MakerContext {
    fn select<U: IsEqual<Time>>(&self) -> Time {
        self.time
    }
}

impl Has<ExecutionConfig<ExUnits>> for MakerContext {
    fn select<U: IsEqual<ExecutionConfig<ExUnits>>>(&self) -> ExecutionConfig<ExUnits> {
        self.execution_conf
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub time: Time,
    pub deployment: GreenProtocolDeployment,
    pub collateral: Collateral,
    pub reward_addr: OperatorRewardAddress,
    pub backlog_capacity: BacklogCapacity,
    pub network_id: NetworkId,
    pub operator_cred: OperatorCred,
    pub dao_ctx: DAOContext,
    pub royalty_context: RoyaltyWithdrawContext,
    pub account_index: Arc<Mutex<AccountIndex>>,
    pub allow_partial: bool,
}

impl Has<NetworkId> for ExecutionContext {
    fn select<U: IsEqual<NetworkId>>(&self) -> NetworkId {
        self.network_id
    }
}

impl Has<OperatorCred> for ExecutionContext {
    fn select<U: IsEqual<OperatorCred>>(&self) -> OperatorCred {
        self.operator_cred
    }
}

impl Has<BacklogCapacity> for ExecutionContext {
    fn select<U: IsEqual<BacklogCapacity>>(&self) -> BacklogCapacity {
        self.backlog_capacity
    }
}

impl Has<Time> for ExecutionContext {
    fn select<U: IsEqual<Time>>(&self) -> Time {
        self.time
    }
}

impl Has<Collateral> for ExecutionContext {
    fn select<U: IsEqual<Collateral>>(&self) -> Collateral {
        self.collateral.clone()
    }
}

impl Has<OperatorRewardAddress> for ExecutionContext {
    fn select<U: IsEqual<OperatorRewardAddress>>(&self) -> OperatorRewardAddress {
        self.reward_addr.clone()
    }
}

impl GreenAccountLookup for ExecutionContext {
    fn current_account(&self, account_id: AccountId) -> Option<FinalizedTxOut> {
        self.account_index
            .lock()
            .expect("account index lock poisoned")
            .current_or_pending_base(account_id)
    }
}

impl GreenPartialPolicy for ExecutionContext {
    fn allow_green_partial(&self) -> bool {
        self.allow_partial
    }
}

impl GreenStorePlanner for ExecutionContext {
    fn plan_sig_insert(
        &self,
        account_id: AccountId,
        old_account_ref: spectrum_cardano_lib::OutputRef,
        canonical_order_id: bloom_offchain_cardano::orders::green::GreenOrderId,
        key: Vec<u8>,
        updated_intent: AlephIntention,
    ) -> Result<PlannedStoreDelta, GreenStorePlanningError> {
        self.account_index
            .lock()
            .expect("account index lock poisoned")
            .plan_sig_insert(
                account_id,
                old_account_ref,
                canonical_order_id,
                key,
                updated_intent,
            )
    }

    fn plan_path_update(
        &self,
        account_id: AccountId,
        old_account_ref: spectrum_cardano_lib::OutputRef,
        key: Vec<u8>,
        old_digest: [u8; 32],
        updated_intent: AlephIntention,
    ) -> Result<PlannedStoreDelta, GreenStorePlanningError> {
        self.account_index
            .lock()
            .expect("account index lock poisoned")
            .plan_path_update(account_id, old_account_ref, key, old_digest, updated_intent)
    }

    fn plan_path_completion(
        &self,
        account_id: AccountId,
        old_account_ref: spectrum_cardano_lib::OutputRef,
        key: Vec<u8>,
        old_digest: [u8; 32],
    ) -> Result<PlannedStoreCompletion, GreenStorePlanningError> {
        self.account_index
            .lock()
            .expect("account index lock poisoned")
            .plan_path_completion(account_id, old_account_ref, key, old_digest)
    }
}

impl Has<DeployedValidator<{ ConstFnPoolV1 as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ ConstFnPoolV1 as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ ConstFnPoolV1 as u8 }> {
        self.deployment.spectrum.const_fn_pool_v1.clone()
    }
}

impl Has<DeployedValidator<{ ALEPH_ACCOUNT_VALIDATOR }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ ALEPH_ACCOUNT_VALIDATOR }>>>(
        &self,
    ) -> DeployedValidator<{ ALEPH_ACCOUNT_VALIDATOR }> {
        self.deployment.aleph_account.clone()
    }
}

impl Has<DeployedValidator<{ ALEPH_BATCH_WITNESS_VALIDATOR }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ ALEPH_BATCH_WITNESS_VALIDATOR }>>>(
        &self,
    ) -> DeployedValidator<{ ALEPH_BATCH_WITNESS_VALIDATOR }> {
        self.deployment.aleph_batch_witness.clone()
    }
}

impl Has<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>>>(
        &self,
    ) -> DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }> {
        DeployedScriptInfo::from(&self.deployment.aleph_account)
    }
}

impl Has<DeployedScriptInfo<{ ALEPH_BATCH_WITNESS_VALIDATOR }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedScriptInfo<{ ALEPH_BATCH_WITNESS_VALIDATOR }>>>(
        &self,
    ) -> DeployedScriptInfo<{ ALEPH_BATCH_WITNESS_VALIDATOR }> {
        DeployedScriptInfo::from(&self.deployment.aleph_batch_witness)
    }
}

impl Has<DeployedValidator<{ ConstFnPoolV2 as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ ConstFnPoolV2 as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ ConstFnPoolV2 as u8 }> {
        self.deployment.spectrum.const_fn_pool_v2.clone()
    }
}

impl Has<DeployedValidator<{ ConstFnPoolFeeSwitch as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ ConstFnPoolFeeSwitch as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ ConstFnPoolFeeSwitch as u8 }> {
        self.deployment.spectrum.const_fn_pool_fee_switch.clone()
    }
}

impl Has<DeployedValidator<{ ConstFnPoolFeeSwitchV2 as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ ConstFnPoolFeeSwitchV2 as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ ConstFnPoolFeeSwitchV2 as u8 }> {
        self.deployment.spectrum.const_fn_pool_fee_switch_v2.clone()
    }
}

impl Has<DeployedValidator<{ ConstFnPoolFeeSwitchBiDirFee as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ ConstFnPoolFeeSwitchBiDirFee as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ ConstFnPoolFeeSwitchBiDirFee as u8 }> {
        self.deployment
            .spectrum
            .const_fn_pool_fee_switch_bidir_fee
            .clone()
    }
}

impl Has<DeployedValidator<{ ConstFnPoolSwap as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ ConstFnPoolSwap as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ ConstFnPoolSwap as u8 }> {
        self.deployment.spectrum.const_fn_pool_swap.clone()
    }
}

impl Has<DeployedValidator<{ ConstFnPoolDeposit as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ ConstFnPoolDeposit as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ ConstFnPoolDeposit as u8 }> {
        self.deployment.spectrum.const_fn_pool_deposit.clone()
    }
}

impl Has<DeployedValidator<{ ConstFnPoolRedeem as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ ConstFnPoolRedeem as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ ConstFnPoolRedeem as u8 }> {
        self.deployment.spectrum.const_fn_pool_redeem.clone()
    }
}

impl Has<DeployedValidator<{ ConstFnFeeSwitchPoolSwap as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ ConstFnFeeSwitchPoolSwap as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ ConstFnFeeSwitchPoolSwap as u8 }> {
        self.deployment.spectrum.const_fn_fee_switch_pool_swap.clone()
    }
}

impl Has<DeployedValidator<{ ConstFnFeeSwitchPoolDeposit as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ ConstFnFeeSwitchPoolDeposit as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ ConstFnFeeSwitchPoolDeposit as u8 }> {
        self.deployment.spectrum.const_fn_fee_switch_pool_deposit.clone()
    }
}

impl Has<DeployedValidator<{ ConstFnFeeSwitchPoolRedeem as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ ConstFnFeeSwitchPoolRedeem as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ ConstFnFeeSwitchPoolRedeem as u8 }> {
        self.deployment.spectrum.const_fn_fee_switch_pool_redeem.clone()
    }
}

impl Has<DeployedValidator<{ BalanceFnPoolV1 as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ BalanceFnPoolV1 as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ BalanceFnPoolV1 as u8 }> {
        self.deployment.spectrum.balance_fn_pool_v1.clone()
    }
}

impl Has<DeployedValidator<{ BalanceFnPoolV2 as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ BalanceFnPoolV2 as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ BalanceFnPoolV2 as u8 }> {
        self.deployment.spectrum.balance_fn_pool_v2.clone()
    }
}

impl Has<DeployedValidator<{ BalanceFnPoolRedeem as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ BalanceFnPoolRedeem as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ BalanceFnPoolRedeem as u8 }> {
        self.deployment.spectrum.balance_fn_pool_redeem.clone()
    }
}

impl Has<DeployedValidator<{ BalanceFnPoolDeposit as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ BalanceFnPoolDeposit as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ BalanceFnPoolDeposit as u8 }> {
        self.deployment.spectrum.balance_fn_pool_deposit.clone()
    }
}

impl Has<DeployedValidator<{ StableFnPoolT2T as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ StableFnPoolT2T as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ StableFnPoolT2T as u8 }> {
        self.deployment.spectrum.stable_fn_pool_t2t.clone()
    }
}

impl Has<DeployedValidator<{ StableFnPoolT2TDeposit as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ StableFnPoolT2TDeposit as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ StableFnPoolT2TDeposit as u8 }> {
        self.deployment.spectrum.stable_fn_pool_t2t_deposit.clone()
    }
}

impl Has<DeployedValidator<{ StableFnPoolT2TRedeem as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ StableFnPoolT2TRedeem as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ StableFnPoolT2TRedeem as u8 }> {
        self.deployment.spectrum.stable_fn_pool_t2t_redeem.clone()
    }
}

impl Has<DeployedValidator<{ LimitOrderV1 as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ LimitOrderV1 as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ LimitOrderV1 as u8 }> {
        self.deployment.spectrum.limit_order.clone()
    }
}

impl Has<DeployedValidator<{ LimitOrderWitnessV1 as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ LimitOrderWitnessV1 as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ LimitOrderWitnessV1 as u8 }> {
        self.deployment.spectrum.limit_order_witness.clone()
    }
}

impl Has<DeployedValidator<{ GridOrderNative as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ GridOrderNative as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ GridOrderNative as u8 }> {
        self.deployment.spectrum.grid_order_native.clone()
    }
}

impl Has<DeployedValidator<{ RoyaltyPoolV1 as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ RoyaltyPoolV1 as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ RoyaltyPoolV1 as u8 }> {
        self.deployment.spectrum.royalty_pool.clone()
    }
}

impl Has<DeployedValidator<{ RoyaltyPoolV1LedgerFixed as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ RoyaltyPoolV1LedgerFixed as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ RoyaltyPoolV1LedgerFixed as u8 }> {
        self.deployment.spectrum.royalty_pool_ledger_fixed.clone()
    }
}

impl Has<DeployedValidator<{ RoyaltyPoolV2 as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ RoyaltyPoolV2 as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ RoyaltyPoolV2 as u8 }> {
        self.deployment.spectrum.royalty_pool_v2.clone()
    }
}

impl Has<DeployedValidator<{ RoyaltyPoolV1Deposit as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ RoyaltyPoolV1Deposit as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ RoyaltyPoolV1Deposit as u8 }> {
        self.deployment.spectrum.royalty_pool_deposit.clone()
    }
}

impl Has<DeployedValidator<{ RoyaltyPoolV2Deposit as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ RoyaltyPoolV2Deposit as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ RoyaltyPoolV2Deposit as u8 }> {
        self.deployment.spectrum.royalty_pool_deposit_v2.clone()
    }
}

impl Has<DeployedValidator<{ RoyaltyPoolV1Redeem as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ RoyaltyPoolV1Redeem as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ RoyaltyPoolV1Redeem as u8 }> {
        self.deployment.spectrum.royalty_pool_redeem.clone()
    }
}

impl Has<DeployedValidator<{ RoyaltyPoolV2Redeem as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ RoyaltyPoolV2Redeem as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ RoyaltyPoolV2Redeem as u8 }> {
        self.deployment.spectrum.royalty_pool_redeem_v2.clone()
    }
}

impl Has<DeployedValidator<{ RoyaltyPoolV1RoyaltyWithdrawRequest as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ RoyaltyPoolV1RoyaltyWithdrawRequest as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ RoyaltyPoolV1RoyaltyWithdrawRequest as u8 }> {
        self.deployment
            .spectrum
            .royalty_pool_royalty_withdraw_request
            .clone()
    }
}

impl Has<DeployedValidator<{ RoyaltyPoolV2RoyaltyWithdrawRequest as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ RoyaltyPoolV2RoyaltyWithdrawRequest as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ RoyaltyPoolV2RoyaltyWithdrawRequest as u8 }> {
        self.deployment
            .spectrum
            .royalty_pool_v2_royalty_withdraw_request
            .clone()
    }
}

impl Has<DeployedValidator<{ RoyaltyPoolDAOV1Request as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ RoyaltyPoolDAOV1Request as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ RoyaltyPoolDAOV1Request as u8 }> {
        self.deployment.spectrum.royalty_pool_dao_request.clone()
    }
}

impl Has<DeployedValidator<{ RoyaltyPoolV2DAOV1Request as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ RoyaltyPoolV2DAOV1Request as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ RoyaltyPoolV2DAOV1Request as u8 }> {
        self.deployment.spectrum.royalty_pool_v2_dao_request.clone()
    }
}

impl Has<DeployedValidator<{ RoyaltyPoolRoyaltyWithdraw as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ RoyaltyPoolRoyaltyWithdraw as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ RoyaltyPoolRoyaltyWithdraw as u8 }> {
        self.deployment.spectrum.royalty_pool_withdraw.clone()
    }
}

impl Has<DeployedValidator<{ RoyaltyPoolRoyaltyWithdrawLedgerFixed as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ RoyaltyPoolRoyaltyWithdrawLedgerFixed as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ RoyaltyPoolRoyaltyWithdrawLedgerFixed as u8 }> {
        self.deployment
            .spectrum
            .royalty_pool_withdraw_ledger_fixed
            .clone()
    }
}

impl Has<DeployedValidator<{ RoyaltyPoolRoyaltyWithdrawV2 as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ RoyaltyPoolRoyaltyWithdrawV2 as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ RoyaltyPoolRoyaltyWithdrawV2 as u8 }> {
        self.deployment.spectrum.royalty_pool_withdraw_v2.clone()
    }
}

impl Has<DeployedValidator<{ RoyaltyPoolDAOV1 as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ RoyaltyPoolDAOV1 as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ RoyaltyPoolDAOV1 as u8 }> {
        self.deployment.spectrum.royalty_pool_dao.clone()
    }
}

impl Has<DeployedValidator<{ RoyaltyPoolV2DAO as u8 }>> for ExecutionContext {
    fn select<U: IsEqual<DeployedValidator<{ RoyaltyPoolV2DAO as u8 }>>>(
        &self,
    ) -> DeployedValidator<{ RoyaltyPoolV2DAO as u8 }> {
        self.deployment.spectrum.royalty_pool_v2_dao.clone()
    }
}

impl Has<DAOContext> for ExecutionContext {
    fn select<U: IsEqual<DAOContext>>(&self) -> DAOContext {
        self.dao_ctx.clone()
    }
}

impl Has<RoyaltyWithdrawContext> for ExecutionContext {
    fn select<U: IsEqual<RoyaltyWithdrawContext>>(&self) -> RoyaltyWithdrawContext {
        self.royalty_context.clone()
    }
}
