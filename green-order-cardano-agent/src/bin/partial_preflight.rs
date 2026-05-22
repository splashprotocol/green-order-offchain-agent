use bloom_offchain::execution_engine::liquidity_book::market_maker::MarketMaker;
use bloom_offchain::execution_engine::liquidity_book::types::RelativePrice;
use num_rational::Ratio;
use serde::Serialize;
use spectrum_cardano_lib::{AssetClass, AssetName, TaggedAmount, TaggedAssetClass, Token};
use spectrum_offchain_cardano::constants::FEE_DEN;
use spectrum_offchain_cardano::data::cfmm_pool::royalty_pool::{RoyaltyPool, RoyaltyPoolVer};
use spectrum_offchain_cardano::data::pool::{Lq, PoolValidation, Rx, Ry};
use spectrum_offchain_cardano::data::PoolId;

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    pool_lovelace_reserve: String,
    pool_token_reserve: String,
    pool_asset_y: String,
    leaving_lovelace: String,
    expected_token_amount: String,
    fee_lovelace: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    consumed_leaving: u64,
    received_output: u64,
    remaining_leaving: u64,
    remaining_expected_output: u64,
    remaining_fee: u64,
}

fn main() -> Result<(), String> {
    let input: Input = serde_json::from_reader(std::io::stdin()).map_err(|err| err.to_string())?;
    let pool_lovelace_reserve = parse_u64("poolLovelaceReserve", &input.pool_lovelace_reserve)?;
    let pool_token_reserve = parse_u64("poolTokenReserve", &input.pool_token_reserve)?;
    let leaving_lovelace = parse_u64("leavingLovelace", &input.leaving_lovelace)?;
    let expected_token_amount = parse_u64("expectedTokenAmount", &input.expected_token_amount)?;
    let fee_lovelace = parse_u64("feeLovelace", &input.fee_lovelace)?;
    let asset_y = parse_asset(&input.pool_asset_y)?;
    let pool = royalty_pool(pool_lovelace_reserve, pool_token_reserve, asset_y);
    let worst_price = bloom_offchain::execution_engine::liquidity_book::side::Side::Ask.wrap(
        bloom_offchain::execution_engine::liquidity_book::types::AbsolutePrice::from_price(
            bloom_offchain::execution_engine::liquidity_book::side::Side::Ask,
            RelativePrice::new(expected_token_amount as u128, leaving_lovelace as u128),
        ),
    );
    let available = pool
        .available_liquidity_on_side(worst_price)
        .ok_or_else(|| "pool returned no available liquidity".to_string())?;
    let consumed = available.input;
    let trade = pool
        .estimated_trade(bloom_offchain::execution_engine::liquidity_book::side::OnSide::Ask(consumed))
        .ok_or_else(|| "pool returned no estimated trade".to_string())?;
    if consumed == 0 || consumed >= leaving_lovelace {
        return Err("preflight is not strict partial: consumed input is not a proper prefix".to_string());
    }
    if trade.output == 0 || trade.output >= expected_token_amount {
        return Err(
            "preflight is not strict partial: output is zero or fully satisfies expectation".to_string(),
        );
    }
    let remaining_leaving = leaving_lovelace - consumed;
    let remaining_fee = remaining_leaving
        .saturating_mul(fee_lovelace)
        .checked_div(leaving_lovelace)
        .ok_or_else(|| "leaving amount is zero".to_string())?;
    serde_json::to_writer_pretty(
        std::io::stdout(),
        &Output {
            consumed_leaving: consumed,
            received_output: trade.output,
            remaining_leaving,
            remaining_expected_output: expected_token_amount - trade.output,
            remaining_fee,
        },
    )
    .map_err(|err| err.to_string())?;
    println!();
    Ok(())
}

fn royalty_pool(lovelace: u64, token: u64, asset_y: AssetClass) -> RoyaltyPool {
    RoyaltyPool {
        id: PoolId(Token(
            cml_chain::PolicyId::from([0; 28]),
            AssetName::from_utf8("partial-preflight".to_string()),
        )),
        reserves_x: TaggedAmount::<Rx>::new(lovelace),
        reserves_y: TaggedAmount::<Ry>::new(token),
        liquidity: TaggedAmount::<Lq>::new(40_000_000),
        asset_x: TaggedAssetClass::<Rx>::new(AssetClass::Native),
        asset_y: TaggedAssetClass::<Ry>::new(asset_y),
        asset_lq: TaggedAssetClass::<Lq>::new(asset_y),
        lp_fee: Ratio::new_raw(99_700, FEE_DEN),
        treasury_fee: Ratio::new(0, 1),
        treasury_x: TaggedAmount::<Rx>::new(0),
        treasury_y: TaggedAmount::<Ry>::new(0),
        first_royalty_fee: Ratio::new(0, 1),
        second_royalty_fee: Ratio::new(0, 1),
        first_royalty_x: TaggedAmount::<Rx>::new(0),
        first_royalty_y: TaggedAmount::<Ry>::new(0),
        second_royalty_x: TaggedAmount::<Rx>::new(0),
        second_royalty_y: TaggedAmount::<Ry>::new(0),
        lq_lower_bound: TaggedAmount::<Rx>::new(0),
        admin_address: cml_crypto::ScriptHash::from([0; 28]),
        treasury_address: cml_crypto::ScriptHash::from([0; 28]),
        first_royalty_pub_key: [0; 32].into(),
        second_royalty_pub_key: [0; 32].into(),
        ver: RoyaltyPoolVer::V1LedgerFixed,
        marginal_cost: spectrum_cardano_lib::ex_units::ExUnits { mem: 0, steps: 0 },
        bounds: PoolValidation {
            min_n2t_lovelace: 0,
            min_t2t_lovelace: 0,
        },
        nonce: 0,
        stake_part_script_hash: None,
    }
}

fn parse_asset(hex: &str) -> Result<AssetClass, String> {
    let bytes = hex::decode(hex).map_err(|err| err.to_string())?;
    AssetClass::from_bytes(&bytes).ok_or_else(|| format!("invalid asset class {}", hex))
}

fn parse_u64(name: &str, value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|err| format!("{} must be a u64 decimal string: {}", name, err))
}
