use bloom_offchain_cardano::orders::green::{ALEPH_ACCOUNT_VALIDATOR, ALEPH_BATCH_WITNESS_VALIDATOR};
use cardano_explorer::CardanoNetwork;
use cml_chain::builders::tx_builder::TransactionUnspentOutput;
use spectrum_cardano_lib::ex_units::ExUnits;
use spectrum_offchain::domain::Has;
use spectrum_offchain_cardano::deployment::{
    DeployedScriptInfo, DeployedValidator, DeployedValidatorRef, DeployedValidators, ProtocolDeployment,
    ProtocolScriptHashes, ProtocolValidator,
};
use type_equalities::IsEqual;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GreenDeployedValidators {
    #[serde(flatten)]
    pub spectrum: DeployedValidators,
    pub aleph_account: DeployedValidatorRef,
    pub aleph_batch_witness: DeployedValidatorRef,
}

#[derive(Clone, Debug)]
pub struct GreenProtocolDeployment {
    pub spectrum: ProtocolDeployment,
    pub aleph_account: DeployedValidator<{ ALEPH_ACCOUNT_VALIDATOR }>,
    pub aleph_batch_witness: DeployedValidator<{ ALEPH_BATCH_WITNESS_VALIDATOR }>,
}

#[derive(Copy, Clone, Debug)]
pub struct GreenScriptHashes {
    pub spectrum: ProtocolScriptHashes,
    pub aleph_account: DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>,
    pub aleph_batch_witness: DeployedScriptInfo<{ ALEPH_BATCH_WITNESS_VALIDATOR }>,
}

impl GreenProtocolDeployment {
    pub async fn unsafe_pull<Net: CardanoNetwork>(
        validators: GreenDeployedValidators,
        explorer: &Net,
    ) -> Self {
        let spectrum_refs = validators.spectrum;
        let royalty_pool_ledger_fixed =
            DeployedValidator::unsafe_pull(spectrum_refs.royalty_pool_ledger_fixed.clone(), explorer).await;
        let script_anchor = royalty_pool_ledger_fixed.reference_utxo.clone();
        Self {
            spectrum: green_protocol_deployment_from_refs(
                spectrum_refs,
                royalty_pool_ledger_fixed,
                script_anchor,
            ),
            aleph_account: DeployedValidator::unsafe_pull(validators.aleph_account, explorer).await,
            aleph_batch_witness: DeployedValidator::unsafe_pull(validators.aleph_batch_witness, explorer)
                .await,
        }
    }
}

