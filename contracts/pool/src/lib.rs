#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short,
    token, Address, BytesN, Env, Symbol, Vec, IntoVal,
};

const DEFAULT_YIELD_BPS: u32 = 800;
const DEFAULT_FACTORING_FEE_BPS: u32 = 0;
const BPS_DENOM: u32 = 10_000;
const SECS_PER_YEAR: u64 = 31_536_000;

const LEDGERS_PER_DAY: u32 = 17_280;
const ACTIVE_INVOICE_TTL: u32 = LEDGERS_PER_DAY * 365;
const COMPLETED_INVOICE_TTL: u32 = LEDGERS_PER_DAY * 30;
const INSTANCE_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 30;
const INSTANCE_LIFETIME_THRESHOLD: u32 = LEDGERS_PER_DAY * 7;
const UPGRADE_TIMELOCK_SECS: u64 = 86400; // 24 hours

#[contracttype]
#[derive(Clone)]
pub struct PoolConfig {
    pub invoice_contract: Address,
    pub admin: Address,
    pub yield_bps: u32,
    pub factoring_fee_bps: u32,
    pub compound_interest: bool,
}

#[contracttype]
#[derive(Clone, Default)]
pub struct PoolTokenTotals {
    pub pool_value: i128,
    pub total_deployed: i128,
    pub total_paid_out: i128,
    pub total_fee_revenue: i128,
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
    pub token: Address,
    pub principal: i128,
    pub funded_at: u64,
    /// Protocol fee locked when the invoice becomes fully funded.
    pub factoring_fee: i128,
    pub due_date: u64,
    pub repaid: bool,
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
    ShareToken(Address),
    FundedInvoice(u64),
    AcceptedTokens,
    TokenTotals(Address),
    Initialized,
    StorageStats,
    Paused,
    ProposedWasmHash,
    UpgradeScheduledAt,
    // #111: exchange rate for each accepted token (bps of USD, e.g. 10000 = 1:1 USD)
    ExchangeRate(Address),
    // #109: KYC / investor whitelist
    KycRequired,
    InvestorKyc(Address),
}

const EVT: Symbol = symbol_short!("POOL");

// Cache for config to reduce storage reads
fn get_config_cached(env: &Env) -> PoolConfig {
    env.storage().instance().get(&DataKey::Config).unwrap()
}

// Optimized bump that only extends if needed
fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

fn require_not_paused(env: &Env) {
    if env
        .storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Paused)
        .unwrap_or(false)
    {
        panic!("contract is paused");
    }
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

