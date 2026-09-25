use soroban_sdk::Env;

pub fn calculate_waterfall_split(
    _env: &Env,
    total_due: i128,
    senior_principal: i128,
    senior_target_yield_bps: u32,
    elapsed_secs: u64,
) -> (i128, i128) {
    let yearly = 365u64 * 24 * 60 * 60;

    let interest = (senior_principal as u128)
        .saturating_mul(senior_target_yield_bps as u128)
        .saturating_mul(elapsed_secs as u128)
        .saturating_div(10_000)
        .saturating_div(yearly) as i128;

    let senior_cap = senior_principal + interest;

    let senior_amount = if total_due >= senior_cap {
        senior_cap
    } else {
        total_due
    };

    let junior_amount = total_due - senior_amount;

    (senior_amount, junior_amount)
}

pub fn calculate_loss_allocation(shortfall: i128, junior_remaining: i128) -> (i128, i128) {
    if shortfall <= junior_remaining {
        (shortfall, 0)
    } else {
        (junior_remaining, shortfall - junior_remaining)
    }
}

pub fn calculate_shares_to_mint(
    amount: i128,
    pool_value: i128,
    total_shares: i128,
) -> i128 {
    if total_shares == 0 || pool_value == 0 {
        return amount;
    }
    if pool_value <= 0 {
        return amount;
    }
    (amount as u128)
        .saturating_mul(total_shares as u128)
        .saturating_div(pool_value as u128) as i128
}
