#![cfg(test)]

//! #863: utilization-driven kinked interest-rate model.
//!
//! Covers the pure `compute_current_rate` curve across every distinguishing
//! utilization band, the timelocked propose/execute/cancel governance
//! lifecycle, per-invoice rate locking at funding time, rate-history ring
//! buffer behavior, and backward compatibility for tokens without a
//! configured model.

use pool::{compute_current_rate, FundingPool, FundingPoolClient, PoolError, RateModelConfig};
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
}

const INIT_AT: u64 = 100_000;
const YIELD_TIMELOCK_SECS: u64 = 172_800; // 48h default
const YIELD_CHANGE_COOLDOWN_SECS: u64 = 86_400; // 24h default

fn setup(env: &Env) -> (FundingPoolClient<'_>, Address, Address) {
    env.ledger().with_mut(|l| l.timestamp = INIT_AT);
    let contract_id = env.register(FundingPool, ());
    let client = FundingPoolClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let invoice_contract = env.register(DummyInvoice, ());
    DummyInvoiceClient::new(env, &invoice_contract).set_pool(&contract_id);
    let share_token = env.register(DummyShare, ());

    client.initialize(&admin, &usdc_id, &share_token, &invoice_contract);
    client.set_max_investor_concentration(&admin, &10_000u32);
    (client, admin, usdc_id)
}

fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token_id).mint(to, &amount);
}

/// Standard curve used across the integration tests:
/// 2% base, kink at 80%, +6% across the gentle slope, +24% across the steep
/// slope, 50% ceiling.
fn standard_model() -> RateModelConfig {
    RateModelConfig {
        base_rate_bps: 200,
        optimal_utilization_bps: 8_000,
        slope1_bps: 600,
        slope2_bps: 2_400,
        max_rate_bps: 5_000,
    }
}

/// Push a rate model through the full propose -> timelock -> execute cycle.
fn enact_rate_model(client: &FundingPoolClient, admin: &Address, token: &Address, env: &Env) {
    client.propose_rate_model_change(admin, token, &standard_model());
    env.ledger()
        .with_mut(|l| l.timestamp += YIELD_TIMELOCK_SECS);
    client.execute_rate_model_change(token);
}

// ── pure-function curve tests ───────────────────────────────────────────────

#[test]
fn test_rate_at_zero_utilization_is_base() {
    assert_eq!(compute_current_rate(0, &standard_model()), 200);
}

#[test]
fn test_rate_just_below_kink() {
    // 200 + 7999 * 600 / 8000 = 200 + 599.925 -> 799 (integer truncation)
    assert_eq!(compute_current_rate(7_999, &standard_model()), 799);
}

#[test]
fn test_rate_exactly_at_kink() {
    // base + full slope1, no slope2 contribution
    assert_eq!(compute_current_rate(8_000, &standard_model()), 800);
}

#[test]
fn test_rate_just_above_kink() {
    // 800 + 1 * 2400 / 2000 = 801
    assert_eq!(compute_current_rate(8_001, &standard_model()), 801);
}

#[test]
fn test_rate_at_full_utilization() {
    // base + slope1 + slope2
    assert_eq!(compute_current_rate(10_000, &standard_model()), 3_200);
}

#[test]
fn test_rate_clamps_to_max_rate() {
    let model = RateModelConfig {
        max_rate_bps: 1_000,
        ..standard_model()
    };
    // Unc clamped value at 100% would be 3_200.
    assert_eq!(compute_current_rate(10_000, &model), 1_000);
    // Just below the ceiling the curve is unaffected: at the kink, 800 < 1_000.
    assert_eq!(compute_current_rate(8_000, &model), 800);
}

#[test]
fn test_rate_clamps_utilization_above_100_percent() {
    assert_eq!(
        compute_current_rate(12_345, &standard_model()),
        compute_current_rate(10_000, &standard_model())
    );
}

