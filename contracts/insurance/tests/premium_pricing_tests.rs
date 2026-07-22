#![cfg(test)]

use insurance::{calculate_premium, PremiumConfig, RiskTier};
use proptest::prelude::*;
use soroban_sdk::{Env, Vec};

fn config(env: &Env) -> PremiumConfig {
    let mut tiers = Vec::new(env);
    tiers.push_back(RiskTier {
        min_score: 750,
        max_score: 850,
        risk_multiplier_bps: 8_000,
    });
    tiers.push_back(RiskTier {
        min_score: 650,
        max_score: 749,
        risk_multiplier_bps: 12_000,
    });
    tiers.push_back(RiskTier {
        min_score: 550,
        max_score: 649,
        risk_multiplier_bps: 18_000,
    });
    tiers.push_back(RiskTier {
        min_score: 200,
        max_score: 549,
        risk_multiplier_bps: 30_000,
    });
    PremiumConfig {
        base_rate_bps: 200,
        tenor_bps_per_day: 10,
        risk_tiers: tiers,
        default_risk_multiplier_bps: 40_000,
        min_premium_bps: 10,
        max_premium_bps: 5_000,
        default_coverage_bps: 8_000,
    }
}

#[test]
fn test_zero_or_negative_principal_yields_zero_premium() {
    let env = Env::default();
    let cfg = config(&env);
    assert_eq!(calculate_premium(0, 700, 30, &cfg), 0);
    assert_eq!(calculate_premium(-100, 700, 30, &cfg), 0);
}

#[test]
fn test_worse_score_never_cheaper_fixed_case() {
    let env = Env::default();
    let cfg = config(&env);
    let principal = 1_000_000i128;
    let tenor = 30u32;

    let best = calculate_premium(principal, 800, tenor, &cfg);
    let mid = calculate_premium(principal, 700, tenor, &cfg);
    let worse = calculate_premium(principal, 600, tenor, &cfg);
    let worst = calculate_premium(principal, 250, tenor, &cfg);
    let unavailable = calculate_premium(principal, 0, tenor, &cfg); // outside all tiers

    assert!(best <= mid, "800 score should not cost more than 700");
    assert!(mid <= worse, "700 score should not cost more than 600");
    assert!(worse <= worst, "600 score should not cost more than 250");
    assert!(
        worst <= unavailable,
        "worst configured tier should not cost more than the no-data fallback"
    );
}

#[test]
fn test_longer_tenor_never_cheaper_fixed_case() {
    let env = Env::default();
    let cfg = config(&env);
    let principal = 1_000_000i128;
    let score = 700u32;

    let short = calculate_premium(principal, score, 10, &cfg);
    let medium = calculate_premium(principal, score, 60, &cfg);
    let long = calculate_premium(principal, score, 180, &cfg);

    assert!(
        short <= medium,
        "10-day tenor should not cost more than 60-day"
    );
    assert!(
        medium <= long,
        "60-day tenor should not cost more than 180-day"
    );
}

#[test]
fn test_premium_respects_configured_bounds() {
    let env = Env::default();
    let cfg = config(&env);
    let principal = 1_000_000i128;

    let min_bound = (principal as u128) * (cfg.min_premium_bps as u128) / 10_000;
    let max_bound = (principal as u128) * (cfg.max_premium_bps as u128) / 10_000;

    for score in [0u32, 300, 600, 700, 800, 850, 1000] {
        for tenor in [0u32, 10, 90, 365, 3650] {
            let premium = calculate_premium(principal, score, tenor, &cfg);
            assert!(
                premium >= min_bound && premium <= max_bound,
                "premium {premium} out of bounds [{min_bound}, {max_bound}] for score={score} tenor={tenor}"
            );
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Property: holding tenor fixed, a strictly worse credit score never
    /// produces a strictly cheaper premium.
    #[test]
    fn prop_premium_monotonic_in_score(
        principal in 1_000i128..1_000_000_000i128,
        tenor in 0u32..730u32,
        better_score in 200u32..850u32,
        delta in 1u32..650u32,
    ) {
        let env = Env::default();
        let cfg = config(&env);
        let worse_score = better_score.saturating_sub(delta).max(0);
        prop_assume!(worse_score < better_score);

        let premium_better = calculate_premium(principal, better_score, tenor, &cfg);
        let premium_worse = calculate_premium(principal, worse_score, tenor, &cfg);

        prop_assert!(
            premium_worse >= premium_better,
            "worse score {} priced {} < better score {} priced {}",
            worse_score, premium_worse, better_score, premium_better
        );
    }

    /// Property: holding score fixed, a longer tenor never produces a
    /// strictly cheaper premium.
    #[test]
    fn prop_premium_monotonic_in_tenor(
        principal in 1_000i128..1_000_000_000i128,
        score in 200u32..850u32,
        shorter_tenor in 0u32..3650u32,
        delta in 1u32..3650u32,
    ) {
        let env = Env::default();
        let cfg = config(&env);
        let longer_tenor = shorter_tenor + delta;

        let premium_short = calculate_premium(principal, score, shorter_tenor, &cfg);
        let premium_long = calculate_premium(principal, score, longer_tenor, &cfg);

        prop_assert!(
            premium_long >= premium_short,
            "shorter tenor {} priced {} > longer tenor {} priced {}",
            shorter_tenor, premium_short, longer_tenor, premium_long
        );
    }

    /// Property: premium is always within the configured [min, max] bps bounds
    /// of principal, regardless of score/tenor extremes.
    #[test]
    fn prop_premium_always_within_bounds(
        principal in 1i128..1_000_000_000_000i128,
        score in 0u32..2000u32,
        tenor in 0u32..36_500u32,
    ) {
        let env = Env::default();
        let cfg = config(&env);
        let premium = calculate_premium(principal, score, tenor, &cfg);

        let min_bound = (principal as u128) * (cfg.min_premium_bps as u128) / 10_000;
        let max_bound = (principal as u128) * (cfg.max_premium_bps as u128) / 10_000;

        prop_assert!(premium >= min_bound);
        prop_assert!(premium <= max_bound);
    }

    /// Property: premium scales with principal — doubling principal never
    /// produces a smaller premium (holding score/tenor fixed), since it's
    /// applied as a fixed bps rate before clamping.
    #[test]
    fn prop_premium_monotonic_in_principal_below_cap(
        principal in 1_000i128..1_000_000i128,
        score in 200u32..850u32,
        tenor in 0u32..365u32,
    ) {
        let env = Env::default();
        let cfg = config(&env);
        let premium_small = calculate_premium(principal, score, tenor, &cfg);
        let premium_large = calculate_premium(principal * 2, score, tenor, &cfg);

        prop_assert!(premium_large >= premium_small);
    }
}
