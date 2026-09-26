use soroban_sdk::{panic_with_error, Address, Env};

use crate::{
    errors::TrancheError,
    events::{DEFAULT, EVT, REPAY},
    math::{calculate_loss_allocation, calculate_waterfall_split},
    state::{DataKey, InvoiceTrancheExposure, TranchePool},
};

pub fn distribute_waterfall_repayment(
    env: &Env,
    token: Address,
    invoice_id: u64,
    total_due: i128,
    elapsed_secs: u64,
) -> (i128, i128) {
    let mut pool: TranchePool = env
        .storage()
        .instance()
        .get(&DataKey::Pool(token.clone()))
        .unwrap_or_else(|| panic_with_error!(env, TrancheError::PoolNotFound));

    // Get invoice exposure to determine senior principal for this invoice
    let exposure: InvoiceTrancheExposure = env
        .storage()
        .instance()
        .get(&DataKey::InvoiceExposure(invoice_id))
        .unwrap_or_else(|| panic_with_error!(env, TrancheError::ExposureNotFound));

    // Calculate waterfall split
    let (senior_amount, junior_amount) = calculate_waterfall_split(
        env,
        total_due,
        exposure.senior_deployed,
        pool.config.senior_target_yield_bps,
        pool.config.junior_first_loss_bps,
        elapsed_secs,
    );

    // Update senior accounting
    pool.senior.deployed -= exposure.senior_deployed;
    pool.senior.earned += senior_amount;
    pool.senior.available += senior_amount;

    // Update junior accounting
    pool.junior.deployed -= exposure.junior_deployed;
    pool.junior.earned += junior_amount;
    pool.junior.available += junior_amount;

    env.storage()
        .instance()
        .set(&DataKey::Pool(token.clone()), &pool);

    env.storage()
        .instance()
        .remove(&DataKey::InvoiceExposure(invoice_id));

    env.events().publish(
        (EVT, REPAY),
        (invoice_id, token, senior_amount, junior_amount),
    );

    (senior_amount, junior_amount)
}

pub fn allocate_loss(env: &Env, token: Address, invoice_id: u64, shortfall: i128) {
    let mut pool: TranchePool = env
        .storage()
        .instance()
        .get(&DataKey::Pool(token.clone()))
        .unwrap_or_else(|| panic_with_error!(env, TrancheError::PoolNotFound));

    // Get invoice exposure
    let exposure: InvoiceTrancheExposure = env
        .storage()
        .instance()
        .get(&DataKey::InvoiceExposure(invoice_id))
        .unwrap_or_else(|| panic_with_error!(env, TrancheError::ExposureNotFound));

    // Calculate loss allocation
    let junior_remaining = pool.junior.deployed + pool.junior.available;
    let (junior_loss, senior_loss) = calculate_loss_allocation(
        shortfall,
        junior_remaining,
        pool.config.junior_first_loss_bps,
    );

    // Apply junior loss
    if junior_loss > 0 {
        pool.junior.losses += junior_loss;
        // Reduce junior's deployed/available proportionally
        let junior_total = pool.junior.deployed + pool.junior.available;
        if junior_total > 0 {
            let deployed_ratio = pool.junior.deployed as u128 / junior_total as u128;
            let available_loss = junior_loss - (junior_loss as u128 * deployed_ratio) as i128;
            let deployed_loss = junior_loss - available_loss;
            pool.junior.deployed -= deployed_loss;
            pool.junior.available -= available_loss;
        }
    }

    // Apply senior loss (only after junior is exhausted)
    if senior_loss > 0 {
        pool.senior.losses += senior_loss;
        let senior_total = pool.senior.deployed + pool.senior.available;
        if senior_total > 0 {
            let deployed_ratio = pool.senior.deployed as u128 / senior_total as u128;
            let available_loss = senior_loss - (senior_loss as u128 * deployed_ratio) as i128;
            let deployed_loss = senior_loss - available_loss;
            pool.senior.deployed -= deployed_loss;
            pool.senior.available -= available_loss;
        }
    }

    // Remove deployed amounts from this invoice
    pool.senior.deployed -= exposure.senior_deployed;
    pool.junior.deployed -= exposure.junior_deployed;

    env.storage()
        .instance()
        .set(&DataKey::Pool(token.clone()), &pool);

    env.storage()
        .instance()
        .remove(&DataKey::InvoiceExposure(invoice_id));

    env.events().publish(
        (EVT, DEFAULT),
        (invoice_id, token, junior_loss, senior_loss),
    );
}
