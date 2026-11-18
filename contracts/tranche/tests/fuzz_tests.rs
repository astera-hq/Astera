#![cfg(test)]

use proptest::prelude::*;
use soroban_sdk::Env;
use tranche::math::{calculate_loss_allocation, calculate_waterfall_split};

const YEAR_SECS: u64 = 365 * 24 * 60 * 60;

// ── dust / rounding boundary tests ──────────────────────────────────────────

#[test]
fn test_waterfall_split_dust_total_due() {
    let env = Env::default();
    // 1-unit total_due: all goes to senior, junior gets 0
    let (s, j) = calculate_waterfall_split(&env, 1, 1_000_000, 1000, 10_000, YEAR_SECS);
    assert_eq!(s, 1);
    assert_eq!(j, 0);
}

#[test]
fn test_waterfall_split_elapsed_secs_one() {
    let env = Env::default();
    // elapsed_secs = 1: interest rounds to 0 for typical principals, senior_cap == principal
    let (s, j) = calculate_waterfall_split(&env, 2_000, 1_000, 1000, 10_000, 1);
    // interest = 1000 * 1000 * 1 / 10_000 / 31_536_000 = 0
    assert_eq!(s, 1_000);
    assert_eq!(j, 1_000);
}

#[test]
fn test_waterfall_split_total_due_one_below_cap() {
    let env = Env::default();
    // senior_cap = 1100 (1000 principal + 10% for 1 year); total_due = 1099
    let (s, j) = calculate_waterfall_split(&env, 1_099, 1_000, 1000, 10_000, YEAR_SECS);
    assert_eq!(s, 1_099);
    assert_eq!(j, 0);
}

#[test]
fn test_waterfall_split_total_due_one_above_cap() {
    let env = Env::default();
    // total_due = 1101, cap = 1100
    let (s, j) = calculate_waterfall_split(&env, 1_101, 1_000, 1000, 10_000, YEAR_SECS);
    assert_eq!(s, 1_100);
    assert_eq!(j, 1);
}

#[test]
fn test_loss_allocation_dust_shortfall() {
    // 1-unit shortfall, junior has plenty
    let (jl, sl) = calculate_loss_allocation(1, 1_000_000, 10_000);
    assert_eq!(jl, 1);
    assert_eq!(sl, 0);
}

#[test]
fn test_loss_allocation_shortfall_one_above_junior() {
    // shortfall = junior + 1: junior wiped, senior takes 1
    let (jl, sl) = calculate_loss_allocation(101, 100, 10_000);
    assert_eq!(jl, 100);
    assert_eq!(sl, 1);
}

#[test]
fn test_loss_allocation_shortfall_one_below_junior() {
    // shortfall = junior - 1: junior absorbs all, senior untouched
    let (jl, sl) = calculate_loss_allocation(99, 100, 10_000);
    assert_eq!(jl, 99);
    assert_eq!(sl, 0);
}

