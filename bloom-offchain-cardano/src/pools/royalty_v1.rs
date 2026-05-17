use std::fmt::{Display, Formatter};

use bloom_offchain::execution_engine::liquidity_book::core::Next;
use bloom_offchain::execution_engine::liquidity_book::market_maker::{
    AbsoluteReserves, AvailableLiquidity, MakerBehavior, MarketMaker, PoolQuality, SpotPrice,
};
use bloom_offchain::execution_engine::liquidity_book::side::OnSide;
use bloom_offchain::execution_engine::liquidity_book::types::AbsolutePrice;
use cml_chain::certs::StakeCredential;
use cml_chain::transaction::TransactionOutput;
use cml_core::serialization::RawBytesEncoding;
use cml_crypto::ScriptHash;
use num_rational::Ratio;
use spectrum_cardano_lib::ex_units::ExUnits;
use spectrum_cardano_lib::plutus_data::DatumExtension;
use spectrum_cardano_lib::transaction::TransactionOutputExtension;
use spectrum_cardano_lib::types::TryFromPData;
use spectrum_cardano_lib::value::ValueExtension;
use spectrum_cardano_lib::AssetClass::Native;
use spectrum_cardano_lib::{Ed25519PublicKey, TaggedAmount, Token};
use spectrum_offchain::domain::{Has, Stable, Tradable};
use spectrum_offchain::ledger::TryFromLedger;
use spectrum_offchain_cardano::constants::{FEE_DEN, MAX_LQ_CAP};
use spectrum_offchain_cardano::data::cfmm_pool::royalty_pool::{
    RoyaltyPool, RoyaltyPoolConfig, RoyaltyPoolVer,
};
use spectrum_offchain_cardano::data::pair::PairId;
use spectrum_offchain_cardano::data::pool::PoolValidation;
use spectrum_offchain_cardano::data::PoolId;
use spectrum_offchain_cardano::deployment::ProtocolValidator::{RoyaltyPoolV1, RoyaltyPoolV1LedgerFixed};
use spectrum_offchain_cardano::deployment::{
    DeployedScriptInfo, DeployedValidator, DeployedValidatorErased, RequiresValidator,
};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct RoyaltyV1PoolOnly {
    inner: RoyaltyPool,
}

impl RoyaltyV1PoolOnly {
    pub fn try_from_royalty_pool(pool: RoyaltyPool) -> Option<Self> {
        match pool.ver {
            RoyaltyPoolVer::V1 | RoyaltyPoolVer::V1LedgerFixed => Some(Self { inner: pool }),
            RoyaltyPoolVer::V2 => None,
        }
    }

    pub fn into_inner(self) -> RoyaltyPool {
        self.inner
    }

    pub fn inner(&self) -> &RoyaltyPool {
        &self.inner
    }

    fn try_pool_ver_from_address<C>(repr: &TransactionOutput, ctx: &C) -> Option<RoyaltyPoolVer>
    where
        C: Has<DeployedScriptInfo<{ RoyaltyPoolV1 as u8 }>>
            + Has<DeployedScriptInfo<{ RoyaltyPoolV1LedgerFixed as u8 }>>,
    {
        let maybe_hash = repr.address().payment_cred().and_then(|c| match c {
            StakeCredential::PubKey { .. } => None,
            StakeCredential::Script { hash, .. } => Some(hash),
        });

        let this_hash = maybe_hash?;
        if ctx
            .select::<DeployedScriptInfo<{ RoyaltyPoolV1 as u8 }>>()
            .script_hash
            == *this_hash
        {
            Some(RoyaltyPoolVer::V1)
        } else if ctx
            .select::<DeployedScriptInfo<{ RoyaltyPoolV1LedgerFixed as u8 }>>()
            .script_hash
            == *this_hash
        {
            Some(RoyaltyPoolVer::V1LedgerFixed)
        } else {
            None
        }
    }
}

