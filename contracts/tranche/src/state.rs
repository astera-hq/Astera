use soroban_sdk::{contracttype, Address};

#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TrancheClass {
    Senior,
    Junior,
}

#[contracttype]
#[derive(Clone, Copy)]
pub struct TrancheConfig {
    pub senior_target_yield_bps: u32,
    pub senior_advance_rate_bps: u32,
    pub junior_first_loss_bps: u32,
}

#[contracttype]
#[derive(Clone, Default, Copy)]
pub struct TrancheAccounting {
    pub deposited: i128,
    pub available: i128,
    pub deployed: i128,
    pub earned: i128,
    pub losses: i128,
    pub total_shares: i128,
}

#[contracttype]
#[derive(Clone)]
pub struct TranchePool {
    pub token: Address,
    pub senior_share_token: Address,
    pub junior_share_token: Address,
    pub config: TrancheConfig,
    pub senior: TrancheAccounting,
    pub junior: TrancheAccounting,
}

#[contracttype]
#[derive(Clone, Default)]
pub struct InvestorPosition {
    pub deposited: i128,
    pub shares: i128,
    pub earned: i128,
    pub losses: i128,
}

#[contracttype]
#[derive(Clone)]
pub struct InvoiceTrancheExposure {
    pub invoice_id: u64,
    pub senior_deployed: i128,
    pub junior_deployed: i128,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Pool(Address),
    Investor(Address, Address, TrancheClass),
    TrancheEnabled(Address),
    NonReentrantKey,
    InvoiceExposure(u64),
}
