#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short,
    token, Address, Env, Symbol, Vec,
};

const DEFAULT_YIELD_BPS: u32 = 800;
const BPS_DENOM: u32 = 10_000;
const SECS_PER_YEAR: u64 = 31_536_000;

const LEDGERS_PER_DAY: u32 = 17_280;
const ACTIVE_INVOICE_TTL: u32 = LEDGERS_PER_DAY * 365;
const COMPLETED_INVOICE_TTL: u32 = LEDGERS_PER_DAY * 30;
const POSITION_TTL: u32 = LEDGERS_PER_DAY * 365;
const INSTANCE_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 30;
const INSTANCE_LIFETIME_THRESHOLD: u32 = LEDGERS_PER_DAY * 7;

#[contracttype]
#[derive(Clone)]
pub struct PoolConfig {
    pub invoice_contract: Address,
    pub admin: Address,
    pub yield_bps: u32,
    pub compound_interest: bool,
}

#[contracttype]
#[derive(Clone, Default)]
pub struct PoolTokenTotals {
    pub total_deposited: i128,
    pub total_deployed: i128,
    pub total_paid_out: i128,
}

#[contracttype]
#[derive(Clone)]
pub struct InvestorPosition {
    pub deposited: i128,
    pub available: i128,
    pub deployed: i128,
    pub earned: i128,
    pub deposit_count: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct FundedInvoice {
    pub invoice_id: u64,
    pub sme: Address,
    /// Stablecoin used for this invoice (principal, disbursement, repayment).
    pub token: Address,
    /// Total funding target for this invoice
    pub principal: i128,
    /// Amount committed by co-funders so far; equals principal when fully funded
    pub committed: i128,
    /// Ledger timestamp when fully funded (0 while still open for commitments)
    pub funded_at: u64,
    pub due_date: u64,
    pub repaid: bool,
}

/// Composite key for per-investor, per-invoice share records
#[contracttype]
#[derive(Clone)]
pub struct CoFundKey {
    pub invoice_id: u64,
    pub investor: Address,
}

#[contracttype]
#[derive(Clone)]
pub struct InvestorTokenKey {
    pub investor: Address,
    pub token: Address,
}

#[contracttype]
#[derive(Clone, Default)]
pub struct PoolStorageStats {
    pub total_funded_invoices: u64,
    pub active_funded_invoices: u64,
    pub cleaned_invoices: u64,
}

#[contracttype]
pub enum DataKey {
    Config,
    InvestorPosition(InvestorTokenKey),
    FundedInvoice(u64),
    CoFunders(u64),
    CoFundShare(CoFundKey),
    AcceptedTokens,
    TokenTotals(Address),
    Initialized,
    StorageStats,
}

const EVT: Symbol = symbol_short!("POOL");

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

fn set_funded_invoice_ttl(env: &Env, invoice_id: u64, is_completed: bool) {
    let ttl = if is_completed {
        COMPLETED_INVOICE_TTL
    } else {
        ACTIVE_INVOICE_TTL
    };
    let key = DataKey::FundedInvoice(invoice_id);
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(&key, ttl, ttl);
    }
}

fn set_position_ttl(env: &Env, key: &DataKey) {
    if env.storage().persistent().has(key) {
        env.storage()
            .persistent()
            .extend_ttl(key, POSITION_TTL, POSITION_TTL);
    }
}

fn calculate_interest(principal: u128, yield_bps: u32, elapsed_secs: u64, is_compound: bool) -> u128 {
    let denominator = BPS_DENOM as u128 * SECS_PER_YEAR as u128;

    if !is_compound {
        return (principal * yield_bps as u128 * elapsed_secs as u128) / denominator;
    }

    let elapsed_days = elapsed_secs / 86400;
    let mut amount = principal;
    let daily_rate_num = yield_bps as u128 * 86400;

    for _ in 0..elapsed_days {
        amount += (amount * daily_rate_num) / denominator;
    }

    let remaining_secs = elapsed_secs % 86400;
    if remaining_secs > 0 {
        amount += (amount * yield_bps as u128 * remaining_secs as u128) / denominator;
    }

    amount - principal
}

#[contract]
pub struct FundingPool;

