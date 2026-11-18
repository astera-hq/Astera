#![cfg(test)]

// #1036: collateral-risk-response satellite — auction reads pool's public
// getters (get_collateral_deposit/get_funded_invoice/get_asset_price),
// tracks the at-risk flag in its own storage, and drives pool's trusted
// risk_liquidate_collateral entrypoint to seize cross-asset collateral
// positions based on live, oracle-priced ratios.

use auction::{AuctionContract, AuctionContractClient, AuctionError};
use pool::{AdminOperation, FundingPool, FundingPoolClient};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    token, Address, Env, Symbol,
};

#[contract]
pub struct DummyShare;

#[contractimpl]
impl DummyShare {
    pub fn total_supply(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "tot"))
            .unwrap_or(0)
    }
    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage().persistent().get(&id).unwrap_or(0)
    }
    pub fn mint(env: Env, to: Address, amount: i128) {
        let t = Self::total_supply(env.clone());
        let b = Self::balance(env.clone(), to.clone());
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "tot"), &(t + amount));
        env.storage().persistent().set(&to, &(b + amount));
    }
    pub fn burn(env: Env, from: Address, amount: i128) {
        let t = Self::total_supply(env.clone());
        let b = Self::balance(env.clone(), from.clone());
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "tot"), &(t - amount));
        env.storage().persistent().set(&from, &(b - amount));
    }
    pub fn decimals(_env: Env) -> u32 {
        7
    }
}

#[contract]
pub struct DummyInvoice;

#[contractimpl]
impl DummyInvoice {
    pub fn get_authorized_pool(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "pool"))
            .expect("not initialized")
    }
    pub fn set_pool(env: Env, pool: Address) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "pool"), &pool);
    }
    pub fn record_funding(_env: Env, _id: u64, _amount: i128, _pool: Address) {}
}

const OPERATION_DELAY_SECS: u64 = 86_400;

fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token_id).mint(to, &amount);
}

/// Registers pool + auction, wires auction as pool's trusted risk contract,
/// lowers pool's collateral threshold to 0 (collateral required on every
/// principal) at 20% bps, and returns a ready-to-fund cross-asset setup:
/// usdc is the funding token, xlm is a second accepted token used as
/// collateral.
#[allow(clippy::type_complexity)]
fn setup(
    env: &Env,
) -> (
    FundingPoolClient<'_>,
    AuctionContractClient<'_>,
    Address,
    Address,
    Address,
    Address,
) {
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);
    let pool_id = env.register(FundingPool, ());
    let pool_client = FundingPoolClient::new(env, &pool_id);
    let admin = Address::generate(env);
    let usdc_id = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();
    let invoice_contract = env.register(DummyInvoice, ());
    DummyInvoiceClient::new(env, &invoice_contract).set_pool(&pool_id);
    let share_token = env.register(DummyShare, ());
    pool_client.initialize(&admin, &usdc_id, &share_token, &invoice_contract);
    pool_client.set_max_investor_concentration(&admin, &10_000u32);

    let xlm_id = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();
    pool_client.add_token(&admin, &xlm_id, &share_token);

    let auction_id = env.register(AuctionContract, ());
    let auction_client = AuctionContractClient::new(env, &auction_id);
    auction_client.initialize(&admin, &pool_id);
    pool_client.set_risk_contract(&admin, &auction_id);

    // threshold=0 => collateral required on every principal; 20% collateral_bps.
    let proposal_id = pool_client.propose_operation(
        &admin,
        &AdminOperation::SetCollateralConfig(0i128, 2_000u32),
    );
    env.ledger()
        .with_mut(|l| l.timestamp += OPERATION_DELAY_SECS + 1);
    pool_client.execute_operation(&admin, &proposal_id);

    (
        pool_client,
        auction_client,
        admin,
        usdc_id,
        xlm_id,
        invoice_contract,
    )
}

/// Funds invoice #1 with `principal` in usdc, collateralized by `xlm_amount`
/// of xlm, given usdc/xlm fallback prices already set by the caller.
fn fund_cross_asset_invoice(
    env: &Env,
    pool_client: &FundingPoolClient<'_>,
    admin: &Address,
    usdc_id: &Address,
    xlm_id: &Address,
    principal: i128,
    xlm_amount: i128,
) {
    let investor = Address::generate(env);
    let sme = Address::generate(env);
    mint(env, usdc_id, &investor, principal * 3);
    mint(env, xlm_id, &sme, xlm_amount);
    pool_client.deposit(&investor, usdc_id, &(principal * 3), &None);
    pool_client.deposit_collateral(&1u64, &sme, xlm_id, &xlm_amount);
    let due_date = env.ledger().timestamp() + 10_000_000;
    pool_client.fund_invoice(admin, &1u64, &principal, &sme, &due_date, usdc_id);
}