fn calculate_interest(
    principal: u128,
    yield_bps: u32,
    elapsed_secs: u64,
    is_compound: bool,
) -> u128 {
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

fn calculate_factoring_fee(principal: i128, factoring_fee_bps: u32) -> i128 {
    ((principal as u128 * factoring_fee_bps as u128) / BPS_DENOM as u128) as i128
}

#[contract]
pub struct FundingPool;

#[contractimpl]
impl FundingPool {
    pub fn initialize(
        env: Env,
        admin: Address,
        initial_token: Address,
        initial_share_token: Address,
        invoice_contract: Address,
    ) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("already initialized");
        }

        let config = PoolConfig {
            invoice_contract,
            admin: admin.clone(),
            yield_bps: DEFAULT_YIELD_BPS,
            factoring_fee_bps: DEFAULT_FACTORING_FEE_BPS,
            compound_interest: false,
        };

        let mut tokens: Vec<Address> = Vec::new(&env);
        tokens.push_back(initial_token.clone());

        env.storage().instance().set(&DataKey::Config, &config);
        env.storage()
            .instance()
            .set(&DataKey::AcceptedTokens, &tokens);
        env.storage().instance().set(
            &DataKey::TokenTotals(initial_token.clone()),
            &PoolTokenTotals::default(),
        );
        env.storage()
            .instance()
            .set(&DataKey::ShareToken(initial_token), &initial_share_token);
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage()
            .instance()
            .set(&DataKey::StorageStats, &PoolStorageStats::default());
        env.storage().instance().set(&DataKey::Paused, &false);
        bump_instance(&env);
    }

    pub fn pause(env: Env, admin: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        env.storage().instance().set(&DataKey::Paused, &true);
        bump_instance(&env);
        env.events().publish((EVT, symbol_short!("paused")), admin);
    }

    pub fn unpause(env: Env, admin: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        env.storage().instance().set(&DataKey::Paused, &false);
        bump_instance(&env);
        env.events()
            .publish((EVT, symbol_short!("unpaused")), admin);
    }

    pub fn is_paused(env: Env) -> bool {
        bump_instance(&env);
        env.storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn add_token(env: Env, admin: Address, token: Address, share_token: Address) {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
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
        env.storage()
            .instance()
            .set(&DataKey::AcceptedTokens, &tokens);
        env.events()
            .publish((EVT, symbol_short!("add_token")), (admin, token.clone()));

        if !env
            .storage()
            .instance()
            .has(&DataKey::TokenTotals(token.clone()))
        {
            env.storage().instance().set(
                &DataKey::TokenTotals(token.clone()),
                &PoolTokenTotals::default(),
            );
            env.storage()
                .instance()
                .set(&DataKey::ShareToken(token), &share_token);
        }
    }

    pub fn remove_token(env: Env, admin: Address, token: Address) {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
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
        if tt.pool_value != 0 || tt.total_deployed != 0 {
            panic!("token has non-zero pool balances");
        }

        env.storage()
            .instance()
            .set(&DataKey::AcceptedTokens, &new_tokens);
        env.events()
            .publish((EVT, symbol_short!("rm_token")), (admin, token));
    }

    pub fn deposit(env: Env, investor: Address, token: Address, amount: i128) {
        investor.require_auth();
        bump_instance(&env);
        if amount <= 0 {
            panic!("amount must be positive");
        }
        Self::assert_accepted_token(&env, &token);

        // #109: enforce KYC check when required
        let kyc_required: bool = env
            .storage()
            .instance()
            .get(&DataKey::KycRequired)
            .unwrap_or(false);
        if kyc_required {
            let approved: bool = env
                .storage()
                .persistent()
                .get(&DataKey::InvestorKyc(investor.clone()))
                .unwrap_or(false);
            if !approved {
                panic!("investor not KYC approved");
            }
        }

        // Transfer tokens first
        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&investor, &env.current_contract_address(), &amount);

        // Batch read: get both token totals and share token in one go
        let token_totals_key = DataKey::TokenTotals(token.clone());
        let share_token_key = DataKey::ShareToken(token.clone());
        
        let mut tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&token_totals_key)
            .unwrap_or_default();

        let share_token: Address = env
            .storage()
            .instance()
            .get(&share_token_key)
            .unwrap();

        // Calculate shares (single external call)
        let total_shares: i128 = env.invoke_contract(
            &share_token,
            &Symbol::new(&env, "total_supply"),
            Vec::new(&env),
        );
        
        let shares_to_mint = if total_shares == 0 || tt.pool_value == 0 {
            amount
        } else {
            (amount as i128 * total_shares) / tt.pool_value
        };

        // Update pool value
        tt.pool_value += amount;
        
        // Batch write: update token totals
        env.storage()
            .instance()
            .set(&token_totals_key, &tt);

        // Mint shares (single external call)
        let mut mint_args = Vec::new(&env);
        mint_args.push_back(investor.clone().into_val(&env));
        mint_args.push_back(shares_to_mint.into_val(&env));
        let _: () = env.invoke_contract(&share_token, &Symbol::new(&env, "mint"), mint_args);

        env.events().publish(
            (EVT, symbol_short!("deposit")),
            (investor, amount, shares_to_mint),
        );
    }

    pub fn withdraw(env: Env, investor: Address, token: Address, shares: i128) {
        investor.require_auth();
        bump_instance(&env);
        if shares <= 0 {
            panic!("shares must be positive");
        }
        Self::assert_accepted_token(&env, &token);

        // Batch read: get share token and token totals
        let share_token_key = DataKey::ShareToken(token.clone());
        let token_totals_key = DataKey::TokenTotals(token.clone());
        
        let share_token: Address = env
            .storage()
            .instance()
            .get(&share_token_key)
            .unwrap();
            
        let mut tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&token_totals_key)
            .unwrap_or_default();

        // Batch external calls: get balance and total supply
        let mut bal_args = Vec::new(&env);
        bal_args.push_back(investor.clone().into_val(&env));
        let share_balance: i128 =
            env.invoke_contract(&share_token, &Symbol::new(&env, "balance"), bal_args);
        if share_balance < shares {
            panic!("insufficient shares");
        }

        let total_shares: i128 = env.invoke_contract(
            &share_token,
            &Symbol::new(&env, "total_supply"),
            Vec::new(&env),
        );

        // Calculate amount and check liquidity
        let amount = (shares as i128 * tt.pool_value) / total_shares;
        let available_liquidity = tt.pool_value - tt.total_deployed;
        if available_liquidity < amount {
            panic!("insufficient available liquidity");
        }

        // Burn shares
        let mut burn_args = Vec::new(&env);
        burn_args.push_back(investor.clone().into_val(&env));
        burn_args.push_back(shares.into_val(&env));
        let _: () = env.invoke_contract(&share_token, &Symbol::new(&env, "burn"), burn_args);

        // Update pool value and write once
        tt.pool_value -= amount;
        env.storage()
            .instance()
            .set(&token_totals_key, &tt);

        // Transfer tokens
        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&env.current_contract_address(), &investor, &amount);

        env.events()
            .publish((EVT, symbol_short!("withdraw")), (investor, amount, shares));
    }

    pub fn fund_invoice(
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
        Self::require_not_paused(&env);
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
            panic!("invoice already funded");
        }

        let mut tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&DataKey::TokenTotals(token.clone()))
            .unwrap_or_default();
        let available = tt.pool_value - tt.total_deployed;
        if available < principal {
            panic!("insufficient available liquidity");
        }

        let config: PoolConfig = env.storage().instance().get(&DataKey::Config).unwrap();
        let factoring_fee = calculate_factoring_fee(principal, config.factoring_fee_bps);

        let record = FundedInvoice {
            invoice_id,
            sme: sme.clone(),
            token: token.clone(),
            principal,
            funded_at: env.ledger().timestamp(),
            due_date,
            factoring_fee,
            repaid: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::FundedInvoice(invoice_id), &record);
        set_funded_invoice_ttl(&env, invoice_id, false);

        tt.total_deployed += principal;
        env.storage()
            .instance()
            .set(&DataKey::TokenTotals(token.clone()), &tt);

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&env.current_contract_address(), &sme, &principal);

        let mut stats: PoolStorageStats = env
            .storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default();
        stats.total_funded_invoices += 1;
        stats.active_funded_invoices += 1;
        env.storage().instance().set(&DataKey::StorageStats, &stats);

        env.events().publish(
            (EVT, symbol_short!("funded")),
            (invoice_id, sme.clone(), principal),
        );
    }

    pub fn repay_invoice(env: Env, invoice_id: u64, payer: Address) {
        payer.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);

        // Batch read: get config and funded invoice record
        let config: PoolConfig = get_config_cached(&env);
        let funded_invoice_key = DataKey::FundedInvoice(invoice_id);
        let mut record: FundedInvoice = env
            .storage()
            .persistent()
            .get(&funded_invoice_key)
            .expect("invoice not found");

        if record.repaid {
            panic!("already repaid");
        }

        // Calculate interest (pure computation, no storage access)
        let now = env.ledger().timestamp();
        let elapsed_secs = now - record.funded_at;
        let total_interest = calculate_interest(
            record.principal as u128,
            config.yield_bps,
            elapsed_secs,
            config.compound_interest,
        );
        let total_due = record.principal + total_interest as i128 + record.factoring_fee;

        // Transfer payment
        let token_client = token::Client::new(&env, &record.token);
        token_client.transfer(&payer, &env.current_contract_address(), &total_due);

        // Batch read: get token totals and stats
        let token_totals_key = DataKey::TokenTotals(record.token.clone());
        let mut tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&token_totals_key)
            .unwrap_or_default();

        // Update values
        record.repaid = true;
        tt.total_deployed -= record.principal;
        tt.pool_value += total_interest as i128; // yield added back to pool NAV
        tt.total_fee_revenue += record.factoring_fee;
        tt.total_paid_out += total_due;

        let mut stats: PoolStorageStats = env
            .storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default();
        stats.active_funded_invoices = stats.active_funded_invoices.saturating_sub(1);

        // Batch write: update all storage at once
        env.storage()
            .persistent()
            .set(&funded_invoice_key, &record);
        set_funded_invoice_ttl(&env, invoice_id, true);
        env.storage()
            .instance()
            .set(&token_totals_key, &tt);
        env.storage().instance().set(&DataKey::StorageStats, &stats);

        env.events().publish(
            (EVT, symbol_short!("repaid")),
            (invoice_id, record.principal, total_interest as i128),
        );
    }

    pub fn set_yield(env: Env, admin: Address, yield_bps: u32) {
        admin.require_auth();
        bump_instance(&env);
        let mut config: PoolConfig = env.storage().instance().get(&DataKey::Config).unwrap();
        Self::require_admin(&env, &admin);
        if yield_bps > 5_000 {
            panic!("yield cannot exceed 50%");
        }
        config.yield_bps = yield_bps;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events()
            .publish((EVT, symbol_short!("set_yield")), (admin, yield_bps));
    }

    pub fn set_factoring_fee(env: Env, admin: Address, factoring_fee_bps: u32) {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        let mut config: PoolConfig = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .expect("not initialized");
        Self::require_admin(&env, &admin);
        if factoring_fee_bps > BPS_DENOM {
            panic!("factoring fee cannot exceed 100%");
        }
        config.factoring_fee_bps = factoring_fee_bps;
        env.storage().instance().set(&DataKey::Config, &config);
    }

    pub fn set_compound_interest(env: Env, admin: Address, compound: bool) {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin);
        let mut config: PoolConfig = env.storage().instance().get(&DataKey::Config).unwrap();
        config.compound_interest = compound;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events()
            .publish((EVT, symbol_short!("set_comp")), (admin, compound));
    }

    pub fn get_config(env: Env) -> PoolConfig {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .expect("not initialized")
    }
    pub fn accepted_tokens(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::AcceptedTokens)
            .expect("not initialized")
    }
    pub fn get_token_totals(env: Env, token: Address) -> PoolTokenTotals {
        env.storage()
            .instance()
            .get(&DataKey::TokenTotals(token))
            .unwrap_or_default()
    }
    pub fn get_funded_invoice(env: Env, invoice_id: u64) -> Option<FundedInvoice> {
        env.storage()
            .persistent()
            .get(&DataKey::FundedInvoice(invoice_id))
    }
    pub fn available_liquidity(env: Env, token: Address) -> i128 {
        let tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&DataKey::TokenTotals(token))
            .unwrap_or_default();
        tt.pool_value - tt.total_deployed
    }
    pub fn get_storage_stats(env: Env) -> PoolStorageStats {
        env.storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default()
    }

    pub fn cleanup_funded_invoice(env: Env, admin: Address, invoice_id: u64) {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
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

        let mut stats: PoolStorageStats = env
            .storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default();
        stats.cleaned_invoices += 1;
        env.storage().instance().set(&DataKey::StorageStats, &stats);
        env.events()
            .publish((EVT, symbol_short!("cleanup")), invoice_id);
    }

    pub fn estimate_repayment(env: Env, invoice_id: u64) -> i128 {
        bump_instance(&env);
        let config: PoolConfig = env.storage().instance().get(&DataKey::Config).unwrap();
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
        record.principal + interest as i128 + record.factoring_fee
    }

    fn require_admin(env: &Env, admin: &Address) {
        let config: PoolConfig = env.storage().instance().get(&DataKey::Config).unwrap();
        if admin != &config.admin {
            panic!("unauthorized");
        }
    }

    fn require_not_paused(env: &Env) {
        require_not_paused(env);
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

    // ---- #111: Exchange rate methods ----

    /// Set the USD exchange rate for a token (in bps, e.g. 10000 = 1:1 with USD).
    /// Used to normalise pool value across stablecoins for display/reporting.
    pub fn set_exchange_rate(env: Env, admin: Address, token: Address, rate_bps: u32) {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin);
        Self::assert_accepted_token(&env, &token);
        if rate_bps == 0 {
            panic!("rate must be positive");
        }
        env.storage()
            .instance()
            .set(&DataKey::ExchangeRate(token.clone()), &rate_bps);
        env.events()
            .publish((EVT, symbol_short!("set_rate")), (admin, token, rate_bps));
    }

    /// Returns the USD exchange rate for `token` in bps (defaults to 10000 = 1:1).
    pub fn get_exchange_rate(env: Env, token: Address) -> u32 {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::ExchangeRate(token))
            .unwrap_or(10_000u32)
    }

    // ---- #109: Investor KYC / whitelist methods ----

    /// Toggle whether KYC is required before depositing.
    pub fn set_kyc_required(env: Env, admin: Address, required: bool) {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&DataKey::KycRequired, &required);
        env.events()
            .publish((EVT, symbol_short!("kyc_req")), (admin, required));
    }

    /// Returns whether KYC is currently required.
    pub fn kyc_required(env: Env) -> bool {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::KycRequired)
            .unwrap_or(false)
    }

    /// Approve or revoke a specific investor's KYC status.
    pub fn set_investor_kyc(env: Env, admin: Address, investor: Address, approved: bool) {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::InvestorKyc(investor.clone()), &approved);
        env.events()
            .publish((EVT, symbol_short!("kyc_set")), (admin, investor, approved));
    }

    /// Returns whether `investor` has been KYC-approved.
    pub fn get_investor_kyc(env: Env, investor: Address) -> bool {
        bump_instance(&env);
        env.storage()
            .persistent()
            .get(&DataKey::InvestorKyc(investor))
            .unwrap_or(false)
    }

    pub fn propose_upgrade(env: Env, admin: Address, wasm_hash: BytesN<32>) {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&DataKey::ProposedWasmHash, &wasm_hash);
        env.storage()
            .instance()
            .set(&DataKey::UpgradeScheduledAt, &env.ledger().timestamp());
        env.events().publish(
            (EVT, symbol_short!("upg_prop")),
            (admin, env.ledger().timestamp() + UPGRADE_TIMELOCK_SECS),
        );
    }

    pub fn execute_upgrade(env: Env, admin: Address) {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin);
        let scheduled_at: u64 = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeScheduledAt)
            .expect("no upgrade proposed");
        let now = env.ledger().timestamp();
        if now < scheduled_at + UPGRADE_TIMELOCK_SECS {
            panic!("upgrade timelock not expired");
        }
        let wasm_hash: BytesN<32> = env
            .storage()
            .instance()
            .get(&DataKey::ProposedWasmHash)
            .expect("no wasm hash proposed");
        let wasm_hash_bytes: BytesN<32> = wasm_hash.try_into().unwrap();
        env.deployer().update_current_contract_wasm(wasm_hash_bytes);
        env.events()
            .publish((EVT, symbol_short!("upgraded")), (admin, now));
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Env,
    };

    #[contract]
    pub struct DummyShare;
    #[contractimpl]
    impl DummyShare {
        pub fn total_supply(env: Env) -> i128 {
            env.storage()
                .instance()
                .get(&symbol_short!("tot"))
                .unwrap_or(0)
        }
        pub fn balance(env: Env, id: Address) -> i128 {
            env.storage().persistent().get(&id).unwrap_or(0)
        }
        pub fn mint(env: Env, to: Address, amount: i128) {
            let t = Self::total_supply(env.clone());
            let b = Self::balance(env.clone(), to.clone());
            env.storage()
                .instance()
                .set(&symbol_short!("tot"), &(t + amount));
            env.storage().persistent().set(&to, &(b + amount));
        }
        pub fn burn(env: Env, from: Address, amount: i128) {
            let t = Self::total_supply(env.clone());
            let b = Self::balance(env.clone(), from.clone());
            env.storage()
                .instance()
                .set(&symbol_short!("tot"), &(t - amount));
            env.storage().persistent().set(&from, &(b - amount));
        }
    }

    fn setup(env: &Env) -> (FundingPoolClient<'_>, Address, Address, Address) {
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let contract_id = env.register(FundingPool, ());
        let client = FundingPoolClient::new(env, &contract_id);
        let admin = Address::generate(env);
        let token_admin = Address::generate(env);
        let usdc_id = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let invoice_contract = Address::generate(env);

        let share_token = env.register(DummyShare, ());
        client.initialize(&admin, &usdc_id, &share_token, &invoice_contract);
        (client, admin, usdc_id, share_token)
    }

    fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
        soroban_sdk::token::StellarAssetClient::new(env, token_id).mint(to, &amount);
    }

    #[test]
    fn test_vault_deposit_withdraw() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, share_token) = setup(&env);
        let investor1 = Address::generate(&env);
        let investor2 = Address::generate(&env);

        mint(&env, &usdc_id, &investor1, 1000);
        mint(&env, &usdc_id, &investor2, 1000);

        client.deposit(&investor1, &usdc_id, &1000);

        let shares1: i128 = env.invoke_contract(
            &share_token,
            &Symbol::new(&env, "balance"),
            soroban_sdk::vec![&env, investor1.clone().into_val(&env)],
        );
        assert_eq!(shares1, 1000);

        let tt = client.get_token_totals(&usdc_id);
        assert_eq!(tt.pool_value, 1000);

        client.deposit(&investor2, &usdc_id, &500);

        let shares2: i128 = env.invoke_contract(
            &share_token,
            &Symbol::new(&env, "balance"),
            soroban_sdk::vec![&env, investor2.clone().into_val(&env)],
        );
        assert_eq!(shares2, 500);

        client.withdraw(&investor1, &usdc_id, &1000);
        let bal = soroban_sdk::token::Client::new(&env, &usdc_id).balance(&investor1);
        assert_eq!(bal, 1000);
    }

    #[test]
    fn test_yield_accumulation() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 10000);
        mint(&env, &usdc_id, &sme, 10000);

        client.deposit(&investor, &usdc_id, &10000);
        client.fund_invoice(
            &admin,
            &1u64,
            &5000i128,
            &sme,
            &(env.ledger().timestamp() + 50000),
            &usdc_id,
        );

        env.ledger().with_mut(|l| l.timestamp += 100_000); // 100k secs
        client.repay_invoice(&1u64, &sme);

        // Wait, 5000 principal at 8% APY for 100k secs.
        let tt = client.get_token_totals(&usdc_id);
        assert!(tt.pool_value > 10000);

        // When investor withdraws their 10000 shares, they should get > 10000 underlying!
        client.withdraw(&investor, &usdc_id, &10000);
        let bal = soroban_sdk::token::Client::new(&env, &usdc_id).balance(&investor);
        assert_eq!(bal, tt.pool_value); // Investor got everything because they owned 100% shares
    }

    #[test]
    fn test_factoring_fee_is_charged_and_tracked_separately() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        let principal: i128 = 1_000_000_000;
        mint(&env, &usdc_id, &investor, principal);
        // sme needs to repay principal + interest + fee
        mint(&env, &usdc_id, &sme, principal * 2);

        // Set factoring fee to 2.5%
        client.set_factoring_fee(&admin, &250);
        client.deposit(&investor, &usdc_id, &principal);
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 30 * 86_400),
            &usdc_id,
        );

        let funded = client.get_funded_invoice(&1u64).unwrap();
        let expected_fee = principal * 250 / BPS_DENOM as i128;
        assert_eq!(funded.factoring_fee, expected_fee);

        env.ledger().with_mut(|l| l.timestamp += 30 * 86_400);

        let expected_interest =
            (principal as u128 * DEFAULT_YIELD_BPS as u128 * (30 * 86_400) as u128)
                / (BPS_DENOM as u128 * SECS_PER_YEAR as u128);
        let expected_total_due = principal + expected_interest as i128 + expected_fee;

        assert_eq!(client.estimate_repayment(&1u64), expected_total_due);

        client.repay_invoice(&1u64, &sme);

        let tt = client.get_token_totals(&usdc_id);
        assert_eq!(tt.total_fee_revenue, expected_fee);
        assert_eq!(tt.total_paid_out, expected_total_due);
        // pool_value grew by the yield
        assert!(tt.pool_value >= principal);
    }

    // ---- Issue #61: Edge-Case Tests ----

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_deposit_zero_amount_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        client.deposit(&investor, &usdc_id, &0i128);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_deposit_negative_amount_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        client.deposit(&investor, &usdc_id, &-100i128);
    }

    #[test]
    #[should_panic(expected = "token not accepted")]
    fn test_deposit_non_whitelisted_token_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let unknown_token = Address::generate(&env);
        client.deposit(&investor, &unknown_token, &1_000i128);
    }

    #[test]
    #[should_panic(expected = "shares must be positive")]
    fn test_withdraw_zero_shares_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 1_000);
        client.deposit(&investor, &usdc_id, &1_000);
        client.withdraw(&investor, &usdc_id, &0i128);
    }

    #[test]
    #[should_panic(expected = "insufficient shares")]
    fn test_withdraw_more_than_balance_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 500);
        client.deposit(&investor, &usdc_id, &500);
        // Attempt to withdraw more shares than owned
        client.withdraw(&investor, &usdc_id, &1_000i128);
    }

    #[test]
    #[should_panic(expected = "principal must be positive")]
    fn test_fund_invoice_zero_principal_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let sme = Address::generate(&env);
        client.fund_invoice(
            &admin,
            &1u64,
            &0i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
    }

    #[test]
    #[should_panic(expected = "insufficient available liquidity")]
    fn test_fund_invoice_insufficient_liquidity_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 500);
        client.deposit(&investor, &usdc_id, &500);
        // Try to fund more than available in pool
        client.fund_invoice(
            &admin,
            &1u64,
            &1_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
    }

    #[test]
    #[should_panic(expected = "invoice already funded")]
    fn test_fund_invoice_duplicate_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 2_000);
        client.deposit(&investor, &usdc_id, &2_000);
        client.fund_invoice(
            &admin,
            &1u64,
            &500i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        // Second fund on same invoice_id must panic
        client.fund_invoice(
            &admin,
            &1u64,
            &500i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
    }

    #[test]
    #[should_panic(expected = "already repaid")]
    fn test_double_repay_invoice_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 1_000);
        mint(&env, &usdc_id, &sme, 2_000);
        client.deposit(&investor, &usdc_id, &1_000);
        client.fund_invoice(
            &admin,
            &1u64,
            &1_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        client.repay_invoice(&1u64, &sme);
        // Second repay must panic
        client.repay_invoice(&1u64, &sme);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_fund_invoice_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let sme = Address::generate(&env);
        let attacker = Address::generate(&env);
        client.fund_invoice(
            &attacker,
            &1u64,
            &100i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
    }

    #[test]
    #[should_panic(expected = "yield cannot exceed 50%")]
    fn test_set_yield_above_50_percent_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        client.set_yield(&admin, &5_001u32);
    }

    #[test]
    fn test_set_yield_at_boundary_50_percent() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        client.set_yield(&admin, &5_000u32);
        assert_eq!(client.get_config().yield_bps, 5_000);
    }

    #[test]
    fn test_add_token_and_remove_unused() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let token_admin2 = Address::generate(&env);
        let new_token = env.register_stellar_asset_contract_v2(token_admin2).address();
        let new_share = env.register(DummyShare, ());
        client.add_token(&admin, &new_token, &new_share);
        let tokens = client.accepted_tokens();
        assert_eq!(tokens.len(), 2);
        client.remove_token(&admin, &new_token);
        let tokens = client.accepted_tokens();
        assert_eq!(tokens.len(), 1);
    }

    #[test]
    #[should_panic(expected = "token has non-zero pool balances")]
    fn test_remove_token_with_balance_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 1_000);
        client.deposit(&investor, &usdc_id, &1_000);
        // pool has a non-zero balance — remove must panic
        client.remove_token(&admin, &usdc_id);
    }
}