impl Display for RoyaltyV1PoolOnly {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(format!("RoyaltyV1PoolOnly({})", self.inner).as_str())
    }
}

impl Stable for RoyaltyV1PoolOnly {
    type StableId = Token;

    fn stable_id(&self) -> Self::StableId {
        self.inner.id.0
    }

    fn is_quasi_permanent(&self) -> bool {
        true
    }
}

impl Tradable for RoyaltyV1PoolOnly {
    type PairId = PairId;

    fn pair_id(&self) -> Self::PairId {
        PairId::canonical(self.inner.asset_x.untag(), self.inner.asset_y.untag())
    }
}

impl MakerBehavior for RoyaltyV1PoolOnly {
    fn swap(self, input: OnSide<u64>) -> Next<Self, void::Void> {
        self.inner.swap(input).map_succ(|inner| Self { inner })
    }

    fn preserve_preview_metadata(self, previewed: Self, rebalanced: Self) -> Self {
        Self {
            inner: self
                .inner
                .preserve_preview_metadata(previewed.inner, rebalanced.inner),
        }
    }
}

impl MarketMaker for RoyaltyV1PoolOnly {
    type U = ExUnits;

    fn static_price(&self) -> SpotPrice {
        self.inner.static_price()
    }

    fn real_price(&self, input: OnSide<u64>) -> Option<AbsolutePrice> {
        self.inner.real_price(input)
    }

    fn quality(&self) -> PoolQuality {
        self.inner.quality()
    }

    fn marginal_cost_hint(&self) -> Self::U {
        self.inner.marginal_cost_hint()
    }

    fn liquidity(&self) -> AbsoluteReserves {
        self.inner.liquidity()
    }

    fn available_liquidity_on_side(&self, worst_price: OnSide<AbsolutePrice>) -> Option<AvailableLiquidity> {
        self.inner.available_liquidity_on_side(worst_price)
    }

    fn estimated_trade(&self, input: OnSide<u64>) -> Option<AvailableLiquidity> {
        self.inner.estimated_trade(input)
    }

    fn is_active(&self) -> bool {
        self.inner.is_active()
    }
}

