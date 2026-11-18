use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};
use tranche::{state::TrancheClass, TrancheContract};

/// Registers a real token contract: `deposit_tranche` performs actual token
/// transfers, so a plain generated address won't do.
fn create_token(env: &Env) -> Address {
    env.register_stellar_asset_contract_v2(Address::generate(env))
        .address()
}

fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token_id).mint(to, &amount);
}

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
        let total = Self::total_supply(env.clone());
        let balance = Self::balance(env.clone(), to.clone());
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "tot"), &(total + amount));
        env.storage().persistent().set(&to, &(balance + amount));
    }

    pub fn burn(env: Env, from: Address, amount: i128) {
        let total = Self::total_supply(env.clone());
        let balance = Self::balance(env.clone(), from.clone());
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "tot"), &(total - amount));
        env.storage().persistent().set(&from, &(balance - amount));
    }
}

#[test]
fn test_basic_deposit_and_withdraw() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = create_token(&env);
    let senior_share_token = env.register(DummyShare, ());
    let junior_share_token = env.register(DummyShare, ());

    let contract_id = env.register(TrancheContract, ());

    env.as_contract(&contract_id, || {
        TrancheContract::initialize(
            env.clone(),
            admin.clone(),
            token.clone(),
            senior_share_token.clone(),
            junior_share_token.clone(),
            tranche::state::TrancheConfig {
                senior_target_yield_bps: 1000,
                senior_advance_rate_bps: 8000,
                junior_first_loss_bps: 10000,
            },
        );
    });

    env.as_contract(&contract_id, || {
        TrancheContract::open_tranche_for_token(
            env.clone(),
            admin.clone(),
            token.clone(),
            senior_share_token.clone(),
            junior_share_token.clone(),
            tranche::state::TrancheConfig {
                senior_target_yield_bps: 1000,
                senior_advance_rate_bps: 8000,
                junior_first_loss_bps: 10000,
            },
        )
        .unwrap();
    });

    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);
    mint(&env, &token, &investor1, 1_000_000);
    mint(&env, &token, &investor2, 1_000_000);

    // Deposit junior first to create capacity for senior
    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor2.clone(),
            token.clone(),
            TrancheClass::Junior,
            2000,
        );
    });

    // Deposit to senior
    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor1.clone(),
            token.clone(),
            TrancheClass::Senior,
            5000,
        );
    });

    // Verify deposits
    env.as_contract(&contract_id, || {
        let pool = TrancheContract::get_pool(env.clone(), token.clone());
        assert_eq!(pool.senior.deposited, 5000);
        assert_eq!(pool.junior.deposited, 2000);
    });

    // Deposits mint share tokens 1:1 to the investor
    assert_eq!(
        DummyShareClient::new(&env, &senior_share_token).balance(&investor1),
        5000
    );
    assert_eq!(
        DummyShareClient::new(&env, &junior_share_token).balance(&investor2),
        2000
    );

    // Withdraw from senior
    env.as_contract(&contract_id, || {
        TrancheContract::withdraw_tranche(
            env.clone(),
            investor1.clone(),
            token.clone(),
            TrancheClass::Senior,
            1000,
        );
    });

    // Verify withdrawal
    env.as_contract(&contract_id, || {
        let pool = TrancheContract::get_pool(env.clone(), token.clone());
        assert_eq!(pool.senior.deposited, 4000);
    });

    // Withdrawals burn share tokens 1:1 from the investor
    assert_eq!(
        DummyShareClient::new(&env, &senior_share_token).balance(&investor1),
        4000
    );
}