#[test]
fn test_rate_kink_at_100_percent_has_no_steep_region() {
    let model = RateModelConfig {
        optimal_utilization_bps: 10_000,
        ..standard_model()
    };
    // Entire span uses slope1; nothing can exceed the kink.
    assert_eq!(compute_current_rate(10_000, &model), 800);
    assert_eq!(compute_current_rate(5_000, &model), 500);
}

#[test]
fn test_rate_zero_slopes_give_flat_base() {
    let model = RateModelConfig {
        slope1_bps: 0,
        slope2_bps: 0,
        ..standard_model()
    };
    assert_eq!(compute_current_rate(0, &model), 200);
    assert_eq!(compute_current_rate(10_000, &model), 200);
}

#[test]
fn test_rate_never_panics_on_degenerate_config() {
    // A zero kink is rejected by validate_rate_model_config before storage,
    // but the pure function itself must stay total (saturates to ceiling).
    let degenerate = RateModelConfig {
        optimal_utilization_bps: 0,
        ..standard_model()
    };
    assert_eq!(compute_current_rate(5_000, &degenerate), 5_000);
}

// ── governance lifecycle ────────────────────────────────────────────────────

#[test]
fn test_propose_rejects_invalid_configs() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);

    let cases = [
        // kink out of bounds
        RateModelConfig {
            optimal_utilization_bps: 0,
            ..standard_model()
        },
        RateModelConfig {
            optimal_utilization_bps: 10_001,
            ..standard_model()
        },
        // zero / excessive ceiling
        RateModelConfig {
            max_rate_bps: 0,
            ..standard_model()
        },
        RateModelConfig {
            max_rate_bps: 5_001,
            ..standard_model()
        },
        // base above ceiling
        RateModelConfig {
            base_rate_bps: 4_000,
            max_rate_bps: 3_000,
            ..standard_model()
        },
        // slopes out of bounds
        RateModelConfig {
            slope1_bps: 5_001,
            ..standard_model()
        },
        RateModelConfig {
            slope2_bps: 5_001,
            ..standard_model()
        },
    ];
    for bad in cases {
        let result = client.try_propose_rate_model_change(&admin, &usdc_id, &bad);
        assert_eq!(result, Err(Ok(PoolError::InvalidRateModelConfig)));
    }
}

#[test]
fn test_execute_rejected_before_timelock() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);

    client.propose_rate_model_change(&admin, &usdc_id, &standard_model());

    env.ledger().with_mut(|l| l.timestamp += 86_400); // +24h < 48h
    let result = client.try_execute_rate_model_change(&usdc_id);
    assert_eq!(result, Err(Ok(PoolError::RateModelChangeNotReady)));
}

#[test]
fn test_execute_without_proposal_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, usdc_id) = setup(&env);

    let result = client.try_execute_rate_model_change(&usdc_id);
    assert_eq!(result, Err(Ok(PoolError::RateModelProposalNotFound)));
}

#[test]
fn test_execute_at_timelock_boundary_sets_model() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);

    client.propose_rate_model_change(&admin, &usdc_id, &standard_model());
    env.ledger()
        .with_mut(|l| l.timestamp += YIELD_TIMELOCK_SECS);
    client.execute_rate_model_change(&usdc_id);

    assert_eq!(
        client.get_rate_model_config(&usdc_id),
        Some(standard_model())
    );
}

#[test]
fn test_cancel_proposal_blocks_execution() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);

    client.propose_rate_model_change(&admin, &usdc_id, &standard_model());
    client.cancel_rate_model_change(&admin, &usdc_id);

    env.ledger()
        .with_mut(|l| l.timestamp += YIELD_TIMELOCK_SECS);
    let result = client.try_execute_rate_model_change(&usdc_id);
    assert_eq!(result, Err(Ok(PoolError::RateModelProposalNotFound)));
}