impl<C> TryFromLedger<TransactionOutput, C> for RoyaltyV1PoolOnly
where
    C: Has<DeployedScriptInfo<{ RoyaltyPoolV1 as u8 }>>
        + Has<DeployedScriptInfo<{ RoyaltyPoolV1LedgerFixed as u8 }>>
        + Has<PoolValidation>,
{
    fn try_from_ledger(repr: &TransactionOutput, ctx: &C) -> Option<Self> {
        let pool_ver = Self::try_pool_ver_from_address(repr, ctx)?;
        let value = repr.value();
        let pd = repr.datum().clone()?.into_pd()?;
        let bounds = ctx.select::<PoolValidation>();
        let stake_part = repr
            .address()
            .staking_cred()
            .and_then(|staking_cred| match staking_cred {
                StakeCredential::Script { hash, .. } => Some(*hash),
                _ => None,
            });
        let marginal_cost = match pool_ver {
            RoyaltyPoolVer::V1 => {
                ctx.select::<DeployedScriptInfo<{ RoyaltyPoolV1 as u8 }>>()
                    .marginal_cost
            }
            RoyaltyPoolVer::V1LedgerFixed => {
                ctx.select::<DeployedScriptInfo<{ RoyaltyPoolV1LedgerFixed as u8 }>>()
                    .marginal_cost
            }
            RoyaltyPoolVer::V2 => return None,
        };

        let conf = RoyaltyPoolConfig::try_from_pd(pd)?;
        let liquidity_neg = value.amount_of(conf.asset_lq.into())?;
        let lov = value.amount_of(Native)?;
        let reserves_x = value.amount_of(conf.asset_x.into())?;
        let reserves_y = value.amount_of(conf.asset_y.into())?;
        let pure_reserves_x = reserves_x.checked_sub(conf.treasury_x + conf.royalty_x)?;
        let pure_reserves_y = reserves_y.checked_sub(conf.treasury_y + conf.royalty_y)?;
        let sufficient_lovelace =
            conf.asset_x.is_native() || conf.asset_y.is_native() || bounds.min_t2t_lovelace <= lov;
        if pure_reserves_x == 0 || pure_reserves_y == 0 || !sufficient_lovelace {
            return None;
        }

        Some(Self {
            inner: RoyaltyPool {
                id: PoolId::try_from(conf.pool_nft).ok()?,
                reserves_x: TaggedAmount::new(reserves_x),
                reserves_y: TaggedAmount::new(reserves_y),
                liquidity: TaggedAmount::new(MAX_LQ_CAP - liquidity_neg),
                asset_x: conf.asset_x,
                asset_y: conf.asset_y,
                asset_lq: conf.asset_lq,
                lp_fee: Ratio::new_raw(conf.lp_fee_num, FEE_DEN),
                treasury_fee: Ratio::new_raw(conf.treasury_fee_num, FEE_DEN),
                treasury_x: TaggedAmount::new(conf.treasury_x),
                treasury_y: TaggedAmount::new(conf.treasury_y),
                first_royalty_fee: Ratio::new_raw(conf.royalty_fee_num, FEE_DEN),
                second_royalty_fee: Ratio::new_raw(0, 100),
                first_royalty_x: TaggedAmount::new(conf.royalty_x),
                first_royalty_y: TaggedAmount::new(conf.royalty_y),
                second_royalty_x: TaggedAmount::new(0),
                second_royalty_y: TaggedAmount::new(0),
                lq_lower_bound: TaggedAmount::new(0),
                admin_address: conf.admin_address,
                treasury_address: ScriptHash::from_raw_bytes(&conf.treasury_address).ok()?,
                first_royalty_pub_key: conf.royalty_pub_key.try_into().ok()?,
                second_royalty_pub_key: Ed25519PublicKey::from([0; 32]),
                ver: pool_ver,
                marginal_cost,
                bounds,
                nonce: conf.nonce,
                stake_part_script_hash: stake_part,
            },
        })
    }
}

impl<C> RequiresValidator<C> for RoyaltyV1PoolOnly
where
    C: Has<DeployedValidator<{ RoyaltyPoolV1 as u8 }>>
        + Has<DeployedValidator<{ RoyaltyPoolV1LedgerFixed as u8 }>>,
{
    fn get_validator(&self, ctx: &C) -> DeployedValidatorErased {
        match self.inner.ver {
            RoyaltyPoolVer::V1 => ctx
                .select::<DeployedValidator<{ RoyaltyPoolV1 as u8 }>>()
                .erased(),
            RoyaltyPoolVer::V1LedgerFixed => ctx
                .select::<DeployedValidator<{ RoyaltyPoolV1LedgerFixed as u8 }>>()
                .erased(),
            RoyaltyPoolVer::V2 => unreachable!("RoyaltyV1PoolOnly cannot contain Royalty V2"),
        }
    }
}

#[cfg(test)]
mod tests {
    use cml_chain::PolicyId;
    use cml_crypto::ScriptHash;
    use num_rational::Ratio;
    use spectrum_cardano_lib::ex_units::ExUnits;
    use spectrum_cardano_lib::{
        AssetClass, AssetName, Ed25519PublicKey, TaggedAmount, TaggedAssetClass, Token,
    };
    use spectrum_offchain::domain::{Stable, Tradable};
    use spectrum_offchain_cardano::data::cfmm_pool::royalty_pool::{RoyaltyPool, RoyaltyPoolVer};
    use spectrum_offchain_cardano::data::pair::PairId;
    use spectrum_offchain_cardano::data::pool::{Lq, PoolValidation, Rx, Ry};
    use spectrum_offchain_cardano::data::PoolId;

    use super::RoyaltyV1PoolOnly;