#[test]
fn test_check_collateral_risk_flags_when_price_drops() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, auction_client, admin, usdc_id, xlm_id, _inv) = setup(&env);
    let keeper = Address::generate(&env);

    pool_client.set_fallback_price(&admin, &usdc_id, &1_000_000i128);
    pool_client.set_fallback_price(&admin, &xlm_id, &500_000i128);

    let principal = 10_000i128;
    // required = 2,000 usdc-equivalent; 6,000 xlm @ 500,000 = 150% covered.
    fund_cross_asset_invoice(
        &env,
        &pool_client,
        &admin,
        &usdc_id,
        &xlm_id,
        principal,
        6_000,
    );
    assert!(pool_client.get_funded_invoice(&1u64).is_some());
    assert!(auction_client.get_at_risk_since(&1u64).is_none());

    // Price drop: xlm now worth 300,000 instead of 500,000 -> ratio falls to
    // 90% (6,000 * 300,000 / (2,000 * 1,000,000)), below both the funding-time
    // 100% floor and the default 120% danger threshold.
    pool_client.set_fallback_price(&admin, &xlm_id, &300_000i128);

    let ratio = auction_client.get_live_collateral_ratio(&1u64);
    assert_eq!(ratio, 9_000u32);

    let at_risk = auction_client.check_collateral_risk(&keeper, &1u64);
    assert!(at_risk);
    assert!(auction_client.get_at_risk_since(&1u64).is_some());
}

#[test]
fn test_check_collateral_risk_clears_on_recovery() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, auction_client, admin, usdc_id, xlm_id, _inv) = setup(&env);
    let keeper = Address::generate(&env);

    pool_client.set_fallback_price(&admin, &usdc_id, &1_000_000i128);
    pool_client.set_fallback_price(&admin, &xlm_id, &500_000i128);
    let principal = 10_000i128;
    fund_cross_asset_invoice(
        &env,
        &pool_client,
        &admin,
        &usdc_id,
        &xlm_id,
        principal,
        6_000,
    );

    pool_client.set_fallback_price(&admin, &xlm_id, &300_000i128);
    assert!(auction_client.check_collateral_risk(&keeper, &1u64));
    assert!(auction_client.get_at_risk_since(&1u64).is_some());

    // Price recovers well above the danger threshold (180%).
    pool_client.set_fallback_price(&admin, &xlm_id, &600_000i128);
    let at_risk = auction_client.check_collateral_risk(&keeper, &1u64);
    assert!(!at_risk);
    assert!(auction_client.get_at_risk_since(&1u64).is_none());
}

#[test]
fn test_liquidate_collateral_after_grace_period_elapses() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, auction_client, admin, usdc_id, xlm_id, _inv) = setup(&env);
    let keeper = Address::generate(&env);

    pool_client.set_fallback_price(&admin, &usdc_id, &1_000_000i128);
    pool_client.set_fallback_price(&admin, &xlm_id, &500_000i128);
    let principal = 10_000i128;
    fund_cross_asset_invoice(
        &env,
        &pool_client,
        &admin,
        &usdc_id,
        &xlm_id,
        principal,
        6_000,
    );

    pool_client.set_fallback_price(&admin, &xlm_id, &300_000i128);
    assert!(auction_client.check_collateral_risk(&keeper, &1u64));

    // Refresh the fallback prices right before the grace period elapses so
    // liquidation isn't blocked by staleness in this test — that path is
    // covered separately by test_liquidate_collateral_rejected_on_stale_oracle.
    let grace = auction_client
        .get_collateral_risk_config()
        .grace_period_secs;
    env.ledger().with_mut(|l| l.timestamp += grace - 10);
    pool_client.set_fallback_price(&admin, &usdc_id, &1_000_000i128);
    pool_client.set_fallback_price(&admin, &xlm_id, &300_000i128);
    env.ledger().with_mut(|l| l.timestamp += 20);

    auction_client.liquidate_collateral(&keeper, &1u64);

    let deposit = pool_client.get_collateral_deposit(&1u64).unwrap();
    assert!(deposit.settled);
    assert_eq!(deposit.seized_at, env.ledger().timestamp());

    // #1476: Verify the collateral sale was actually created
    let sale = auction_client.get_sale(&1u64).expect("Sale should be created");
    assert_eq!(sale.token, usdc_id);
}