#[test]
fn test_funding_and_repayment() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = create_token(&env);
    let senior_share_token = env.register(DummyShare, ());
    let junior_share_token = env.register(DummyShare, ());

    let contract_id = env.register(TrancheContract, ());

    env.as_contract(&contract_id, || {
        TrancheContract::initialize(
            env.clone(),
            admin.clone(),
            token.clone(),
            senior_share_token.clone(),
            junior_share_token.clone(),
            tranche::state::TrancheConfig {
                senior_target_yield_bps: 1000,
                senior_advance_rate_bps: 8000,
                junior_first_loss_bps: 10000,
            },
        );
    });

    env.as_contract(&contract_id, || {
        TrancheContract::open_tranche_for_token(
            env.clone(),
            admin.clone(),
            token.clone(),
            senior_share_token,
            junior_share_token,
            tranche::state::TrancheConfig {
                senior_target_yield_bps: 1000,
                senior_advance_rate_bps: 8000,
                junior_first_loss_bps: 10000,
            },
        )
        .unwrap();
    });

    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);
    mint(&env, &token, &investor1, 1_000_000);
    mint(&env, &token, &investor2, 1_000_000);

    // Deposit junior first to create capacity for senior
    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor2.clone(),
            token.clone(),
            TrancheClass::Junior,
            2000,
        );
    });

    // Deposit to senior
    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor1.clone(),
            token.clone(),
            TrancheClass::Senior,
            8000,
        );
    });

    // Fund an invoice
    env.as_contract(&contract_id, || {
        let (senior_amt, junior_amt) =
            TrancheContract::fund_invoice_from_tranches(env.clone(), token.clone(), 1, 1000);
        assert_eq!(senior_amt, 800); // 80% of 1000
        assert_eq!(junior_amt, 200); // 20% of 1000
    });

    // Verify funding state
    env.as_contract(&contract_id, || {
        let pool = TrancheContract::get_pool(env.clone(), token.clone());
        assert_eq!(pool.senior.deployed, 800);
        assert_eq!(pool.junior.deployed, 200);
        assert_eq!(pool.senior.available, 7200); // 8000 - 800
        assert_eq!(pool.junior.available, 1800); // 2000 - 200
    });

    // Repay the invoice
    env.as_contract(&contract_id, || {
        let (senior_payout, junior_payout) = TrancheContract::distribute_waterfall_repayment(
            env.clone(),
            token.clone(),
            1,
            1100,              // 10% yield
            30 * 24 * 60 * 60, // 30 days
        );
        // Senior should get capped amount, junior gets remainder
        assert!(senior_payout > 800);
        assert!(junior_payout > 200);
    });

    // Verify repayment state
    env.as_contract(&contract_id, || {
        let pool = TrancheContract::get_pool(env.clone(), token.clone());
        assert_eq!(pool.senior.deployed, 0); // Invoice repaid
        assert_eq!(pool.junior.deployed, 0);
        assert!(pool.senior.earned > 0);
        assert!(pool.junior.earned > 0);
    });
}