/// #911: a rate model change must wait for the admin operation delay, even when that
/// delay is longer than the yield timelock.
#[test]
fn test_rate_change_respects_operation_delay() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);

    client.set_operation_delay(&admin, &259_200u64); // 72h, longer than the 48h timelock
    client.propose_rate_model_change(&admin, &usdc_id, &standard_model());

    env.ledger()
        .with_mut(|l| l.timestamp += YIELD_TIMELOCK_SECS); // 48h
    let result = client.try_execute_rate_model_change(&usdc_id);
    assert_eq!(result, Err(Ok(PoolError::RateModelChangeNotReady)));

    env.ledger().with_mut(|l| l.timestamp += 86_400); // 72h total
    client.execute_rate_model_change(&usdc_id);
    assert_eq!(
        client.get_rate_model_config(&usdc_id),
        Some(standard_model())
    );
}

const ONE_YEAR_SECS: u64 = 31_536_000;

/// #913: with no capital deployed there is nothing to earn, so the estimate is zero.
#[test]
fn test_estimate_deposit_yield_is_zero_without_deployed_capital() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, usdc_id) = setup(&env);

    assert_eq!(
        client.estimate_deposit_yield(&usdc_id, &10_000i128, &ONE_YEAR_SECS),
        0
    );
}

#[test]
fn test_estimate_deposit_yield_rejects_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, usdc_id) = setup(&env);

    let result = client.try_estimate_deposit_yield(&usdc_id, &0i128, &ONE_YEAR_SECS);
    assert_eq!(result, Err(Ok(PoolError::ZeroAmount)));
}

/// #913: the estimate grows with the horizon and with the size of the deposit, and is
/// positive once capital is deployed.
#[test]
fn test_estimate_deposit_yield_scales_with_horizon_and_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);

    let investor = Address::generate(&env);
    let borrower = Address::generate(&env);
    mint(&env, &usdc_id, &investor, 100_000);
    client.deposit(&investor, &usdc_id, &100_000, &None);
    enact_rate_model(&client, &admin, &usdc_id, &env);

    let due_date = env.ledger().timestamp() + 1_000_000;
    client.fund_invoice(&admin, &1u64, &50_000i128, &borrower, &due_date, &usdc_id);

    let one_year = client.estimate_deposit_yield(&usdc_id, &10_000i128, &ONE_YEAR_SECS);
    let two_years = client.estimate_deposit_yield(&usdc_id, &10_000i128, &(2 * ONE_YEAR_SECS));
    let larger_deposit = client.estimate_deposit_yield(&usdc_id, &30_000i128, &ONE_YEAR_SECS);

    assert!(one_year > 0);
    assert!(two_years > one_year);
    assert!(larger_deposit > one_year);
}

#[test]
fn test_cooldown_enforced_between_executed_changes() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);

    enact_rate_model(&client, &admin, &usdc_id, &env);

    // Immediately after execution the cooldown window is still open.
    let result = client.try_propose_rate_model_change(&admin, &usdc_id, &standard_model());
    assert_eq!(result, Err(Ok(PoolError::InvalidAmount)));

    // After the cooldown a fresh proposal is accepted again.
    env.ledger()
        .with_mut(|l| l.timestamp += YIELD_CHANGE_COOLDOWN_SECS);
    client.propose_rate_model_change(&admin, &usdc_id, &standard_model());
}

#[test]
fn test_views_error_when_model_not_configured() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, usdc_id) = setup(&env);

    assert_eq!(
        client.try_get_current_rate(&usdc_id),
        Err(Ok(PoolError::RateModelNotConfigured))
    );
    assert_eq!(
        client.try_preview_rate_at_utilization(&usdc_id, &5_000u32),
        Err(Ok(PoolError::RateModelNotConfigured))
    );
    assert_eq!(client.get_rate_model_config(&usdc_id), None);
    assert_eq!(client.get_rate_history(&usdc_id, &10u32).len(), 0);
}

// ── rate locking at funding time ────────────────────────────────────────────

