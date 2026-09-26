use soroban_sdk::{contracttype, panic_with_error, Address, Env};

use crate::{
    errors::TrancheError,
    events::{EVT, FUND},
    state::{DataKey, InvoiceTrancheExposure, TranchePool},
};

#[contracttype]
#[derive(Clone)]
pub struct TrancheFundingSplit {
    pub senior_amount: i128,
    pub junior_amount: i128,
}

pub fn fund_invoice_from_tranches(
    env: &Env,
    token: Address,
    invoice_id: u64,
    total_amount: i128,
) -> TrancheFundingSplit {
    if env
        .storage()
        .instance()
        .has(&DataKey::InvoiceExposure(invoice_id))
    {
        panic_with_error!(env, TrancheError::AlreadyInitialized);
    }

    let mut pool: TranchePool = env
        .storage()
        .instance()
        .get(&DataKey::Pool(token.clone()))
        .unwrap_or_else(|| panic_with_error!(env, TrancheError::PoolNotFound));

    // Calculate proportional split based on senior advance rate
    let senior_amount = total_amount * pool.config.senior_advance_rate_bps as i128 / 10_000;
    let junior_amount = total_amount - senior_amount;

    // Check availability
    if pool.senior.available < senior_amount {
        panic_with_error!(env, TrancheError::InsufficientBalance);
    }
    if pool.junior.available < junior_amount {
        panic_with_error!(env, TrancheError::InsufficientBalance);
    }

    // Update accounting
    pool.senior.available -= senior_amount;
    pool.senior.deployed += senior_amount;
    pool.junior.available -= junior_amount;
    pool.junior.deployed += junior_amount;

    // Record tranche exposure for this invoice
    let exposure = InvoiceTrancheExposure {
        invoice_id,
        senior_deployed: senior_amount,
        junior_deployed: junior_amount,
    };
    env.storage()
        .instance()
        .set(&DataKey::InvoiceExposure(invoice_id), &exposure);

    env.storage()
        .instance()
        .set(&DataKey::Pool(token.clone()), &pool);

    env.events().publish(
        (EVT, FUND),
        (invoice_id, token, senior_amount, junior_amount),
    );

    TrancheFundingSplit {
        senior_amount,
        junior_amount,
    }
}

pub fn get_invoice_exposure(env: &Env, invoice_id: u64) -> Option<InvoiceTrancheExposure> {
    env.storage()
        .instance()
        .get(&DataKey::InvoiceExposure(invoice_id))
}