#[test]
fn test_full_scenario_five_invoices_mixed_outcomes() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = create_token(&env);
    let senior_share_token = env.register(DummyShare, ());
    let junior_share_token = env.register(DummyShare, ());

    let contract_id = env.register(TrancheContract, ());

    env.as_contract(&contract_id, || {
        TrancheContract::initialize(
            env.clone(),
            admin.clone(),
            token.clone(),
            senior_share_token.clone(),
            junior_share_token.clone(),
            tranche::state::TrancheConfig {
                senior_target_yield_bps: 1000,
                senior_advance_rate_bps: 8000,
                junior_first_loss_bps: 10000,
            },
        );
    });

    env.as_contract(&contract_id, || {
        TrancheContract::open_tranche_for_token(
            env.clone(),
            admin.clone(),
            token.clone(),
            senior_share_token,
            junior_share_token,
            tranche::state::TrancheConfig {
                senior_target_yield_bps: 1000,
                senior_advance_rate_bps: 8000,
                junior_first_loss_bps: 10000,
            },
        )
        .unwrap();
    });

    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);
    mint(&env, &token, &investor1, 1_000_000);
    mint(&env, &token, &investor2, 1_000_000);

    // Deposit junior first to create capacity for senior
    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor2.clone(),
            token.clone(),
            TrancheClass::Junior,
            2000,
        );
    });

    // Deposit to senior
    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor1.clone(),
            token.clone(),
            TrancheClass::Senior,
            8000,
        );
    });

    // Fund 5 invoices
    // Invoice 1: 1000 (800 senior, 200 junior)
    env.as_contract(&contract_id, || {
        let (senior1, junior1) =
            TrancheContract::fund_invoice_from_tranches(env.clone(), token.clone(), 1, 1000);
        assert_eq!(senior1, 800);
        assert_eq!(junior1, 200);
    });

    // Invoice 2: 1500 (1200 senior, 300 junior)
    env.as_contract(&contract_id, || {
        let (senior2, junior2) =
            TrancheContract::fund_invoice_from_tranches(env.clone(), token.clone(), 2, 1500);
        assert_eq!(senior2, 1200);
        assert_eq!(junior2, 300);
    });

    // Invoice 3: 2000 (1600 senior, 400 junior)
    env.as_contract(&contract_id, || {
        let (senior3, junior3) =
            TrancheContract::fund_invoice_from_tranches(env.clone(), token.clone(), 3, 2000);
        assert_eq!(senior3, 1600);
        assert_eq!(junior3, 400);
    });

    // Invoice 4: 1500 (1200 senior, 300 junior)
    env.as_contract(&contract_id, || {
        let (senior4, junior4) =
            TrancheContract::fund_invoice_from_tranches(env.clone(), token.clone(), 4, 1500);
        assert_eq!(senior4, 1200);
        assert_eq!(junior4, 300);
    });

    // Invoice 5: 1000 (800 senior, 200 junior)
    env.as_contract(&contract_id, || {
        let (senior5, junior5) =
            TrancheContract::fund_invoice_from_tranches(env.clone(), token.clone(), 5, 1000);
        assert_eq!(senior5, 800);
        assert_eq!(junior5, 200);
    });

    // Check totals after funding
    env.as_contract(&contract_id, || {
        let pool = TrancheContract::get_pool(env.clone(), token.clone());
        assert_eq!(pool.senior.deposited, 8000);
        assert_eq!(pool.senior.deployed, 5600); // 800+1200+1600+1200+800
        assert_eq!(pool.senior.available, 2400); // 8000-5600
        assert_eq!(pool.junior.deposited, 2000);
        assert_eq!(pool.junior.deployed, 1400); // 200+300+400+300+200
        assert_eq!(pool.junior.available, 600); // 2000-1400
    });

    // Invoice 1: Full repayment (1100: principal + 10% yield)
    env.as_contract(&contract_id, || {
        let (senior_payout1, junior_payout1) = TrancheContract::distribute_waterfall_repayment(
            env.clone(),
            token.clone(),
            1,
            1100,
            30 * 24 * 60 * 60, // 30 days
        );
        // Senior cap: 800 + (800 * 1000 * 30 days / 365 days / 10000) ≈ 800 + 6.58 = 806.58
        // For simplicity, let's check that senior gets capped amount and junior gets remainder
        assert!(senior_payout1 > 800 && senior_payout1 <= 810);
        assert!(junior_payout1 > 290);
    });

    // Invoice 2: Full repayment (1650)
    env.as_contract(&contract_id, || {
        let (senior_payout2, junior_payout2) = TrancheContract::distribute_waterfall_repayment(
            env.clone(),
            token.clone(),
            2,
            1650,
            30 * 24 * 60 * 60,
        );
        assert!(senior_payout2 > 1200 && senior_payout2 <= 1220);
        assert!(junior_payout2 > 430);
    });

    // Invoice 3: Full repayment (2200)
    env.as_contract(&contract_id, || {
        let (senior_payout3, junior_payout3) = TrancheContract::distribute_waterfall_repayment(
            env.clone(),
            token.clone(),
            3,
            2200,
            30 * 24 * 60 * 60,
        );
        assert!(senior_payout3 > 1600 && senior_payout3 <= 1630);
        assert!(junior_payout3 > 570);
    });

    // Invoice 4: Full default. A repayment settles an invoice completely
    // (its exposure is removed), so a default is modelled as no repayment
    // at all: the full 1500 funded amount is lost.
    env.as_contract(&contract_id, || {
        TrancheContract::allocate_loss(
            env.clone(),
            token.clone(),
            4,
            1500, // Full principal lost
        );
    });

    // Invoice 5: Full default with zero collateral (shortfall of 1000)
    env.as_contract(&contract_id, || {
        TrancheContract::allocate_loss(
            env.clone(),
            token.clone(),
            5,
            1000, // Full principal lost
        );
    });

    // Check final state
    env.as_contract(&contract_id, || {
        let pool_final = TrancheContract::get_pool(env.clone(), token.clone());

        // Senior should have earned from invoices 1, 2, 3, and partial from 4
        // Should have lost some from invoice 4 (after junior exhausted) and invoice 5
        assert!(pool_final.senior.earned > 0);
        assert!(pool_final.senior.losses > 0);

        // Junior should have earned from invoices 1, 2, 3
        // Should have absorbed losses from invoice 4 and 5
        assert!(pool_final.junior.earned > 0);
        assert!(pool_final.junior.losses > 0);

        // Verify that junior absorbed losses first
        // Total shortfall: 1000 (invoice 4) + 1000 (invoice 5) = 2000
        // Junior had 1400 deployed + 600 available = 2000 total
        // Junior should have absorbed significant losses, but may have earnings from successful invoices
        let junior_remaining =
            pool_final.junior.deposited + pool_final.junior.earned - pool_final.junior.losses;
        assert!(
            junior_remaining < pool_final.junior.deposited,
            "Junior should have lost principal"
        );
        assert!(
            pool_final.junior.losses > 1500,
            "Junior should have absorbed most losses"
        );

        // Senior should have taken remaining loss after junior was exhausted
        // Senior had 5600 deployed - 800 (invoice 1) - 1200 (invoice 2) - 1600 (invoice 3) - 500 (partial invoice 4) = 1500 remaining
        // After junior exhausted (2000 loss absorbed), senior takes remaining 1000 loss
        assert!(pool_final.senior.losses > 0);
    });
}