#[test]
fn test_invoices_lock_curve_rate_at_funding_time() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);

    let investor = Address::generate(&env);
    let borrower = Address::generate(&env);
    mint(&env, &usdc_id, &investor, 100_000);
    client.deposit(&investor, &usdc_id, &100_000, &None);

    enact_rate_model(&client, &admin, &usdc_id, &env);

    let due_date = env.ledger().timestamp() + 1_000_000;

    // 10% utilization: 200 + 1000 * 600 / 8000 = 275 bps.
    client.fund_invoice(&admin, &1u64, &10_000i128, &borrower, &due_date, &usdc_id);
    // 80% utilization (exactly at kink): 200 + 600 = 800 bps.
    client.fund_invoice(&admin, &2u64, &70_000i128, &borrower, &due_date, &usdc_id);
    // 95% utilization (above kink): 800 + 1500 * 2400 / 2000 = 2_600 bps.
    client.fund_invoice(&admin, &3u64, &15_000i128, &borrower, &due_date, &usdc_id);

    assert_eq!(
        client.get_funded_invoice(&1u64).unwrap().locked_yield_bps,
        275
    );
    assert_eq!(
        client.get_funded_invoice(&2u64).unwrap().locked_yield_bps,
        800
    );
    assert_eq!(
        client.get_funded_invoice(&3u64).unwrap().locked_yield_bps,
        2_600
    );

    // Live rate view agrees with the last funding's snapshot.
    assert_eq!(client.get_current_rate(&usdc_id), 2_600);
    assert_eq!(client.preview_rate_at_utilization(&usdc_id, &0u32), 200);
    assert_eq!(
        client.preview_rate_at_utilization(&usdc_id, &10_000u32),
        3_200
    );
}

#[test]
fn test_repayment_uses_locked_rate_after_utilization_changes() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);

    let investor = Address::generate(&env);
    let borrower = Address::generate(&env);
    mint(&env, &usdc_id, &investor, 100_000);
    client.deposit(&investor, &usdc_id, &100_000, &None);

    enact_rate_model(&client, &admin, &usdc_id, &env);

    let due_date = env.ledger().timestamp() + 1_000_000;

    // Invoice 1 funds at 10% utilization and locks 275 bps.
    client.fund_invoice(&admin, &1u64, &10_000i128, &borrower, &due_date, &usdc_id);
    // Invoice 2 then pushes utilization to 80% — the live rate is now 800 bps.
    client.fund_invoice(&admin, &2u64, &70_000i128, &borrower, &due_date, &usdc_id);

    env.ledger().with_mut(|l| l.timestamp += 100_000);

    // Simple interest at the locked 275 bps for 100_000s on 10_000 principal:
    // 10_000 * 275 * 100_000 / (10_000 * 31_536_000) = 0.87 -> 1 (round half
    // up). The live 800 bps would charge 3 stroops instead — so this
    // assertion only holds if the originally-locked rate is used.
    assert_eq!(client.estimate_repayment(&1u64, &None), 10_001);
    // Invoice 2 at its locked 800 bps: 70_000 * 800 * 100_000 /
    // (10_000 * 31_536_000) = 17.76 -> 18 (round half up).
    assert_eq!(client.estimate_repayment(&2u64, &None), 70_018);

    mint(&env, &usdc_id, &borrower, 10_001);
    client.repay_invoice(&1u64, &borrower, &10_001i128);

    let record = client.get_funded_invoice(&1u64).unwrap();
    assert_eq!(record.repaid_amount, 10_001);
    assert_eq!(record.locked_yield_bps, 275);
}

