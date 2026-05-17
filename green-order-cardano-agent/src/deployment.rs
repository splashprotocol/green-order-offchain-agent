use bloom_offchain_cardano::orders::green::{ALEPH_ACCOUNT_VALIDATOR, ALEPH_BATCH_WITNESS_VALIDATOR};
use cardano_explorer::CardanoNetwork;
use spectrum_offchain::domain::Has;
use spectrum_offchain_cardano::deployment::{
    DeployedScriptInfo, DeployedValidator, DeployedValidatorRef, DeployedValidators, ProtocolDeployment,
    ProtocolScriptHashes,
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
        Self {
            spectrum: ProtocolDeployment::unsafe_pull(validators.spectrum, explorer).await,
            aleph_account: DeployedValidator::unsafe_pull(validators.aleph_account, explorer).await,
            aleph_batch_witness: DeployedValidator::unsafe_pull(validators.aleph_batch_witness, explorer)
                .await,
        }
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