#[test]
fn test_liquidate_collateral_rejected_before_grace_period_elapses() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, auction_client, admin, usdc_id, xlm_id, _inv) = setup(&env);
    let keeper = Address::generate(&env);

    pool_client.set_fallback_price(&admin, &usdc_id, &1_000_000i128);
    pool_client.set_fallback_price(&admin, &xlm_id, &500_000i128);
    let principal = 10_000i128;
    fund_cross_asset_invoice(
        &env,
        &pool_client,
        &admin,
        &usdc_id,
        &xlm_id,
        principal,
        6_000,
    );

    pool_client.set_fallback_price(&admin, &xlm_id, &300_000i128);
    assert!(auction_client.check_collateral_risk(&keeper, &1u64));

    let result = auction_client.try_liquidate_collateral(&keeper, &1u64);
    assert_eq!(result, Err(Ok(AuctionError::GracePeriodNotElapsed)));
    assert!(!pool_client.get_collateral_deposit(&1u64).unwrap().settled);
}

#[test]
fn test_liquidate_collateral_rejected_when_position_recovers() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, auction_client, admin, usdc_id, xlm_id, _inv) = setup(&env);
    let keeper = Address::generate(&env);

    pool_client.set_fallback_price(&admin, &usdc_id, &1_000_000i128);
    pool_client.set_fallback_price(&admin, &xlm_id, &500_000i128);
    let principal = 10_000i128;
    fund_cross_asset_invoice(
        &env,
        &pool_client,
        &admin,
        &usdc_id,
        &xlm_id,
        principal,
        6_000,
    );

    pool_client.set_fallback_price(&admin, &xlm_id, &300_000i128);
    assert!(auction_client.check_collateral_risk(&keeper, &1u64));

    let grace = auction_client
        .get_collateral_risk_config()
        .grace_period_secs;
    env.ledger().with_mut(|l| l.timestamp += grace - 10);
    // Refresh + recover: price is back above the danger threshold by the
    // time the grace period elapses.
    pool_client.set_fallback_price(&admin, &usdc_id, &1_000_000i128);
    pool_client.set_fallback_price(&admin, &xlm_id, &600_000i128);
    env.ledger().with_mut(|l| l.timestamp += 20);

    // Succeeds (no error) but reports no liquidation happened — the stale
    // local flag is cleared either way.
    let liquidated = auction_client.liquidate_collateral(&keeper, &1u64);
    assert!(!liquidated);
    assert!(!pool_client.get_collateral_deposit(&1u64).unwrap().settled);
    assert!(auction_client.get_at_risk_since(&1u64).is_none());
}

#[test]
fn test_liquidate_collateral_rejected_on_stale_oracle() {
    let env = Env::default();
    env.mock_all_auths();
    let (pool_client, auction_client, admin, usdc_id, xlm_id, _inv) = setup(&env);
    let keeper = Address::generate(&env);

    pool_client.set_fallback_price(&admin, &usdc_id, &1_000_000i128);
    pool_client.set_fallback_price(&admin, &xlm_id, &500_000i128);
    let principal = 10_000i128;
    fund_cross_asset_invoice(
        &env,
        &pool_client,
        &admin,
        &usdc_id,
        &xlm_id,
        principal,
        6_000,
    );

    pool_client.set_fallback_price(&admin, &xlm_id, &300_000i128);
    assert!(auction_client.check_collateral_risk(&keeper, &1u64));

    // Let the grace period elapse WITHOUT refreshing the fallback prices —
    // pool's default oracle staleness window (1 hour) is far shorter than
    // the default 3-day grace period, so by the time the grace period is up
    // the last-set prices are themselves stale. This must block liquidation
    // rather than let a stale reading justify a seizure.
    let grace = auction_client
        .get_collateral_risk_config()
        .grace_period_secs;
    env.ledger().with_mut(|l| l.timestamp += grace + 1);

    let result = auction_client.try_liquidate_collateral(&keeper, &1u64);
    assert_eq!(result, Err(Ok(AuctionError::OraclePriceUnavailable)));
    assert!(!pool_client.get_collateral_deposit(&1u64).unwrap().settled);
}

#[test]
fn test_set_collateral_risk_config_rejects_invalid_values() {
    let env = Env::default();
    env.mock_all_auths();
    let (_pool_client, auction_client, admin, _usdc_id, _xlm_id, _inv) = setup(&env);

    // danger_bps at or below 100% leaves no early-warning buffer.
    let result = auction_client.try_set_collateral_risk_config(&admin, &10_000u32, &259_200u64);
    assert_eq!(result, Err(Ok(AuctionError::InvalidRiskConfig)));

    // grace_period_secs of 0 gives keepers no window to react at all.
    let result = auction_client.try_set_collateral_risk_config(&admin, &12_000u32, &0u64);
    assert_eq!(result, Err(Ok(AuctionError::InvalidRiskConfig)));
}

#[test]
fn test_set_collateral_risk_config_rejects_non_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (_pool_client, auction_client, _admin, _usdc_id, _xlm_id, _inv) = setup(&env);
    let intruder = Address::generate(&env);

    let result = auction_client.try_set_collateral_risk_config(&intruder, &15_000u32, &259_200u64);
    assert_eq!(result, Err(Ok(AuctionError::Unauthorized)));
}