fn green_protocol_deployment_from_refs(
    refs: DeployedValidators,
    royalty_pool_ledger_fixed: DeployedValidator<{ ProtocolValidator::RoyaltyPoolV1LedgerFixed as u8 }>,
    script_anchor: TransactionUnspentOutput,
) -> ProtocolDeployment {
    ProtocolDeployment {
        limit_order_witness: materialize_ref(refs.limit_order_witness, &script_anchor),
        limit_order: materialize_ref(refs.limit_order, &script_anchor),
        instant_order_witness: materialize_ref(refs.instant_order_witness, &script_anchor),
        instant_order: materialize_ref(refs.instant_order, &script_anchor),
        grid_order_native: materialize_ref(refs.grid_order_native, &script_anchor),
        const_fn_pool_v1: materialize_ref(refs.const_fn_pool_v1, &script_anchor),
        const_fn_pool_v2: materialize_ref(refs.const_fn_pool_v2, &script_anchor),
        const_fn_pool_fee_switch: materialize_ref(refs.const_fn_pool_fee_switch, &script_anchor),
        const_fn_pool_fee_switch_v2: materialize_ref(refs.const_fn_pool_fee_switch_v2, &script_anchor),
        const_fn_pool_fee_switch_bidir_fee: materialize_ref(
            refs.const_fn_pool_fee_switch_bidir_fee,
            &script_anchor,
        ),
        const_fn_pool_swap: materialize_ref(refs.const_fn_pool_swap, &script_anchor),
        const_fn_pool_deposit: materialize_ref(refs.const_fn_pool_deposit, &script_anchor),
        const_fn_pool_redeem: materialize_ref(refs.const_fn_pool_redeem, &script_anchor),
        const_fn_fee_switch_pool_swap: materialize_ref(refs.const_fn_fee_switch_pool_swap, &script_anchor),
        const_fn_fee_switch_pool_deposit: materialize_ref(
            refs.const_fn_fee_switch_pool_deposit,
            &script_anchor,
        ),
        const_fn_fee_switch_pool_redeem: materialize_ref(
            refs.const_fn_fee_switch_pool_redeem,
            &script_anchor,
        ),
        balance_fn_pool_v1: materialize_ref(refs.balance_fn_pool_v1, &script_anchor),
        balance_fn_pool_v2: materialize_ref(refs.balance_fn_pool_v2, &script_anchor),
        balance_fn_pool_deposit: materialize_ref(refs.balance_fn_pool_deposit, &script_anchor),
        balance_fn_pool_redeem: materialize_ref(refs.balance_fn_pool_redeem, &script_anchor),
        stable_fn_pool_t2t: materialize_ref(refs.stable_fn_pool_t2t, &script_anchor),
        stable_fn_pool_t2t_deposit: materialize_ref(refs.stable_fn_pool_t2t_deposit, &script_anchor),
        stable_fn_pool_t2t_redeem: materialize_ref(refs.stable_fn_pool_t2t_redeem, &script_anchor),
        royalty_pool: materialize_ref(refs.royalty_pool, &script_anchor),
        royalty_pool_ledger_fixed,
        royalty_pool_v2: materialize_ref(refs.royalty_pool_v2, &script_anchor),
        royalty_pool_deposit: materialize_ref(refs.royalty_pool_deposit, &script_anchor),
        royalty_pool_deposit_v2: materialize_ref(refs.royalty_pool_deposit_v2, &script_anchor),
        royalty_pool_redeem: materialize_ref(refs.royalty_pool_redeem, &script_anchor),
        royalty_pool_redeem_v2: materialize_ref(refs.royalty_pool_redeem_v2, &script_anchor),
        royalty_pool_royalty_withdraw_request: materialize_ref(
            refs.royalty_pool_withdraw_request,
            &script_anchor,
        ),
        royalty_pool_v2_royalty_withdraw_request: materialize_ref(
            refs.royalty_pool_v2_withdraw_request,
            &script_anchor,
        ),
        royalty_pool_dao_request: materialize_ref(refs.royalty_pool_dao_request, &script_anchor),
        royalty_pool_v2_dao_request: materialize_ref(refs.royalty_pool_v2_dao_request, &script_anchor),
        royalty_pool_dao: materialize_ref(refs.royalty_pool_dao_contract, &script_anchor),
        royalty_pool_v2_dao: materialize_ref(refs.royalty_pool_dao_contract_v2, &script_anchor),
        royalty_pool_withdraw: materialize_ref(refs.royalty_pool_withdraw_contract, &script_anchor),
        royalty_pool_withdraw_ledger_fixed: materialize_ref(
            refs.royalty_pool_withdraw_contract_ledger_fixed,
            &script_anchor,
        ),
        royalty_pool_withdraw_v2: materialize_ref(refs.royalty_pool_withdraw_contract_v2, &script_anchor),
    }
}

fn materialize_ref<const TYP: u8>(
    reference: DeployedValidatorRef,
    script_anchor: &TransactionUnspentOutput,
) -> DeployedValidator<TYP> {
    DeployedValidator {
        reference_utxo: script_anchor.clone(),
        hash: reference.hash,
        cost: reference.cost,
        marginal_cost: reference.marginal_cost.unwrap_or(ExUnits { mem: 0, steps: 0 }),
    }
}

impl From<&GreenProtocolDeployment> for GreenScriptHashes {
    fn from(deployment: &GreenProtocolDeployment) -> Self {
        Self {
            spectrum: ProtocolScriptHashes::from(&deployment.spectrum),
            aleph_account: DeployedScriptInfo::from(&deployment.aleph_account),
            aleph_batch_witness: DeployedScriptInfo::from(&deployment.aleph_batch_witness),
        }
    }
}

impl Has<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>> for GreenScriptHashes {
    fn select<U: IsEqual<DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }>>>(
        &self,
    ) -> DeployedScriptInfo<{ ALEPH_ACCOUNT_VALIDATOR }> {
        self.aleph_account
    }
}

impl Has<DeployedScriptInfo<{ ALEPH_BATCH_WITNESS_VALIDATOR }>> for GreenScriptHashes {
    fn select<U: IsEqual<DeployedScriptInfo<{ ALEPH_BATCH_WITNESS_VALIDATOR }>>>(
        &self,
    ) -> DeployedScriptInfo<{ ALEPH_BATCH_WITNESS_VALIDATOR }> {
        self.aleph_batch_witness
    }
}

#[cfg(test)]
mod tests {
    use super::GreenDeployedValidators;

    #[test]
    fn parses_green_deployment_with_aleph_validators() {
        let raw = include_str!("../resources/preprod.deployment.json");
        let deployment: GreenDeployedValidators = serde_json::from_str(raw).unwrap();
        assert_eq!(deployment.aleph_account.hash.to_string().len(), 56);
        assert_eq!(deployment.aleph_batch_witness.hash.to_string().len(), 56);
    }
}