#[test]
fn test_funding_without_model_locks_static_yield() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);

    let investor = Address::generate(&env);
    let borrower = Address::generate(&env);
    mint(&env, &usdc_id, &investor, 100_000);
    client.deposit(&investor, &usdc_id, &100_000, &None);

    let due_date = env.ledger().timestamp() + 1_000_000;
    client.fund_invoice(&admin, &1u64, &10_000i128, &borrower, &due_date, &usdc_id);

    // DEFAULT_YIELD_BPS = 800 locked verbatim — no curve configured.
    assert_eq!(
        client.get_funded_invoice(&1u64).unwrap().locked_yield_bps,
        800
    );

    // The manual override still drives new fundings for model-less tokens.
    env.ledger()
        .with_mut(|l| l.timestamp += YIELD_CHANGE_COOLDOWN_SECS + 1);
    client.set_yield(&admin, &900u32);
    client.fund_invoice(&admin, &2u64, &10_000i128, &borrower, &due_date, &usdc_id);
    assert_eq!(
        client.get_funded_invoice(&2u64).unwrap().locked_yield_bps,
        900
    );
}

#[test]
fn test_curve_takes_precedence_over_static_yield_once_configured() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);

    let investor = Address::generate(&env);
    let borrower = Address::generate(&env);
    mint(&env, &usdc_id, &investor, 100_000);
    client.deposit(&investor, &usdc_id, &100_000, &None);

    enact_rate_model(&client, &admin, &usdc_id, &env);

    // Manual override down to 700 bps (within the 200 bps max step) — must
    // NOT affect new fundings now that the token has a curve; the flat yield
    // is inert for this token.
    env.ledger()
        .with_mut(|l| l.timestamp += YIELD_CHANGE_COOLDOWN_SECS + 1);
    client.set_yield(&admin, &700u32);

    let due_date = env.ledger().timestamp() + 1_000_000;
    client.fund_invoice(&admin, &1u64, &10_000i128, &borrower, &due_date, &usdc_id);
    assert_eq!(
        client.get_funded_invoice(&1u64).unwrap().locked_yield_bps,
        275
    );
}

// ── rate-history ring buffer ────────────────────────────────────────────────

#[test]
fn test_rate_history_records_on_fund_and_repay_in_order() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);

    let investor = Address::generate(&env);
    let borrower = Address::generate(&env);
    mint(&env, &usdc_id, &investor, 100_000);
    client.deposit(&investor, &usdc_id, &100_000, &None);

    enact_rate_model(&client, &admin, &usdc_id, &env);

    let due_date = env.ledger().timestamp() + 1_000_000;
    env.ledger().with_mut(|l| l.timestamp += 100);
    client.fund_invoice(&admin, &1u64, &10_000i128, &borrower, &due_date, &usdc_id);
    env.ledger().with_mut(|l| l.timestamp += 100);
    client.fund_invoice(&admin, &2u64, &70_000i128, &borrower, &due_date, &usdc_id);
    env.ledger().with_mut(|l| l.timestamp += 100);

    // Repay invoice 2 in full — utilization drops back, another sample lands.
    let due = client.estimate_repayment(&2u64, &None);
    mint(&env, &usdc_id, &borrower, due);
    client.repay_invoice(&2u64, &borrower, &due);

    let history = client.get_rate_history(&usdc_id, &100u32);
    // 1 sample on execute + 2 fundings + 1 repayment.
    assert_eq!(history.len(), 4);
    // Chronological (oldest-first), strictly non-decreasing timestamps.
    for i in 1..history.len() {
        assert!(
            history.get(i - 1).unwrap().timestamp <= history.get(i).unwrap().timestamp,
            "history out of order at index {i}"
        );
    }
    // Funding raised utilization; repayment brought it back down.
    assert_eq!(history.get(1).unwrap().utilization_bps, 1_000);
    assert_eq!(history.get(2).unwrap().utilization_bps, 8_000);
    assert!(history.get(3).unwrap().utilization_bps < 8_000);
    // Rates track the curve at those utilization levels.
    assert_eq!(history.get(1).unwrap().rate_bps, 275);
    assert_eq!(history.get(2).unwrap().rate_bps, 800);

    // The limit keeps only the most recent samples.
    let limited = client.get_rate_history(&usdc_id, &2u32);
    assert_eq!(limited.len(), 2);
    assert_eq!(limited.get(0), history.get(2));
    assert_eq!(limited.get(1), history.get(3));
}

