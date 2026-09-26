use soroban_sdk::Env;
use tranche::math::{calculate_loss_allocation, calculate_waterfall_split};

#[test]
fn test_waterfall_split_senior_cap() {
    let env = Env::default();

    // Test case: senior gets capped return, junior gets remainder
    let senior_principal = 1000;
    let senior_target_yield_bps = 1000; // 10%
    let elapsed_secs = 365 * 24 * 60 * 60; // 1 year

    let total_due = 1200; // More than senior cap (1100)
    let (senior_amount, junior_amount) = calculate_waterfall_split(
        &env,
        total_due,
        senior_principal,
        senior_target_yield_bps,
        10_000,
        elapsed_secs,
    );

    assert_eq!(senior_amount, 1100); // Principal + 10% yield
    assert_eq!(junior_amount, 100); // Remainder
}

#[test]
fn test_waterfall_split_partial_payment() {
    let env = Env::default();

    let senior_principal = 1000;
    let senior_target_yield_bps = 1000; // 10%
    let elapsed_secs = 365 * 24 * 60 * 60;

    let total_due = 500; // Less than senior cap
    let (senior_amount, junior_amount) = calculate_waterfall_split(
        &env,
        total_due,
        senior_principal,
        senior_target_yield_bps,
        10_000,
        elapsed_secs,
    );

    assert_eq!(senior_amount, 500); // All goes to senior
    assert_eq!(junior_amount, 0); // Junior gets nothing
}

#[test]
fn test_waterfall_split_exact_cap() {
    let env = Env::default();

    let senior_principal = 1000;
    let senior_target_yield_bps = 1000; // 10%
    let elapsed_secs = 365 * 24 * 60 * 60;

    let total_due = 1100; // Exactly senior cap
    let (senior_amount, junior_amount) = calculate_waterfall_split(
        &env,
        total_due,
        senior_principal,
        senior_target_yield_bps,
        10_000,
        elapsed_secs,
    );

    assert_eq!(senior_amount, 1100);
    assert_eq!(junior_amount, 0);
}

#[test]
fn test_waterfall_split_zero_yield() {
    let env = Env::default();

    let senior_principal = 1000;
    let senior_target_yield_bps = 0; // 0% yield
    let elapsed_secs = 365 * 24 * 60 * 60;

    let total_due = 1500;
    let (senior_amount, junior_amount) = calculate_waterfall_split(
        &env,
        total_due,
        senior_principal,
        senior_target_yield_bps,
        10_000,
        elapsed_secs,
    );

    assert_eq!(senior_amount, 1000); // Only principal
    assert_eq!(junior_amount, 500); // All excess to junior
}

#[test]
fn test_waterfall_split_time_proportional() {
    let env = Env::default();

    let senior_principal = 1000;
    let senior_target_yield_bps = 1000; // 10% annual
    let elapsed_secs = 182 * 24 * 60 * 60; // 6 months

    let total_due = 1500;
    let (senior_amount, junior_amount) = calculate_waterfall_split(
        &env,
        total_due,
        senior_principal,
        senior_target_yield_bps,
        10_000,
        elapsed_secs,
    );

    // Senior cap should be 1000 + 50 (half of 100) = 1050 (allow for rounding)
    assert!((1049..=1051).contains(&senior_amount));
    assert!((449..=451).contains(&junior_amount));
}

#[test]
fn test_waterfall_split_monotonic_senior() {
    let env = Env::default();

    let senior_principal = 1000;
    let senior_target_yield_bps = 1000;
    let elapsed_secs = 365 * 24 * 60 * 60;

    let mut prev_senior = 0;
    for total_due in 0..=2000 {
        let (senior_amount, _) = calculate_waterfall_split(
            &env,
            total_due,
            senior_principal,
            senior_target_yield_bps,
            10_000,
            elapsed_secs,
        );
        assert!(
            senior_amount >= prev_senior,
            "Senior amount should be monotonically non-decreasing"
        );
        prev_senior = senior_amount;
    }
}

#[test]
fn test_waterfall_split_sum_equals_total() {
    let env = Env::default();

    let senior_principal = 1000;
    let senior_target_yield_bps = 1000;
    let elapsed_secs = 365 * 24 * 60 * 60;

    for total_due in [0, 500, 1000, 1100, 1500, 2000] {
        let (senior_amount, junior_amount) = calculate_waterfall_split(
            &env,
            total_due,
            senior_principal,
            senior_target_yield_bps,
            10_000,
            elapsed_secs,
        );
        assert_eq!(
            senior_amount + junior_amount,
            total_due,
            "Sum of senior and junior should equal total due"
        );
    }
}

#[test]
fn test_loss_allocation_junior_absorbs_all() {
    let shortfall = 100;
    let junior_remaining = 200;

    let (junior_loss, senior_loss) = calculate_loss_allocation(shortfall, junior_remaining, 10_000);

    assert_eq!(junior_loss, 100);
    assert_eq!(senior_loss, 0);
}

#[test]
fn test_loss_allocation_junior_exhausted() {
    let shortfall = 300;
    let junior_remaining = 100;

    let (junior_loss, senior_loss) = calculate_loss_allocation(shortfall, junior_remaining, 10_000);

    assert_eq!(junior_loss, 100); // Junior wiped out
    assert_eq!(senior_loss, 200); // Senior takes remainder
}

#[test]
fn test_loss_allocation_exact_shortfall() {
    let shortfall = 100;
    let junior_remaining = 100;

    let (junior_loss, senior_loss) = calculate_loss_allocation(shortfall, junior_remaining, 10_000);

    assert_eq!(junior_loss, 100);
    assert_eq!(senior_loss, 0);
}

#[test]
fn test_loss_allocation_zero_junior_balance() {
    let shortfall = 100;
    let junior_remaining = 0;

    let (junior_loss, senior_loss) = calculate_loss_allocation(shortfall, junior_remaining, 10_000);

    assert_eq!(junior_loss, 0);
    assert_eq!(senior_loss, 100);
}

#[test]
fn test_loss_allocation_zero_shortfall() {
    let shortfall = 0;
    let junior_remaining = 100;

    let (junior_loss, senior_loss) = calculate_loss_allocation(shortfall, junior_remaining, 10_000);

    assert_eq!(junior_loss, 0);
    assert_eq!(senior_loss, 0);
}

#[test]
fn test_loss_allocation_sum_equals_shortfall() {
    for shortfall in [0, 50, 100, 200, 500] {
        for junior_remaining in [0, 50, 100, 200, 500] {
            let (junior_loss, senior_loss) = calculate_loss_allocation(shortfall, junior_remaining, 10_000);
            assert_eq!(
                junior_loss + senior_loss,
                shortfall,
                "Loss allocation sum should equal shortfall"
            );
        }
    }
}

#[test]
fn test_loss_allocation_junior_first_loss_bps_capped() {
    let shortfall = 150;
    let junior_remaining = 200;
    // Junior first loss bps set to 50% (5000 bps) -> max junior loss = 100
    let (junior_loss, senior_loss) = calculate_loss_allocation(shortfall, junior_remaining, 5000);

    assert_eq!(junior_loss, 100);
    assert_eq!(senior_loss, 50);
}