#[test]
fn test_junior_absorbs_full_default_senior_whole() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = create_token(&env);
    let senior_share_token = env.register(DummyShare, ());
    let junior_share_token = env.register(DummyShare, ());

    let contract_id = env.register(TrancheContract, ());

    env.as_contract(&contract_id, || {
        TrancheContract::initialize(
            env.clone(),
            admin.clone(),
            token.clone(),
            senior_share_token.clone(),
            junior_share_token.clone(),
            tranche::state::TrancheConfig {
                senior_target_yield_bps: 1000,
                senior_advance_rate_bps: 8000,
                junior_first_loss_bps: 10000,
            },
        );
    });

    env.as_contract(&contract_id, || {
        TrancheContract::open_tranche_for_token(
            env.clone(),
            admin.clone(),
            token.clone(),
            senior_share_token,
            junior_share_token,
            tranche::state::TrancheConfig {
                senior_target_yield_bps: 1000,
                senior_advance_rate_bps: 8000,
                junior_first_loss_bps: 10000,
            },
        )
        .unwrap();
    });

    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);
    mint(&env, &token, &investor1, 1_000_000);
    mint(&env, &token, &investor2, 1_000_000);

    // Deposit junior first to create capacity
    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor2.clone(),
            token.clone(),
            TrancheClass::Junior,
            2000,
        );
    });

    // Deposit to senior
    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor1.clone(),
            token.clone(),
            TrancheClass::Senior,
            8000,
        );
    });

    // Fund one invoice: 1000 (800 senior, 200 junior)
    env.as_contract(&contract_id, || {
        TrancheContract::fund_invoice_from_tranches(env.clone(), token.clone(), 1, 1000);
    });

    // Default with 200 collateral recovered (800 shortfall)
    env.as_contract(&contract_id, || {
        TrancheContract::allocate_loss(env.clone(), token.clone(), 1, 800);
    });

    env.as_contract(&contract_id, || {
        let pool = TrancheContract::get_pool(env.clone(), token.clone());

        // Junior should have absorbed the entire 800 loss
        assert_eq!(pool.junior.losses, 800);
        // Senior should have no losses
        assert_eq!(pool.senior.losses, 0);

        // Junior should still have 1200 remaining (2000 - 800)
        assert_eq!(pool.junior.deposited - pool.junior.losses, 1200);
    });
}

#[test]
fn test_senior_takes_loss_after_junior_exhausted() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = create_token(&env);
    let senior_share_token = env.register(DummyShare, ());
    let junior_share_token = env.register(DummyShare, ());

    let contract_id = env.register(TrancheContract, ());

    env.as_contract(&contract_id, || {
        TrancheContract::initialize(
            env.clone(),
            admin.clone(),
            token.clone(),
            senior_share_token.clone(),
            junior_share_token.clone(),
            tranche::state::TrancheConfig {
                senior_target_yield_bps: 1000,
                senior_advance_rate_bps: 8000,
                junior_first_loss_bps: 10000,
            },
        );
    });

    env.as_contract(&contract_id, || {
        TrancheContract::open_tranche_for_token(
            env.clone(),
            admin.clone(),
            token.clone(),
            senior_share_token,
            junior_share_token,
            tranche::state::TrancheConfig {
                senior_target_yield_bps: 1000,
                senior_advance_rate_bps: 8000,
                junior_first_loss_bps: 10000,
            },
        )
        .unwrap();
    });

    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);
    mint(&env, &token, &investor1, 1_000_000);
    mint(&env, &token, &investor2, 1_000_000);

    // Deposit junior first to create capacity
    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor2.clone(),
            token.clone(),
            TrancheClass::Junior,
            500, // Small junior buffer
        );
    });

    // Deposit to senior - limited by advance rate
    // With 500 junior, senior can deposit up to: 0.8 * (500 + X) = X => X = 2000
    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor1.clone(),
            token.clone(),
            TrancheClass::Senior,
            2000,
        );
    });

    // Fund invoice: 1000 (800 senior, 200 junior)
    env.as_contract(&contract_id, || {
        TrancheContract::fund_invoice_from_tranches(env.clone(), token.clone(), 1, 1000);
    });

    // Default with zero collateral (1000 shortfall)
    env.as_contract(&contract_id, || {
        TrancheContract::allocate_loss(env.clone(), token.clone(), 1, 1000);
    });

    env.as_contract(&contract_id, || {
        let pool = TrancheContract::get_pool(env.clone(), token.clone());

        // Junior should be wiped out (absorbed 200 from this invoice, but had only 500 total)
        assert!(pool.junior.losses >= 200);
        // Senior should take the remaining loss
        assert!(pool.senior.losses > 0);

        // Total losses should equal shortfall
        assert_eq!(pool.senior.losses + pool.junior.losses, 1000);
    });
}