    fn token(seed: u8) -> Token {
        Token(
            PolicyId::from([seed; 28]),
            AssetName::from_utf8(format!("t{seed}")),
        )
    }

    fn tagged_asset<T>(asset: AssetClass) -> TaggedAssetClass<T> {
        TaggedAssetClass::new(asset)
    }

    fn royalty_pool(ver: RoyaltyPoolVer, asset_x: AssetClass, asset_y: AssetClass) -> RoyaltyPool {
        let pool_token = token(9);
        RoyaltyPool {
            id: PoolId(pool_token),
            reserves_x: TaggedAmount::new(1_000_000),
            reserves_y: TaggedAmount::new(2_000_000),
            liquidity: TaggedAmount::new(1_000),
            asset_x: tagged_asset::<Rx>(asset_x),
            asset_y: tagged_asset::<Ry>(asset_y),
            asset_lq: tagged_asset::<Lq>(AssetClass::Token(token(8))),
            lp_fee: Ratio::new_raw(3, 1_000),
            treasury_fee: Ratio::new_raw(0, 1_000),
            treasury_x: TaggedAmount::new(0),
            treasury_y: TaggedAmount::new(0),
            first_royalty_fee: Ratio::new_raw(0, 1_000),
            second_royalty_fee: Ratio::new_raw(0, 1_000),
            first_royalty_x: TaggedAmount::new(0),
            first_royalty_y: TaggedAmount::new(0),
            second_royalty_x: TaggedAmount::new(0),
            second_royalty_y: TaggedAmount::new(0),
            lq_lower_bound: TaggedAmount::new(0),
            admin_address: ScriptHash::from([1; 28]),
            treasury_address: ScriptHash::from([2; 28]),
            first_royalty_pub_key: Ed25519PublicKey::from([3; 32]),
            second_royalty_pub_key: Ed25519PublicKey::from([4; 32]),
            ver,
            marginal_cost: ExUnits { mem: 1, steps: 2 },
            bounds: PoolValidation {
                min_n2t_lovelace: 1,
                min_t2t_lovelace: 1,
            },
            nonce: 0,
            stake_part_script_hash: None,
        }
    }

    #[test]
    fn accepts_v1_and_v1_ledger_fixed_only() {
        assert!(RoyaltyV1PoolOnly::try_from_royalty_pool(royalty_pool(
            RoyaltyPoolVer::V1,
            AssetClass::Native,
            AssetClass::Token(token(1)),
        ))
        .is_some());

        assert!(RoyaltyV1PoolOnly::try_from_royalty_pool(royalty_pool(
            RoyaltyPoolVer::V1LedgerFixed,
            AssetClass::Native,
            AssetClass::Token(token(1)),
        ))
        .is_some());

        assert!(RoyaltyV1PoolOnly::try_from_royalty_pool(royalty_pool(
            RoyaltyPoolVer::V2,
            AssetClass::Native,
            AssetClass::Token(token(1)),
        ))
        .is_none());
    }

    #[test]
    fn stable_id_is_pool_nft_token_not_raw_pool_id() {
        let pool = royalty_pool(
            RoyaltyPoolVer::V1,
            AssetClass::Native,
            AssetClass::Token(token(1)),
        );
        let expected = pool.id.0;

        let royalty_v1 = RoyaltyV1PoolOnly::try_from_royalty_pool(pool).unwrap();

        assert_eq!(expected, royalty_v1.stable_id());
    }

    #[test]
    fn pair_id_is_canonical_from_assets() {
        let token_a = AssetClass::Token(token(1));
        let token_b = AssetClass::Token(token(2));
        let pool = royalty_pool(RoyaltyPoolVer::V1, token_b, token_a);

        let royalty_v1 = RoyaltyV1PoolOnly::try_from_royalty_pool(pool).unwrap();

        assert_eq!(PairId::canonical(token_a, token_b), royalty_v1.pair_id());
    }
}