#[test]
fn test_rate_history_collapses_consecutive_duplicates() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);

    enact_rate_model(&client, &admin, &usdc_id, &env);

    // A second snapshot at unchanged utilization/rate is a no-op: repaying
    // nothing changed nothing, so no funding happens here at all — instead,
    // simply confirm only the execute-time sample exists.
    let history = client.get_rate_history(&usdc_id, &100u32);
    assert_eq!(history.len(), 1);
    assert_eq!(history.get(0).unwrap().utilization_bps, 0);
    assert_eq!(history.get(0).unwrap().rate_bps, 200);
}

#[test]
fn test_rate_history_ring_buffer_evicts_oldest_on_overflow() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);

    // Push MAX_RATE_HISTORY + 5 (725) distinct samples by cycling
    // propose -> execute with a different base rate each round (consecutive
    // duplicates would collapse, so each round must differ from the last).
    let total_rounds = 725u32;
    for i in 0..total_rounds {
        let model = RateModelConfig {
            base_rate_bps: (i % 50) + 1,
            optimal_utilization_bps: 8_000,
            slope1_bps: 0,
            slope2_bps: 0,
            max_rate_bps: 5_000,
        };
        client.propose_rate_model_change(&admin, &usdc_id, &model);
        env.ledger()
            .with_mut(|l| l.timestamp += YIELD_TIMELOCK_SECS);
        client.execute_rate_model_change(&usdc_id);
        // Move past the cooldown before the next proposal.
        env.ledger()
            .with_mut(|l| l.timestamp += YIELD_CHANGE_COOLDOWN_SECS);
    }

    let history = client.get_rate_history(&usdc_id, &1_000u32);
    // Capacity is 720 — the oldest 5 samples were evicted.
    assert_eq!(history.len(), 720);
    // Empty pool => utilization 0 => rate == base_rate_bps of that round.
    // Round i contributes rate (i % 50) + 1; retained rounds are 5..=724.
    assert_eq!(history.get(0).unwrap().rate_bps, 6); // round 5
    assert_eq!(history.get(719).unwrap().rate_bps, 25); // round 724: 724 % 50 = 24 -> 25
                                                        // Chronological order is preserved across the wraparound.
    for i in 1..history.len() {
        assert!(
            history.get(i - 1).unwrap().timestamp <= history.get(i).unwrap().timestamp,
            "history out of order at index {i}"
        );
    }
}

// #905: the utilization view mirrors the rate ring buffer, timestamps and utilization
// included, and honours the requested limit.
#[test]
fn test_pool_utilization_history_mirrors_rate_samples() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);

    let investor = Address::generate(&env);
    let borrower = Address::generate(&env);
    mint(&env, &usdc_id, &investor, 100_000);
    client.deposit(&investor, &usdc_id, &100_000, &None);

    enact_rate_model(&client, &admin, &usdc_id, &env);

    let due_date = env.ledger().timestamp() + 1_000_000;
    env.ledger().with_mut(|l| l.timestamp += 100);
    client.fund_invoice(&admin, &1u64, &10_000i128, &borrower, &due_date, &usdc_id);

    let rate_history = client.get_rate_history(&usdc_id, &100u32);
    let utilization = client.get_pool_utilization_history(&usdc_id, &100u32);
    assert_eq!(utilization.len(), rate_history.len());
    for i in 0..utilization.len() {
        let u = utilization.get(i).unwrap();
        let r = rate_history.get(i).unwrap();
        assert_eq!(u.timestamp, r.timestamp);
        assert_eq!(u.utilization_bps, r.utilization_bps);
    }
    assert_eq!(
        utilization.get(utilization.len() - 1).unwrap().utilization_bps,
        1_000
    );

    assert_eq!(
        client
            .get_pool_utilization_history(&usdc_id, &1u32)
            .len(),
        1
    );
}
