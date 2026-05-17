use cml_chain::transaction::TransactionOutput;
use either::Either;

use bloom_offchain::execution_engine::bundled::Bundled;
use bloom_offchain_cardano::orders::green::GreenOrder;
use bloom_offchain_cardano::pools::royalty_v1::RoyaltyV1PoolOnly;
use spectrum_cardano_lib::output::FinalizedTxOut;
use spectrum_cardano_lib::{OutputRef, Token};
use spectrum_offchain::domain::{Baked, EntitySnapshot, Has, Stable, Tradable};
use spectrum_offchain::ledger::TryFromLedger;
use spectrum_offchain_cardano::creds::OperatorCred;
use spectrum_offchain_cardano::data::pair::PairId;
use spectrum_offchain_cardano::data::pool::PoolValidation;
use spectrum_offchain_cardano::deployment::DeployedScriptInfo;
use spectrum_offchain_cardano::deployment::ProtocolValidator::{RoyaltyPoolV1, RoyaltyPoolV1LedgerFixed};
use spectrum_offchain_cardano::handler_context::{ConsumedInputs, ProducedIdentifiers};

#[repr(transparent)]
#[derive(Debug, Clone)]
pub struct EvolvingCardanoEntity(
    pub Bundled<Either<Baked<GreenOrder, OutputRef>, Baked<RoyaltyV1PoolOnly, OutputRef>>, FinalizedTxOut>,
);

impl Stable for EvolvingCardanoEntity {
    type StableId = Token;
    fn stable_id(&self) -> Self::StableId {
        self.0.stable_id()
    }
    fn is_quasi_permanent(&self) -> bool {
        self.0.is_quasi_permanent()
    }
}

impl EntitySnapshot for EvolvingCardanoEntity {
    type Version = OutputRef;
    fn version(&self) -> Self::Version {
        self.0.version()
    }
}

impl Tradable for EvolvingCardanoEntity {
    type PairId = PairId;
    fn pair_id(&self) -> Self::PairId {
        self.0.pair_id()
    }
}

impl<C> TryFromLedger<TransactionOutput, C> for EvolvingCardanoEntity
where
    C: Clone
        + Has<OperatorCred>
        + Has<OutputRef>
        + Has<ConsumedInputs>
        + Has<ProducedIdentifiers<Token>>
        + Has<DeployedScriptInfo<{ RoyaltyPoolV1 as u8 }>>
        + Has<DeployedScriptInfo<{ RoyaltyPoolV1LedgerFixed as u8 }>>
        + Has<PoolValidation>,
{
    fn try_from_ledger(repr: &TransactionOutput, ctx: &C) -> Option<Self> {
        RoyaltyV1PoolOnly::try_from_ledger(repr, ctx).map(|pool| {
            Self(Bundled(
                Either::Right(Baked::new(pool, ctx.select::<OutputRef>())),
                FinalizedTxOut::new(repr.clone(), ctx.select::<OutputRef>()),
            ))
        })
    }
}