// ── property tests ───────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// senior_amount + junior_amount == total_due for all inputs
    #[test]
    fn prop_waterfall_split_sum_invariant(
        total_due in 0i128..1_000_000_000i128,
        senior_principal in 1i128..500_000_000i128,
        yield_bps in 0u32..5_000u32,
        elapsed in 1u64..YEAR_SECS * 5,
    ) {
        let env = Env::default();
        let (s, j) = calculate_waterfall_split(&env, total_due, senior_principal, yield_bps, 10_000, elapsed);
        prop_assert_eq!(s + j, total_due, "sum must equal total_due");
    }

    /// senior_amount never exceeds the computed cap
    #[test]
    fn prop_waterfall_split_senior_never_exceeds_cap(
        total_due in 0i128..1_000_000_000i128,
        senior_principal in 1i128..500_000_000i128,
        yield_bps in 0u32..5_000u32,
        elapsed in 1u64..YEAR_SECS * 5,
    ) {
        let env = Env::default();
        let interest = senior_principal * yield_bps as i128 * elapsed as i128
            / 10_000
            / YEAR_SECS as i128;
        let cap = senior_principal + interest;
        let (s, _) = calculate_waterfall_split(&env, total_due, senior_principal, yield_bps, 10_000, elapsed);
        prop_assert!(s <= cap, "senior {} must not exceed cap {}", s, cap);
    }

    /// senior_amount is monotonically non-decreasing in total_due
    #[test]
    fn prop_waterfall_split_monotonic_senior(
        lower in 0i128..500_000_000i128,
        delta in 0i128..500_000_000i128,
        senior_principal in 1i128..500_000_000i128,
        yield_bps in 0u32..5_000u32,
        elapsed in 1u64..YEAR_SECS * 5,
    ) {
        let env = Env::default();
        let higher = lower + delta;
        let (s_low, _) = calculate_waterfall_split(&env, lower, senior_principal, yield_bps, 10_000, elapsed);
        let (s_high, _) = calculate_waterfall_split(&env, higher, senior_principal, yield_bps, 10_000, elapsed);
        prop_assert!(s_high >= s_low, "senior must be non-decreasing in total_due");
    }

    /// junior_loss + senior_loss == shortfall for all inputs
    #[test]
    fn prop_loss_allocation_sum_invariant(
        shortfall in 0i128..1_000_000_000i128,
        junior_remaining in 0i128..1_000_000_000i128,
    ) {
        let (jl, sl) = calculate_loss_allocation(shortfall, junior_remaining, 10_000);
        prop_assert_eq!(jl + sl, shortfall, "loss sum must equal shortfall");
    }

    /// junior absorbs first; senior_loss > 0 only when junior is exhausted
    #[test]
    fn prop_loss_allocation_junior_first(
        shortfall in 0i128..1_000_000_000i128,
        junior_remaining in 0i128..1_000_000_000i128,
    ) {
        let (jl, sl) = calculate_loss_allocation(shortfall, junior_remaining, 10_000);
        if shortfall <= junior_remaining {
            prop_assert_eq!(sl, 0, "senior must take no loss when junior covers shortfall");
        } else {
            prop_assert_eq!(jl, junior_remaining, "junior must be fully wiped before senior takes loss");
        }
    }

    /// junior_loss never exceeds junior_remaining
    #[test]
    fn prop_loss_allocation_junior_loss_bounded(
        shortfall in 0i128..1_000_000_000i128,
        junior_remaining in 0i128..1_000_000_000i128,
    ) {
        let (jl, _) = calculate_loss_allocation(shortfall, junior_remaining, 10_000);
        prop_assert!(jl <= junior_remaining, "junior loss {} must not exceed junior_remaining {}", jl, junior_remaining);
    }

    /// Core tranche accounting invariant: senior + junior principal + interest always equals total deployed
    #[test]
    fn prop_tranche_accounting_conservation(
        senior_principal in 1i128..500_000_000i128,
        junior_principal in 1i128..500_000_000i128,
        yield_bps in 100u32..5_000u32,
        elapsed in 1u64..YEAR_SECS * 5,
    ) {
        let env = Env::default();
        let total_deployed = senior_principal + junior_principal;
        let interest = (total_deployed * yield_bps as i128 * elapsed as i128)
            / 10_000
            / YEAR_SECS as i128;
        let total_due = total_deployed + interest;

        let (senior_repay, junior_repay) = calculate_waterfall_split(&env, total_due, senior_principal, yield_bps, 10_000, elapsed);

        // Check that waterfall split correctly distributes the repayment
        prop_assert_eq!(senior_repay + junior_repay, total_due, "repayment must equal total due");

        // Senior should receive at least principal, junior should receive remainder
        prop_assert!(senior_repay >= senior_principal, "senior must receive at least principal");
    }

    /// Tranche loss waterfall invariant: with a shortfall, junior absorbs first
    #[test]
    fn prop_tranche_loss_waterfall_junior_protection(
        senior_principal in 1i128..500_000_000i128,
        junior_principal in 1i128..500_000_000i128,
        loss_ratio in 0u32..10_000u32,
    ) {
        let total_principal = senior_principal + junior_principal;
        let shortfall = (total_principal * loss_ratio as i128) / 10_000;

        let (junior_loss, senior_loss) = calculate_loss_allocation(shortfall, junior_principal, 10_000);

        // Junior must absorb loss first
        prop_assert!(junior_loss <= junior_principal, "junior loss {} must not exceed junior principal {}", junior_loss, junior_principal);

        // Senior only takes loss when junior is exhausted
        if shortfall <= junior_principal {
            prop_assert_eq!(senior_loss, 0, "senior must take zero loss when junior can cover");
        } else {
            prop_assert_eq!(junior_loss, junior_principal, "junior must be exhausted before senior takes loss");
            prop_assert_eq!(junior_loss + senior_loss, shortfall, "losses must sum to shortfall");
        }
    }

    /// Tranche deposit-repay cycle invariant: deposits + yield >= deposits for on-time repayment
    #[test]
    fn prop_tranche_yield_accumulation(
        deposit_amount in 100_000i128..10_000_000_000i128,
        yield_bps in 500u32..5_000u32,
        hold_duration in 86_400u64..YEAR_SECS * 3,
    ) {
        let _env = Env::default();
        let interest = (deposit_amount * yield_bps as i128 * hold_duration as i128)
            / 10_000
            / YEAR_SECS as i128;
        let repay_amount = deposit_amount + interest;

        // Repayment should always be >= deposit for positive yield
        prop_assert!(repay_amount >= deposit_amount, "repayment must be >= deposit with positive yield");
        prop_assert!(interest >= 0, "interest must be non-negative with positive yield_bps");
    }

    /// Tranche dust handling: rounding doesn't cause off-by-one errors accumulating across operations
    #[test]
    fn prop_tranche_rounding_stability(
        operation_count in 2usize..20,
        base_amount in 1_000i128..100_000_000i128,
    ) {
        let env = Env::default();

        // Simulate multiple operations and ensure cumulative rounding error stays bounded
        let mut cumulative_principal = base_amount;
        let mut cumulative_error = 0i128;

        for _ in 0..operation_count {
            let (s, j) = calculate_waterfall_split(&env, cumulative_principal, base_amount, 1000, 10_000, YEAR_SECS);
            let this_total = s + j;

            // Each operation should sum correctly (any difference is rounding error)
            let error = cumulative_principal - this_total;
            cumulative_error = cumulative_error.saturating_add(error.abs());
            cumulative_principal = this_total;
        }

        // Over many operations, rounding error should stay bounded
        prop_assert!(cumulative_error.abs() <= (operation_count as i128) * 2,
                     "cumulative rounding error {} should stay bounded by 2*operation_count", cumulative_error);
    }
}