#[contractimpl]
impl FundingPool {
    pub fn initialize(env: Env, admin: Address, initial_token: Address, invoice_contract: Address) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("already initialized");
        }

        let config = PoolConfig {
            invoice_contract,
            admin,
            yield_bps: DEFAULT_YIELD_BPS,
            compound_interest: false,
        };

        let mut tokens: Vec<Address> = Vec::new(&env);
        tokens.push_back(initial_token.clone());

        env.storage().instance().set(&DataKey::Config, &config);
        env.storage().instance().set(&DataKey::AcceptedTokens, &tokens);
        env.storage().instance().set(
            &DataKey::TokenTotals(initial_token),
            &PoolTokenTotals::default(),
        );
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage()
            .instance()
            .set(&DataKey::StorageStats, &PoolStorageStats::default());
        bump_instance(&env);
    }

    pub fn add_token(env: Env, admin: Address, token: Address) {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin);

        let mut tokens: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AcceptedTokens)
            .expect("not initialized");

        for i in 0..tokens.len() {
            if tokens.get(i).unwrap() == token {
                panic!("token already accepted");
            }
        }
        tokens.push_back(token.clone());
        env.storage().instance().set(&DataKey::AcceptedTokens, &tokens);

        if !env
            .storage()
            .instance()
            .has(&DataKey::TokenTotals(token.clone()))
        {
            env.storage().instance().set(
                &DataKey::TokenTotals(token),
                &PoolTokenTotals::default(),
            );
        }
    }

    pub fn remove_token(env: Env, admin: Address, token: Address) {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin);

        let tokens: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AcceptedTokens)
            .expect("not initialized");

        let mut new_tokens: Vec<Address> = Vec::new(&env);
        let mut found = false;
        for i in 0..tokens.len() {
            let t = tokens.get(i).unwrap();
            if t == token {
                found = true;
            } else {
                new_tokens.push_back(t);
            }
        }
        if !found {
            panic!("token not in whitelist");
        }

        let tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&DataKey::TokenTotals(token.clone()))
            .unwrap_or_default();
        if tt.total_deposited != 0 || tt.total_deployed != 0 || tt.total_paid_out != 0 {
            panic!("token has non-zero pool balances");
        }

        env.storage()
            .instance()
            .set(&DataKey::AcceptedTokens, &new_tokens);
    }

    pub fn deposit(env: Env, investor: Address, token: Address, amount: i128) {
        investor.require_auth();
        bump_instance(&env);
        if amount <= 0 {
            panic!("amount must be positive");
        }
        Self::assert_accepted_token(&env, &token);

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&investor, &env.current_contract_address(), &amount);

        let key = InvestorTokenKey {
            investor: investor.clone(),
            token: token.clone(),
        };
        let pos_key = DataKey::InvestorPosition(key.clone());
        let mut position: InvestorPosition = env
            .storage()
            .persistent()
            .get(&pos_key)
            .unwrap_or(InvestorPosition {
                deposited: 0,
                available: 0,
                deployed: 0,
                earned: 0,
                deposit_count: 0,
            });

        position.deposited += amount;
        position.available += amount;
        position.deposit_count += 1;

        env.storage().persistent().set(&pos_key, &position);
        set_position_ttl(&env, &pos_key);

        let mut tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&DataKey::TokenTotals(token.clone()))
            .unwrap_or_default();
        tt.total_deposited += amount;
        env.storage()
            .instance()
            .set(&DataKey::TokenTotals(token), &tt);

        env.events()
            .publish((EVT, symbol_short!("deposit")), (investor, amount));
    }

    pub fn init_co_funding(
        env: Env,
        admin: Address,
        invoice_id: u64,
        principal: i128,
        sme: Address,
        due_date: u64,
        token: Address,
    ) {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin);
        Self::assert_accepted_token(&env, &token);

        if principal <= 0 {
            panic!("principal must be positive");
        }
        if env
            .storage()
            .persistent()
            .has(&DataKey::FundedInvoice(invoice_id))
        {
            panic!("invoice already registered for funding");
        }

        let record = FundedInvoice {
            invoice_id,
            sme,
            token: token.clone(),
            principal,
            committed: 0,
            funded_at: 0,
            due_date,
            repaid: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::FundedInvoice(invoice_id), &record);
        set_funded_invoice_ttl(&env, invoice_id, false);

        let co_funders: Vec<Address> = Vec::new(&env);
        env.storage()
            .persistent()
            .set(&DataKey::CoFunders(invoice_id), &co_funders);

        let mut stats: PoolStorageStats = env
            .storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default();
        stats.total_funded_invoices += 1;
        stats.active_funded_invoices += 1;
        env.storage().instance().set(&DataKey::StorageStats, &stats);
    }

    pub fn commit_to_invoice(
        env: Env,
        investor: Address,
        invoice_id: u64,
        amount: i128,
    ) {
        investor.require_auth();
        bump_instance(&env);
        if amount <= 0 {
            panic!("amount must be positive");
        }

        let mut record: FundedInvoice = env
            .storage()
            .persistent()
            .get(&DataKey::FundedInvoice(invoice_id))
            .expect("invoice not registered for co-funding");

        if record.funded_at != 0 {
            panic!("invoice already fully funded");
        }
        if record.repaid {
            panic!("invoice already repaid");
        }

        let remaining = record.principal - record.committed;
        if amount > remaining {
            panic!("amount exceeds remaining funding gap");
        }

        let pos_key = InvestorTokenKey {
            investor: investor.clone(),
            token: record.token.clone(),
        };
        let pos_data_key = DataKey::InvestorPosition(pos_key.clone());
        let mut position: InvestorPosition = env
            .storage()
            .persistent()
            .get(&pos_data_key)
            .expect("investor has no position in this invoice token");

        if position.available < amount {
            panic!("insufficient available balance");
        }

        position.available -= amount;
        position.deployed += amount;
        env.storage().persistent().set(&pos_data_key, &position);
        set_position_ttl(&env, &pos_data_key);

        let share_key = CoFundKey {
            invoice_id,
            investor: investor.clone(),
        };
        let existing_share: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::CoFundShare(share_key.clone()))
            .unwrap_or(0);

        if existing_share == 0 {
            let mut co_funders: Vec<Address> = env
                .storage()
                .persistent()
                .get(&DataKey::CoFunders(invoice_id))
                .unwrap_or_else(|| Vec::new(&env));
            co_funders.push_back(investor.clone());
            env.storage()
                .persistent()
                .set(&DataKey::CoFunders(invoice_id), &co_funders);
        }

        env.storage()
            .persistent()
            .set(&DataKey::CoFundShare(share_key), &(existing_share + amount));

        record.committed += amount;

        let mut tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&DataKey::TokenTotals(record.token.clone()))
            .unwrap_or_default();
        tt.total_deployed += amount;

        if record.committed == record.principal {
            let token_client = token::Client::new(&env, &record.token);
            token_client.transfer(
                &env.current_contract_address(),
                &record.sme,
                &record.principal,
            );

            record.funded_at = env.ledger().timestamp();
            env.events().publish(
                (EVT, symbol_short!("funded")),
                (invoice_id, record.sme.clone(), record.principal),
            );
        }

        env.storage()
            .persistent()
            .set(&DataKey::FundedInvoice(invoice_id), &record);
        set_funded_invoice_ttl(&env, invoice_id, false);
        env.storage()
            .instance()
            .set(&DataKey::TokenTotals(record.token.clone()), &tt);
    }

    pub fn repay_invoice(env: Env, invoice_id: u64, payer: Address) {
        payer.require_auth();
        bump_instance(&env);

        let config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .expect("not initialized");

        let mut record: FundedInvoice = env
            .storage()
            .persistent()
            .get(&DataKey::FundedInvoice(invoice_id))
            .expect("invoice not found");

        if record.funded_at == 0 {
            panic!("invoice not fully funded yet");
        }
        if record.repaid {
            panic!("already repaid");
        }

        let now = env.ledger().timestamp();
        let elapsed_secs = now - record.funded_at;
        let total_interest = calculate_interest(
            record.principal as u128,
            config.yield_bps,
            elapsed_secs,
            config.compound_interest,
        );
        let total_due = record.principal + total_interest as i128;

        let token_client = token::Client::new(&env, &record.token);
        token_client.transfer(&payer, &env.current_contract_address(), &total_due);

        let co_funders: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::CoFunders(invoice_id))
            .unwrap_or_else(|| Vec::new(&env));

        for investor_addr in co_funders.iter() {
            let share_key = CoFundKey {
                invoice_id,
                investor: investor_addr.clone(),
            };
            let share: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::CoFundShare(share_key))
                .unwrap_or(0);
            if share == 0 {
                continue;
            }

            let investor_interest =
                (total_interest * share as u128 / record.principal as u128) as i128;

            let pos_key = InvestorTokenKey {
                investor: investor_addr.clone(),
                token: record.token.clone(),
            };
            let pos_data_key = DataKey::InvestorPosition(pos_key.clone());
            let mut pos: InvestorPosition = env
                .storage()
                .persistent()
                .get(&pos_data_key)
                .expect("co-funder position missing");

            pos.available += share + investor_interest;
            pos.deployed -= share;
            pos.earned += investor_interest;
            env.storage().persistent().set(&pos_data_key, &pos);
            set_position_ttl(&env, &pos_data_key);
        }

        record.repaid = true;
        env.storage()
            .persistent()
            .set(&DataKey::FundedInvoice(invoice_id), &record);
        set_funded_invoice_ttl(&env, invoice_id, true);

        let mut stats: PoolStorageStats = env
            .storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default();
        stats.active_funded_invoices = stats.active_funded_invoices.saturating_sub(1);
        env.storage().instance().set(&DataKey::StorageStats, &stats);

        let mut tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&DataKey::TokenTotals(record.token.clone()))
            .unwrap_or_default();
        tt.total_deployed -= record.principal;
        tt.total_paid_out += total_due;
        tt.total_deposited += total_interest as i128;
        env.storage()
            .instance()
            .set(&DataKey::TokenTotals(record.token.clone()), &tt);

        env.events().publish(
            (EVT, symbol_short!("repaid")),
            (invoice_id, record.principal, total_interest as i128),
        );
    }

    pub fn withdraw(env: Env, investor: Address, token: Address, amount: i128) {
        investor.require_auth();
        bump_instance(&env);
        if amount <= 0 {
            panic!("amount must be positive");
        }
        Self::assert_accepted_token(&env, &token);

        let key = InvestorTokenKey {
            investor: investor.clone(),
            token: token.clone(),
        };
        let pos_data_key = DataKey::InvestorPosition(key.clone());
        let mut position: InvestorPosition = env
            .storage()
            .persistent()
            .get(&pos_data_key)
            .expect("no position found");

        if position.available < amount {
            panic!("insufficient available balance");
        }

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&env.current_contract_address(), &investor, &amount);

        position.available -= amount;
        position.deposited -= amount;
        env.storage().persistent().set(&pos_data_key, &position);
        set_position_ttl(&env, &pos_data_key);

        let mut tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&DataKey::TokenTotals(token.clone()))
            .unwrap_or_default();
        tt.total_deposited -= amount;
        env.storage()
            .instance()
            .set(&DataKey::TokenTotals(token), &tt);

        env.events()
            .publish((EVT, symbol_short!("withdraw")), (investor, amount));
    }

    pub fn set_yield(env: Env, admin: Address, yield_bps: u32) {
        admin.require_auth();
        bump_instance(&env);
        let mut config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .expect("not initialized");
        Self::require_admin(&env, &admin);
        if yield_bps > 5_000 {
            panic!("yield cannot exceed 50%");
        }
        config.yield_bps = yield_bps;
        env.storage().instance().set(&DataKey::Config, &config);
    }

    pub fn set_compound_interest(env: Env, admin: Address, compound: bool) {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin);

        let mut config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .expect("not initialized");

        config.compound_interest = compound;
        env.storage().instance().set(&DataKey::Config, &config);
    }

    // ---- Views ----

        pub fn get_config(env: Env) -> PoolConfig {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .expect("not initialized")
    }

    pub fn accepted_tokens(env: Env) -> Vec<Address> {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::AcceptedTokens)
            .expect("not initialized")
    }

    pub fn get_token_totals(env: Env, token: Address) -> PoolTokenTotals {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::TokenTotals(token))
            .unwrap_or_default()
    }

    pub fn get_position(env: Env, investor: Address, token: Address) -> Option<InvestorPosition> {
        bump_instance(&env);
        let key = InvestorTokenKey { investor, token };
        env.storage()
            .persistent()
            .get(&DataKey::InvestorPosition(key))
    }

    pub fn get_funded_invoice(env: Env, invoice_id: u64) -> Option<FundedInvoice> {
        bump_instance(&env);
        env.storage()
            .persistent()
            .get(&DataKey::FundedInvoice(invoice_id))
    }

    pub fn get_co_fund_share(env: Env, invoice_id: u64, investor: Address) -> i128 {
        bump_instance(&env);
        env.storage()
            .persistent()
            .get(&DataKey::CoFundShare(CoFundKey { invoice_id, investor }))
            .unwrap_or(0)
    }

    pub fn available_liquidity(env: Env, token: Address) -> i128 {
        bump_instance(&env);
        let tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&DataKey::TokenTotals(token))
            .unwrap_or_default();
        tt.total_deposited - tt.total_deployed
    }

    pub fn get_storage_stats(env: Env) -> PoolStorageStats {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default()
    }

    pub fn cleanup_funded_invoice(env: Env, admin: Address, invoice_id: u64) {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin);

        let record: FundedInvoice = env
            .storage()
            .persistent()
            .get(&DataKey::FundedInvoice(invoice_id))
            .expect("funded invoice not found");

        if !record.repaid {
            panic!("can only cleanup repaid invoices");
        }

        env.storage()
            .persistent()
            .remove(&DataKey::FundedInvoice(invoice_id));

        let co_funders: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::CoFunders(invoice_id))
            .unwrap_or_else(|| Vec::new(&env));

        for investor_addr in co_funders.iter() {
            let share_key = CoFundKey {
                invoice_id,
                investor: investor_addr.clone(),
            };
            env.storage()
                .persistent()
                .remove(&DataKey::CoFundShare(share_key));
        }

        env.storage()
            .persistent()
            .remove(&DataKey::CoFunders(invoice_id));

        let mut stats: PoolStorageStats = env
            .storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default();
        stats.cleaned_invoices += 1;
        env.storage().instance().set(&DataKey::StorageStats, &stats);

        env.events().publish((EVT, symbol_short!("cleanup")), invoice_id);
    }

    pub fn estimate_repayment(env: Env, invoice_id: u64) -> i128 {
        bump_instance(&env);
        let config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .expect("not initialized");
        let record: FundedInvoice = env
            .storage()
            .persistent()
            .get(&DataKey::FundedInvoice(invoice_id))
            .expect("invoice not funded");

        if record.funded_at == 0 {
            return record.principal;
        }

        let now = env.ledger().timestamp();
        let elapsed = now - record.funded_at;
        let interest = calculate_interest(
            record.principal as u128,
            config.yield_bps,
            elapsed,
            config.compound_interest,
        );

        record.principal + interest as i128
    }

    fn require_admin(env: &Env, admin: &Address) {
        let config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .expect("not initialized");
        if admin != &config.admin {
            panic!("unauthorized");
        }
    }

    fn assert_accepted_token(env: &Env, token: &Address) {
        let tokens: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AcceptedTokens)
            .expect("not initialized");
        for i in 0..tokens.len() {
            if tokens.get(i).unwrap() == *token {
                return;
            }
        }
        panic!("token not accepted");
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Env,
    };

    fn setup(env: &Env) -> (FundingPoolClient<'_>, Address, Address) {
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let contract_id = env.register(FundingPool, ());
        let client = FundingPoolClient::new(env, &contract_id);

        let admin = Address::generate(env);
        let token_admin = Address::generate(env);
        let usdc_id = env.register_stellar_asset_contract_v2(token_admin).address();
        let invoice_contract = Address::generate(env);

        client.initialize(&admin, &usdc_id, &invoice_contract);
        (client, admin, usdc_id)
    }

    fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
        soroban_sdk::token::StellarAssetClient::new(env, token_id).mint(to, &amount);
    }

    #[test]
    fn test_deposit_and_withdraw() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, _admin, usdc_id) = setup(&env);
        let investor = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 5_000_000_000);

        client.deposit(&investor, &usdc_id, &2_000_000_000);

        let pos = client.get_position(&investor, &usdc_id).unwrap();
        assert_eq!(pos.deposited, 2_000_000_000);
        assert_eq!(pos.available, 2_000_000_000);

        client.withdraw(&investor, &usdc_id, &500_000_000);
        let pos = client.get_position(&investor, &usdc_id).unwrap();
        assert_eq!(pos.available, 1_500_000_000);

        let tt = client.get_token_totals(&usdc_id);
        assert_eq!(tt.total_deposited, 1_500_000_000);
        let config = client.get_config();
        assert_eq!(config.yield_bps, DEFAULT_YIELD_BPS);
    }

    #[test]
    fn test_add_token_and_remove_unused() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, _usdc_id) = setup(&env);
        let eur_admin = Address::generate(&env);
        let eurc_id = env.register_stellar_asset_contract_v2(eur_admin).address();

        client.add_token(&admin, &eurc_id);
        assert_eq!(client.accepted_tokens().len(), 2);

        client.remove_token(&admin, &eurc_id);
        assert_eq!(client.accepted_tokens().len(), 1);
    }

    #[test]
    #[should_panic(expected = "token has non-zero pool balances")]
    fn test_cannot_remove_token_with_balance() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, usdc_id) = setup(&env);
        let investor = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 1_000_000_000);
        client.deposit(&investor, &usdc_id, &1_000_000_000);
        client.remove_token(&admin, &usdc_id);
    }

    #[test]
    #[should_panic(expected = "token not accepted")]
    fn test_cannot_deposit_unlisted_token() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, _admin, _usdc_id) = setup(&env);
        let other_admin = Address::generate(&env);
        let other = env.register_stellar_asset_contract_v2(other_admin).address();
        let investor = Address::generate(&env);
        mint(&env, &other, &investor, 1_000_000_000);
        client.deposit(&investor, &other, &100);
    }

    #[test]
    fn test_co_funding_two_investors() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, usdc_id) = setup(&env);
        let investor1 = Address::generate(&env);
        let investor2 = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor1, 2_000_000_000);
        mint(&env, &usdc_id, &investor2, 1_000_000_000);

        client.deposit(&investor1, &usdc_id, &2_000_000_000);
        client.deposit(&investor2, &usdc_id, &1_000_000_000);

        let invoice_id: u64 = 1;
        let principal: i128 = 3_000_000_000;
        let due_date = env.ledger().timestamp() + 2_592_000;

        client.init_co_funding(&admin, &invoice_id, &principal, &sme, &due_date, &usdc_id);

        client.commit_to_invoice(&investor1, &invoice_id, &2_000_000_000);
        let record = client.get_funded_invoice(&invoice_id).unwrap();
        assert_eq!(record.committed, 2_000_000_000);
        assert_eq!(record.funded_at, 0);

        client.commit_to_invoice(&investor2, &invoice_id, &1_000_000_000);
        let record = client.get_funded_invoice(&invoice_id).unwrap();
        assert_eq!(record.committed, principal);
        assert!(record.funded_at != 0);

        let pos1 = client.get_position(&investor1, &usdc_id).unwrap();
        assert_eq!(pos1.available, 0);
        assert_eq!(pos1.deployed, 2_000_000_000);

        let pos2 = client.get_position(&investor2, &usdc_id).unwrap();
        assert_eq!(pos2.available, 0);
        assert_eq!(pos2.deployed, 1_000_000_000);

        assert_eq!(client.get_co_fund_share(&invoice_id, &investor1), 2_000_000_000);
        assert_eq!(client.get_co_fund_share(&invoice_id, &investor2), 1_000_000_000);

        let tt = client.get_token_totals(&usdc_id);
        assert_eq!(tt.total_deployed, principal);
    }

    #[test]
    fn test_multi_token_deposit_fund_repay() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, usdc_id) = setup(&env);
        let eur_admin = Address::generate(&env);
        let eurc_id = env.register_stellar_asset_contract_v2(eur_admin).address();
        client.add_token(&admin, &eurc_id);

        let inv_usdc = Address::generate(&env);
        let inv_eur = Address::generate(&env);
        let sme_usdc = Address::generate(&env);
        let sme_eur = Address::generate(&env);

        mint(&env, &usdc_id, &inv_usdc, 2_000_000_000);
        mint(&env, &eurc_id, &inv_eur, 1_000_000_000);
        mint(&env, &usdc_id, &sme_usdc, 5_000_000_000);
        mint(&env, &eurc_id, &sme_eur, 5_000_000_000);

        client.deposit(&inv_usdc, &usdc_id, &2_000_000_000);
        client.deposit(&inv_eur, &eurc_id, &1_000_000_000);

        let due = env.ledger().timestamp() + 2_592_000;

        // USDC invoice
        client.init_co_funding(&admin, &1, &2_000_000_000, &sme_usdc, &due, &usdc_id);
        client.commit_to_invoice(&inv_usdc, &1, &2_000_000_000);
        assert!(client.get_funded_invoice(&1).unwrap().funded_at != 0);

        // EURC invoice
        client.init_co_funding(&admin, &2, &1_000_000_000, &sme_eur, &due, &eurc_id);
        client.commit_to_invoice(&inv_eur, &2, &1_000_000_000);
        assert!(client.get_funded_invoice(&2).unwrap().funded_at != 0);

        env.ledger().with_mut(|l| l.timestamp += 2_592_000);

        client.repay_invoice(&1, &sme_usdc);
        client.repay_invoice(&2, &sme_eur);

        let pos_u = client.get_position(&inv_usdc, &usdc_id).unwrap();
        let pos_e = client.get_position(&inv_eur, &eurc_id).unwrap();
        assert_eq!(pos_u.deployed, 0);
        assert_eq!(pos_e.deployed, 0);
        assert!(pos_u.available >= 2_000_000_000);
        assert!(pos_e.available >= 1_000_000_000);
        assert!(pos_u.earned > 0);
        assert!(pos_e.earned > 0);

        // Withdraw in original tokens
        client.withdraw(&inv_usdc, &usdc_id, &pos_u.available);
        client.withdraw(&inv_eur, &eurc_id, &pos_e.available);

        let tt_u = client.get_token_totals(&usdc_id);
        let tt_e = client.get_token_totals(&eurc_id);
        assert_eq!(tt_u.total_deployed, 0);
        assert_eq!(tt_e.total_deployed, 0);
    }

    #[test]
    fn test_yield_split_proportional() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, usdc_id) = setup(&env);
        let investor1 = Address::generate(&env);
        let investor2 = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor1, 2_000_000_000);
        mint(&env, &usdc_id, &investor2, 1_000_000_000);
        mint(&env, &usdc_id, &sme, 3_100_000_000);

        let invoice_id: u64 = 1;
        let principal: i128 = 3_000_000_000;
        let funded_ts = env.ledger().timestamp();
        let due_date = funded_ts + 2_592_000;

        client.deposit(&investor1, &usdc_id, &2_000_000_000);
        client.deposit(&investor2, &usdc_id, &1_000_000_000);
        client.init_co_funding(&admin, &invoice_id, &principal, &sme, &due_date, &usdc_id);
        client.commit_to_invoice(&investor1, &invoice_id, &2_000_000_000);
        client.commit_to_invoice(&investor2, &invoice_id, &1_000_000_000);

        env.ledger().with_mut(|l| l.timestamp += 2_592_000);

        client.repay_invoice(&invoice_id, &sme);

        let pos1 = client.get_position(&investor1, &usdc_id).unwrap();
        let pos2 = client.get_position(&investor2, &usdc_id).unwrap();

        assert!(pos1.available >= 2_000_000_000);
        assert!(pos2.available >= 1_000_000_000);
        assert!(pos1.earned > 0);
        assert!(pos2.earned > 0);
        assert_eq!(pos1.earned, pos2.earned * 2);
        assert_eq!(pos1.deployed, 0);
        assert_eq!(pos2.deployed, 0);

        let record = client.get_funded_invoice(&invoice_id).unwrap();
        assert!(record.repaid);
    }

    #[test]
    fn test_incremental_commits_same_investor() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, usdc_id) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 3_000_000_000);
        client.deposit(&investor, &usdc_id, &3_000_000_000);

        let due_date = env.ledger().timestamp() + 2_592_000;
        client.init_co_funding(&admin, &1, &3_000_000_000, &sme, &due_date, &usdc_id);

        client.commit_to_invoice(&investor, &1, &1_000_000_000);
        client.commit_to_invoice(&investor, &1, &2_000_000_000);

        assert_eq!(client.get_co_fund_share(&1, &investor), 3_000_000_000);

        let record = client.get_funded_invoice(&1).unwrap();
        assert!(record.funded_at != 0);
    }

    #[test]
    #[should_panic(expected = "amount exceeds remaining funding gap")]
    fn test_cannot_over_commit() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, usdc_id) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 5_000_000_000);
        client.deposit(&investor, &usdc_id, &5_000_000_000);

        let due_date = env.ledger().timestamp() + 2_592_000;
        client.init_co_funding(&admin, &1, &3_000_000_000, &sme, &due_date, &usdc_id);

        client.commit_to_invoice(&investor, &1, &4_000_000_000);
    }

    #[test]
    #[should_panic(expected = "invoice not fully funded yet")]
    fn test_cannot_repay_before_fully_funded() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, usdc_id) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 5_000_000_000);
        mint(&env, &usdc_id, &sme, 5_000_000_000);
        client.deposit(&investor, &usdc_id, &5_000_000_000);

        let due_date = env.ledger().timestamp() + 2_592_000;
        client.init_co_funding(&admin, &1, &3_000_000_000, &sme, &due_date, &usdc_id);
        client.commit_to_invoice(&investor, &1, &1_000_000_000);

        client.repay_invoice(&1, &sme);
    }

    // ---- Integration Tests: Full Deposit → Fund → Repay → Withdraw Cycle ----

    #[test]
    fn test_full_cycle_deposit_fund_repay_withdraw() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, usdc_id) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        let deposit_amount: i128 = 1_000_000_000;
        let repay_buffer: i128 = 100_000_000;
        mint(&env, &usdc_id, &investor, deposit_amount);
        mint(&env, &usdc_id, &sme, deposit_amount + repay_buffer);

        client.deposit(&investor, &usdc_id, &deposit_amount);
        let pos = client.get_position(&investor, &usdc_id).unwrap();
        assert_eq!(pos.deposited, deposit_amount);
        assert_eq!(pos.available, deposit_amount);
        assert_eq!(pos.deployed, 0);
        assert_eq!(pos.earned, 0);
        assert_eq!(pos.deposit_count, 1);

        let tt = client.get_token_totals(&usdc_id);
        assert_eq!(tt.total_deposited, deposit_amount);
        assert_eq!(tt.total_deployed, 0);

        let invoice_id: u64 = 100;
        let principal = deposit_amount;
        let due_date = env.ledger().timestamp() + 2_592_000;

        client.init_co_funding(&admin, &invoice_id, &principal, &sme, &due_date, &usdc_id);
        client.commit_to_invoice(&investor, &invoice_id, &principal);

        let pos = client.get_position(&investor, &usdc_id).unwrap();
        assert_eq!(pos.available, 0);
        assert_eq!(pos.deployed, principal);

        let record = client.get_funded_invoice(&invoice_id).unwrap();
        assert_eq!(record.principal, principal);
        assert_eq!(record.committed, principal);
        assert!(record.funded_at > 0);
        assert!(!record.repaid);

        let tt = client.get_token_totals(&usdc_id);
        assert_eq!(tt.total_deployed, principal);

        let elapsed_days = 30u64;
        env.ledger().with_mut(|l| l.timestamp += elapsed_days * 86_400);

        let initial_available = pos.available;
        client.repay_invoice(&invoice_id, &sme);

        let pos = client.get_position(&investor, &usdc_id).unwrap();
        assert_eq!(pos.deployed, 0);
        assert!(pos.available > initial_available + principal);
        assert!(pos.earned > 0);

        let expected_interest = (principal as u128 * DEFAULT_YIELD_BPS as u128 
            * (elapsed_days * 86_400) as u128) 
            / (BPS_DENOM as u128 * SECS_PER_YEAR as u128);
        assert_eq!(pos.earned, expected_interest as i128);

        let record = client.get_funded_invoice(&invoice_id).unwrap();
        assert!(record.repaid);

        let withdraw_amount = pos.available;
        let earned_before_withdraw = pos.earned;
        client.withdraw(&investor, &usdc_id, &withdraw_amount);

        let pos = client.get_position(&investor, &usdc_id).unwrap();
        assert_eq!(pos.available, 0);
        assert_eq!(pos.deployed, 0);
        assert_eq!(pos.earned, earned_before_withdraw);
    }

    #[test]
    fn test_interest_calculation_various_time_periods() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, usdc_id) = setup(&env);
        let principal: i128 = 1_000_000_000;

        let investor1 = Address::generate(&env);
        let sme1 = Address::generate(&env);
        mint(&env, &usdc_id, &investor1, principal);
        mint(&env, &usdc_id, &sme1, principal + 100_000_000);

        client.deposit(&investor1, &usdc_id, &principal);
        client.init_co_funding(&admin, &1, &principal, &sme1, &(env.ledger().timestamp() + 604_800), &usdc_id);
        client.commit_to_invoice(&investor1, &1, &principal);

        env.ledger().with_mut(|l| l.timestamp += 7 * 86_400);
        client.repay_invoice(&1, &sme1);

        let pos1 = client.get_position(&investor1, &usdc_id).unwrap();
        let expected_7d = (principal as u128 * DEFAULT_YIELD_BPS as u128 * (7 * 86_400) as u128) 
            / (BPS_DENOM as u128 * SECS_PER_YEAR as u128);
        assert_eq!(pos1.earned, expected_7d as i128);

        let investor2 = Address::generate(&env);
        let sme2 = Address::generate(&env);
        mint(&env, &usdc_id, &investor2, principal);
        mint(&env, &usdc_id, &sme2, principal + 200_000_000);

        client.deposit(&investor2, &usdc_id, &principal);
        client.init_co_funding(&admin, &2, &principal, &sme2, &(env.ledger().timestamp() + 7_776_000), &usdc_id);
        client.commit_to_invoice(&investor2, &2, &principal);

        env.ledger().with_mut(|l| l.timestamp += 90 * 86_400);
        client.repay_invoice(&2, &sme2);

        let pos2 = client.get_position(&investor2, &usdc_id).unwrap();
        let expected_90d = (principal as u128 * DEFAULT_YIELD_BPS as u128 * (90 * 86_400) as u128) 
            / (BPS_DENOM as u128 * SECS_PER_YEAR as u128);
        assert_eq!(pos2.earned, expected_90d as i128);

        let investor3 = Address::generate(&env);
        let sme3 = Address::generate(&env);
        mint(&env, &usdc_id, &investor3, principal);
        mint(&env, &usdc_id, &sme3, principal + 300_000_000);

        client.deposit(&investor3, &usdc_id, &principal);
        client.init_co_funding(&admin, &3, &principal, &sme3, &(env.ledger().timestamp() + SECS_PER_YEAR), &usdc_id);
        client.commit_to_invoice(&investor3, &3, &principal);

        env.ledger().with_mut(|l| l.timestamp += SECS_PER_YEAR);
        client.repay_invoice(&3, &sme3);

        let pos3 = client.get_position(&investor3, &usdc_id).unwrap();
        let expected_365d = (principal as u128 * DEFAULT_YIELD_BPS as u128 * SECS_PER_YEAR as u128) 
            / (BPS_DENOM as u128 * SECS_PER_YEAR as u128);
        assert_eq!(pos3.earned, expected_365d as i128);

        assert!(pos2.earned > pos1.earned);
        assert!(pos3.earned > pos2.earned);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_deposit_zero_amount_panics() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, _admin, usdc_id) = setup(&env);
        let investor = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 1_000_000_000);

        client.deposit(&investor, &usdc_id, &0);
    }

    #[test]
    #[should_panic(expected = "insufficient available balance")]
    fn test_fund_invoice_insufficient_liquidity_panics() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, usdc_id) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 500_000_000);
        client.deposit(&investor, &usdc_id, &500_000_000);

        let invoice_id: u64 = 1;
        let principal: i128 = 1_000_000_000;
        let due_date = env.ledger().timestamp() + 2_592_000;

        client.init_co_funding(&admin, &invoice_id, &principal, &sme, &due_date, &usdc_id);
        
        client.commit_to_invoice(&investor, &invoice_id, &1_000_000_000);
    }

    #[test]
    #[should_panic(expected = "already repaid")]
    fn test_repay_already_repaid_invoice_panics() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, usdc_id) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        let principal: i128 = 1_000_000_000;
        mint(&env, &usdc_id, &investor, principal);
        mint(&env, &usdc_id, &sme, principal * 2);

        client.deposit(&investor, &usdc_id, &principal);
        
        let invoice_id: u64 = 1;
        let due_date = env.ledger().timestamp() + 2_592_000;
        client.init_co_funding(&admin, &invoice_id, &principal, &sme, &due_date, &usdc_id);
        client.commit_to_invoice(&investor, &invoice_id, &principal);

        env.ledger().with_mut(|l| l.timestamp += 30 * 86_400);
        
        client.repay_invoice(&invoice_id, &sme);
        
        client.repay_invoice(&invoice_id, &sme);
    }

    #[test]
    #[should_panic(expected = "insufficient available balance")]
    fn test_withdraw_more_than_available_panics() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, _admin, usdc_id) = setup(&env);
        let investor = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 1_000_000_000);
        client.deposit(&investor, &usdc_id, &1_000_000_000);

        client.withdraw(&investor, &usdc_id, &2_000_000_000);
    }

    #[test]
    #[should_panic(expected = "yield cannot exceed 50%")]
    fn test_set_yield_above_50_percent_panics() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, _usdc_id) = setup(&env);

        client.set_yield(&admin, &5_100);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_fund_invoice_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, _admin, usdc_id) = setup(&env);
        let non_admin = Address::generate(&env);
        let sme = Address::generate(&env);

        let invoice_id: u64 = 1;
        let principal: i128 = 1_000_000_000;
        let due_date = env.ledger().timestamp() + 2_592_000;

        client.init_co_funding(&non_admin, &invoice_id, &principal, &sme, &due_date, &usdc_id);
    }

    #[test]
    fn test_set_yield_at_boundary_50_percent() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, _usdc_id) = setup(&env);

        // Setting yield to exactly 50% (5000 bps) should succeed
        client.set_yield(&admin, &5_000);

        let config = client.get_config();
        assert_eq!(config.yield_bps, 5_000);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
        #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_withdraw_zero_amount_panics() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, _admin, usdc_id) = setup(&env);
        let investor = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 1_000_000_000);
        client.deposit(&investor, &usdc_id, &1_000_000_000);

        client.withdraw(&investor, &usdc_id, &0);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_deposit_negative_amount_panics() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, _admin, usdc_id) = setup(&env);
        let investor = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 1_000_000_000);

        client.deposit(&investor, &usdc_id, &-100);
    }

    #[test]
    fn test_storage_stats() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, usdc_id) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        let stats = client.get_storage_stats();
        assert_eq!(stats.total_funded_invoices, 0);

        mint(&env, &usdc_id, &investor, 2_000_000_000);
        mint(&env, &usdc_id, &sme, 2_000_000_000);
        client.deposit(&investor, &usdc_id, &1_000_000_000);

        let due = env.ledger().timestamp() + 2_592_000;
        client.init_co_funding(&admin, &1, &1_000_000_000, &sme, &due, &usdc_id);

        let stats = client.get_storage_stats();
        assert_eq!(stats.total_funded_invoices, 1);
        assert_eq!(stats.active_funded_invoices, 1);

        client.commit_to_invoice(&investor, &1, &1_000_000_000);
        env.ledger().with_mut(|l| l.timestamp += 30 * 86_400);
        client.repay_invoice(&1, &sme);

        let stats = client.get_storage_stats();
        assert_eq!(stats.active_funded_invoices, 0);
    }

    #[test]
    fn test_cleanup_funded_invoice() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, usdc_id) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 2_000_000_000);
        mint(&env, &usdc_id, &sme, 2_000_000_000);
        client.deposit(&investor, &usdc_id, &1_000_000_000);

        let due = env.ledger().timestamp() + 2_592_000;
        client.init_co_funding(&admin, &1, &1_000_000_000, &sme, &due, &usdc_id);
        client.commit_to_invoice(&investor, &1, &1_000_000_000);
        env.ledger().with_mut(|l| l.timestamp += 30 * 86_400);
        client.repay_invoice(&1, &sme);

        let stats_before = client.get_storage_stats();
        client.cleanup_funded_invoice(&admin, &1);
        let stats_after = client.get_storage_stats();

        assert_eq!(stats_after.cleaned_invoices, stats_before.cleaned_invoices + 1);
        assert!(client.get_funded_invoice(&1).is_none());
    }

    #[test]
    fn test_set_compound_interest() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, _usdc_id) = setup(&env);

        let config = client.get_config();
        assert_eq!(config.compound_interest, false);

        client.set_compound_interest(&admin, &true);

        let config = client.get_config();
        assert_eq!(config.compound_interest, true);
    }

    #[test]
    fn test_compound_vs_simple_interest() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, usdc_id) = setup(&env);
        let principal: i128 = 10_000_000_000_000;

        let investor1 = Address::generate(&env);
        let sme1 = Address::generate(&env);
        mint(&env, &usdc_id, &investor1, principal);
        mint(&env, &usdc_id, &sme1, principal + 1_000_000_000_000);

        client.deposit(&investor1, &usdc_id, &principal);
        // Fund invoice for a long enough time to see divergence between simple and compound.
        client.init_co_funding(&admin, &1, &principal, &sme1, &(env.ledger().timestamp() + SECS_PER_YEAR * 2), &usdc_id);
        client.commit_to_invoice(&investor1, &1, &principal);

        let elapsed_days = 90u64;
        env.ledger().with_mut(|l| l.timestamp += elapsed_days * 86_400);

        let est_simple = client.estimate_repayment(&1);

        client.set_compound_interest(&admin, &true);
        let est_compound = client.estimate_repayment(&1);

        assert!(est_compound > est_simple);

        client.repay_invoice(&1, &sme1);
        let pos = client.get_position(&investor1, &usdc_id).unwrap();
        
        let interest = est_compound - principal;
        assert_eq!(pos.earned, interest);
    }
}
