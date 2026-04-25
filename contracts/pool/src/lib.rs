#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, BytesN, Env, IntoVal,
    Symbol, Vec,
};

const DEFAULT_YIELD_BPS: u32 = 800;
const DEFAULT_FACTORING_FEE_BPS: u32 = 0;
const BPS_DENOM: u32 = 10_000;
const SECS_PER_YEAR: u64 = 31_536_000;
/// Default collateral threshold: invoices >= 10,000 USDC (7 decimals) require collateral.
const DEFAULT_COLLATERAL_THRESHOLD: i128 = 100_000_000_000; // 10,000 USDC
/// Default collateral ratio: 20% of principal (2000 bps).
const DEFAULT_COLLATERAL_BPS: u32 = 2_000;
const DEFAULT_YIELD_CHANGE_COOLDOWN_SECS: u64 = 86_400; // 24 hours
const DEFAULT_MAX_YIELD_CHANGE_BPS: u32 = 200; // +/- 200 bps per adjustment

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
    pub last_yield_change_at: u64,
    pub yield_change_cooldown_secs: u64,
    pub max_yield_change_bps: u32,
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
pub struct ExchangeRateBounds {
    pub min_bps: u32,
    pub max_bps: u32,
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
    /// Total amount repaid so far (supports partial repayments)
    pub repaid_amount: i128,
}

#[contracttype]
#[derive(Clone)]
pub struct FundingRequest {
    pub invoice_id: u64,
    pub principal: i128,
    pub sme: Address,
    pub due_date: u64,
    pub token: Address,
}

#[contracttype]
#[derive(Clone, Default)]
pub struct PoolStorageStats {
    pub total_funded_invoices: u64,
    pub active_funded_invoices: u64,
    pub cleaned_invoices: u64,
}

/// Collateral configuration: threshold above which collateral is required,
/// and the required ratio expressed in basis points of the principal.
#[contracttype]
#[derive(Clone)]
pub struct CollateralConfig {
    /// Minimum principal amount (inclusive) that triggers the collateral requirement.
    /// Invoices with principal >= this value must have collateral deposited before funding.
    pub threshold: i128,
    /// Required collateral as a fraction of principal, in basis points (e.g. 2000 = 20%).
    pub collateral_bps: u32,
}

/// Record of collateral deposited for a specific invoice.
#[contracttype]
#[derive(Clone)]
pub struct CollateralDeposit {
    /// The invoice this collateral secures.
    pub invoice_id: u64,
    /// Address that deposited the collateral (typically the SME).
    pub depositor: Address,
    /// Stablecoin token used for collateral.
    pub token: Address,
    /// Amount of collateral locked.
    pub amount: i128,
    /// Whether the collateral has been settled (returned or seized).
    pub settled: bool,
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
    ExchangeRateBounds(Address),
    // #109: KYC / investor whitelist
    KycRequired,
    InvestorKyc(Address),
    // Collateral: threshold config and per-invoice deposits
    CollateralConfig,
    CollateralDeposit(u64),
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
        // Use checked intermediates so very large principals or durations fail
        // predictably instead of overflowing before the final division.
        let numerator = principal
            .checked_mul(yield_bps as u128)
            .and_then(|value| value.checked_mul(elapsed_secs as u128))
            .expect("interest calculation overflow");
        return numerator / denominator;
    }
    let elapsed_days = elapsed_secs / 86400;
    let mut amount = principal;
    let daily_rate_num = yield_bps as u128 * 86400;
    for _ in 0..elapsed_days {
        let accrued = amount
            .checked_mul(daily_rate_num)
            .expect("interest calculation overflow")
            / denominator;
        amount = amount
            .checked_add(accrued)
            .expect("interest calculation overflow");
    }
    let remaining_secs = elapsed_secs % 86400;
    if remaining_secs > 0 {
        let accrued = amount
            .checked_mul(yield_bps as u128)
            .and_then(|value| value.checked_mul(remaining_secs as u128))
            .expect("interest calculation overflow")
            / denominator;
        amount = amount
            .checked_add(accrued)
            .expect("interest calculation overflow");
    }
    amount - principal
}

fn calculate_factoring_fee(principal: i128, factoring_fee_bps: u32) -> i128 {
    ((principal as u128 * factoring_fee_bps as u128) / BPS_DENOM as u128) as i128
}

/// Returns the required collateral amount for `principal` given the pool's collateral config.
/// Returns 0 if the principal is below the threshold (no collateral required).
fn required_collateral(principal: i128, config: &CollateralConfig) -> i128 {
    if principal < config.threshold {
        return 0;
    }
    ((principal as u128 * config.collateral_bps as u128) / BPS_DENOM as u128) as i128
}

fn fund_invoice_request(
    env: &Env,
    config: &PoolConfig,
    accepted_tokens: &Vec<Address>,
    stats: &mut PoolStorageStats,
    request: &FundingRequest,
) {
    if request.principal <= 0 {
        panic!("principal must be positive");
    }

    // Verify the token is accepted.
    let mut token_ok = false;
    for i in 0..accepted_tokens.len() {
        if accepted_tokens.get(i).unwrap() == request.token {
            token_ok = true;
            break;
        }
    }
    if !token_ok {
        panic!("token not accepted");
    }

    // Ensure sufficient liquidity (cash = NAV - deployed).
    let token_totals_key = DataKey::TokenTotals(request.token.clone());
    let mut tt: PoolTokenTotals = env
        .storage()
        .instance()
        .get(&token_totals_key)
        .unwrap_or_default();
    let available_liquidity = tt.pool_value - tt.total_deployed;
    if available_liquidity < request.principal {
        panic!("insufficient available liquidity");
    }

    let now = env.ledger().timestamp();
    let factoring_fee = calculate_factoring_fee(request.principal, config.factoring_fee_bps);
    let funded = FundedInvoice {
        invoice_id: request.invoice_id,
        sme: request.sme.clone(),
        token: request.token.clone(),
        principal: request.principal,
        funded_at: now,
        factoring_fee,
        due_date: request.due_date,
        repaid_amount: 0i128,
    };

    // Transfer principal to SME; NAV is unchanged because the funded invoice becomes an asset.
    let token_client = token::Client::new(env, &request.token);
    token_client.transfer(
        &env.current_contract_address(),
        &request.sme,
        &request.principal,
    );

    // Persist invoice record and update totals/stats.
    env.storage()
        .persistent()
        .set(&DataKey::FundedInvoice(request.invoice_id), &funded);
    set_funded_invoice_ttl(env, request.invoice_id, false);

    tt.total_deployed += request.principal;
    env.storage().instance().set(&token_totals_key, &tt);

    stats.total_funded_invoices += 1;
    stats.active_funded_invoices += 1;

    env.events().publish(
        (EVT, symbol_short!("funded")),
        (
            request.invoice_id,
            request.sme.clone(),
            request.principal,
            request.token.clone(),
        ),
    );
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
            last_yield_change_at: env.ledger().timestamp(),
            yield_change_cooldown_secs: DEFAULT_YIELD_CHANGE_COOLDOWN_SECS,
            max_yield_change_bps: DEFAULT_MAX_YIELD_CHANGE_BPS,
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
        env.storage().instance().set(
            &DataKey::CollateralConfig,
            &CollateralConfig {
                threshold: DEFAULT_COLLATERAL_THRESHOLD,
                collateral_bps: DEFAULT_COLLATERAL_BPS,
            },
        );
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

        let share_token: Address = env.storage().instance().get(&share_token_key).unwrap();

        // Calculate shares (single external call)
        let total_shares: i128 = env.invoke_contract(
            &share_token,
            &Symbol::new(&env, "total_supply"),
            Vec::new(&env),
        );

        let shares_to_mint = if total_shares == 0 || tt.pool_value == 0 {
            amount
        } else {
            (amount * total_shares) / tt.pool_value
        };

        // Update pool value
        tt.pool_value += amount;

        // Batch write: update token totals
        env.storage().instance().set(&token_totals_key, &tt);

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

        let share_token: Address = env.storage().instance().get(&share_token_key).unwrap();

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
        let amount = (shares * tt.pool_value) / total_shares;
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
        env.storage().instance().set(&token_totals_key, &tt);

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
        let config = get_config_cached(&env);
        if env
            .storage()
            .persistent()
            .has(&DataKey::FundedInvoice(invoice_id))
        {
            panic!("invoice already funded");
        }
        let accepted_tokens: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AcceptedTokens)
            .expect("not initialized");

        // Collateral check: high-value invoices must have collateral deposited first.
        let collateral_cfg: CollateralConfig = env
            .storage()
            .instance()
            .get(&DataKey::CollateralConfig)
            .unwrap_or(CollateralConfig {
                threshold: DEFAULT_COLLATERAL_THRESHOLD,
                collateral_bps: DEFAULT_COLLATERAL_BPS,
            });
        let req_collateral = required_collateral(principal, &collateral_cfg);
        if req_collateral > 0 {
            let deposit: Option<CollateralDeposit> = env
                .storage()
                .persistent()
                .get(&DataKey::CollateralDeposit(invoice_id));
            match deposit {
                None => panic!("collateral required for high-value invoice"),
                Some(d) => {
                    if d.settled {
                        panic!("collateral already settled");
                    }
                    if d.amount < req_collateral {
                        panic!("insufficient collateral deposited");
                    }
                }
            }
        }

        let mut stats: PoolStorageStats = env
            .storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default();
        let request = FundingRequest {
            invoice_id,
            principal,
            sme,
            due_date,
            token,
        };
        fund_invoice_request(&env, &config, &accepted_tokens, &mut stats, &request);
        env.storage().instance().set(&DataKey::StorageStats, &stats);
    }

    pub fn fund_multiple_invoices(env: Env, admin: Address, requests: Vec<FundingRequest>) {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin);
        if requests.len() == 0 {
            panic!("no invoices provided");
        }

        let config = get_config_cached(&env);
        let accepted_tokens: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AcceptedTokens)
            .expect("not initialized");
        let mut stats: PoolStorageStats = env
            .storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default();

        for i in 0..requests.len() {
            let request = requests.get(i).unwrap();
            fund_invoice_request(&env, &config, &accepted_tokens, &mut stats, &request);
        }

        env.storage().instance().set(&DataKey::StorageStats, &stats);
    }

    pub fn repay_invoice(env: Env, invoice_id: u64, payer: Address, amount: i128) {
        payer.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);

        if amount <= 0 {
            panic!("payment amount must be positive");
        }

        // Batch read: get config and funded invoice record
        let config: PoolConfig = get_config_cached(&env);
        let funded_invoice_key = DataKey::FundedInvoice(invoice_id);
        let mut record: FundedInvoice = env
            .storage()
            .persistent()
            .get(&funded_invoice_key)
            .expect("invoice not found");

        // Calculate total due (principal + interest + factoring fee)
        let now = env.ledger().timestamp();
        let elapsed_secs = now - record.funded_at;
        let total_interest = calculate_interest(
            record.principal as u128,
            config.yield_bps,
            elapsed_secs,
            config.compound_interest,
        );
        let total_due = record.principal + total_interest as i128 + record.factoring_fee;

        // Check if already fully repaid
        if record.repaid_amount >= total_due {
            panic!("invoice already fully repaid");
        }

        // Check if payment would exceed total due
        if record.repaid_amount + amount > total_due {
            panic!("payment exceeds total due");
        }

        // Transfer payment
        let token_client = token::Client::new(&env, &record.token);
        token_client.transfer(&payer, &env.current_contract_address(), &amount);

        // Update repaid amount
        record.repaid_amount += amount;

        let fully_repaid = record.repaid_amount >= total_due;

        // Batch read: get token totals and stats
        let token_totals_key = DataKey::TokenTotals(record.token.clone());
        let mut tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&token_totals_key)
            .unwrap_or_default();

        let mut stats: PoolStorageStats = env
            .storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default();

        // Only update pool totals when fully repaid
        if fully_repaid {
            tt.total_deployed -= record.principal;
            tt.pool_value += total_interest as i128; // yield added back to pool NAV
            tt.total_fee_revenue += record.factoring_fee;
            tt.total_paid_out += total_due;
            stats.active_funded_invoices = stats.active_funded_invoices.saturating_sub(1);
        }

        // Batch write: update all storage at once
        env.storage().persistent().set(&funded_invoice_key, &record);
        if fully_repaid {
            set_funded_invoice_ttl(&env, invoice_id, true);
        }
        env.storage().instance().set(&token_totals_key, &tt);
        env.storage().instance().set(&DataKey::StorageStats, &stats);

        if fully_repaid {
            env.events().publish(
                (EVT, symbol_short!("repaid")),
                (invoice_id, record.principal, total_interest as i128),
            );
        } else {
            // Emit partial repayment event
            env.events().publish(
                (EVT, symbol_short!("part_pay")),
                (invoice_id, amount, record.repaid_amount),
            );
        }

        // Release collateral back to depositor if any was locked for this invoice (only when fully repaid)
        if fully_repaid {
            if let Some(mut col) = env
                .storage()
                .persistent()
                .get::<DataKey, CollateralDeposit>(&DataKey::CollateralDeposit(invoice_id))
            {
                if !col.settled {
                    let col_token_client = token::Client::new(&env, &col.token);
                    col_token_client.transfer(
                        &env.current_contract_address(),
                        &col.depositor,
                        &col.amount,
                    );
                    col.settled = true;
                    env.storage()
                        .persistent()
                        .set(&DataKey::CollateralDeposit(invoice_id), &col);
                    env.events().publish(
                        (EVT, symbol_short!("col_ret")),
                        (invoice_id, col.depositor, col.amount),
                    );
                }
            }
        }
    }

    // ---- Collateral management ----

    /// Admin sets the collateral configuration.
    /// `threshold` — minimum principal (inclusive) that requires collateral.
    /// `collateral_bps` — required collateral as % of principal in basis points (max 10000 = 100%).
    pub fn set_collateral_config(env: Env, admin: Address, threshold: i128, collateral_bps: u32) {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin);
        if threshold < 0 {
            panic!("threshold must be non-negative");
        }
        if collateral_bps > BPS_DENOM {
            panic!("collateral ratio cannot exceed 100%");
        }
        let cfg = CollateralConfig {
            threshold,
            collateral_bps,
        };
        env.storage()
            .instance()
            .set(&DataKey::CollateralConfig, &cfg);
        env.events().publish(
            (EVT, symbol_short!("col_cfg")),
            (admin, threshold, collateral_bps),
        );
    }

    /// Returns the current collateral configuration.
    pub fn get_collateral_config(env: Env) -> CollateralConfig {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::CollateralConfig)
            .unwrap_or(CollateralConfig {
                threshold: DEFAULT_COLLATERAL_THRESHOLD,
                collateral_bps: DEFAULT_COLLATERAL_BPS,
            })
    }

    /// Returns the required collateral amount for a given principal under current config.
    /// Returns 0 if no collateral is required.
    pub fn required_collateral_for(env: Env, principal: i128) -> i128 {
        bump_instance(&env);
        let cfg: CollateralConfig = env
            .storage()
            .instance()
            .get(&DataKey::CollateralConfig)
            .unwrap_or(CollateralConfig {
                threshold: DEFAULT_COLLATERAL_THRESHOLD,
                collateral_bps: DEFAULT_COLLATERAL_BPS,
            });
        required_collateral(principal, &cfg)
    }

    /// SME (or any party) deposits collateral for a high-value invoice before it can be funded.
    /// The collateral is held by the pool contract until the invoice is repaid (returned)
    /// or defaulted (seized to protect investors).
    pub fn deposit_collateral(
        env: Env,
        invoice_id: u64,
        depositor: Address,
        token: Address,
        amount: i128,
    ) {
        depositor.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::assert_accepted_token(&env, &token);

        if amount <= 0 {
            panic!("collateral amount must be positive");
        }

        // Prevent depositing collateral for an already-funded invoice.
        if env
            .storage()
            .persistent()
            .has(&DataKey::FundedInvoice(invoice_id))
        {
            panic!("invoice already funded; collateral cannot be changed");
        }

        // Prevent double-deposit.
        if env
            .storage()
            .persistent()
            .has(&DataKey::CollateralDeposit(invoice_id))
        {
            panic!("collateral already deposited for this invoice");
        }

        // Transfer collateral from depositor to pool.
        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&depositor, &env.current_contract_address(), &amount);

        let record = CollateralDeposit {
            invoice_id,
            depositor: depositor.clone(),
            token: token.clone(),
            amount,
            settled: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::CollateralDeposit(invoice_id), &record);
        // Use active invoice TTL — collateral lives as long as the invoice.
        env.storage().persistent().extend_ttl(
            &DataKey::CollateralDeposit(invoice_id),
            ACTIVE_INVOICE_TTL,
            ACTIVE_INVOICE_TTL,
        );

        env.events().publish(
            (EVT, symbol_short!("col_dep")),
            (invoice_id, depositor, token, amount),
        );
    }

    /// Returns the collateral deposit record for an invoice, if any.
    pub fn get_collateral_deposit(env: Env, invoice_id: u64) -> Option<CollateralDeposit> {
        bump_instance(&env);
        env.storage()
            .persistent()
            .get(&DataKey::CollateralDeposit(invoice_id))
    }

    /// Admin seizes collateral for a defaulted invoice, transferring it to the pool
    /// to partially compensate investors for the loss.
    /// Can only be called after the invoice has been marked as defaulted (repaid == false
    /// and the invoice is past due + grace period).
    pub fn seize_collateral(env: Env, admin: Address, invoice_id: u64) {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin);

        let record: FundedInvoice = env
            .storage()
            .persistent()
            .get(&DataKey::FundedInvoice(invoice_id))
            .expect("funded invoice not found");

        // Calculate total due to check if fully repaid
        let config: PoolConfig = env.storage().instance().get(&DataKey::Config).unwrap();
        let now = env.ledger().timestamp();
        let elapsed_secs = now - record.funded_at;
        let total_interest = calculate_interest(
            record.principal as u128,
            config.yield_bps,
            elapsed_secs,
            config.compound_interest,
        );
        let total_due = record.principal + total_interest as i128 + record.factoring_fee;

        if record.repaid_amount >= total_due {
            panic!("invoice already repaid; collateral was returned on repayment");
        }

        let mut col: CollateralDeposit = env
            .storage()
            .persistent()
            .get(&DataKey::CollateralDeposit(invoice_id))
            .expect("no collateral deposit found for this invoice");

        if col.settled {
            panic!("collateral already settled");
        }

        // Credit the seized collateral into the pool's token totals so investors benefit.
        let token_totals_key = DataKey::TokenTotals(col.token.clone());
        let mut tt: PoolTokenTotals = env
            .storage()
            .instance()
            .get(&token_totals_key)
            .unwrap_or_default();

        // The seized collateral reduces the effective loss: add it to pool_value and
        // reduce total_deployed by the original principal (the invoice is now a loss).
        tt.pool_value += col.amount;
        tt.total_deployed -= record.principal;
        env.storage().instance().set(&token_totals_key, &tt);

        col.settled = true;
        env.storage()
            .persistent()
            .set(&DataKey::CollateralDeposit(invoice_id), &col);
        env.storage().persistent().extend_ttl(
            &DataKey::CollateralDeposit(invoice_id),
            COMPLETED_INVOICE_TTL,
            COMPLETED_INVOICE_TTL,
        );

        env.events().publish(
            (EVT, symbol_short!("col_seiz")),
            (invoice_id, col.depositor, col.amount),
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

        let now = env.ledger().timestamp();
        let next_allowed = config
            .last_yield_change_at
            .saturating_add(config.yield_change_cooldown_secs);
        if now < next_allowed {
            panic!("yield change cooldown active");
        }

        let current = config.yield_bps;
        let delta = if yield_bps >= current {
            yield_bps - current
        } else {
            current - yield_bps
        };
        if delta > config.max_yield_change_bps {
            panic!("yield change exceeds maximum step");
        }

        config.yield_bps = yield_bps;
        config.last_yield_change_at = now;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events()
            .publish((EVT, symbol_short!("set_yield")), (admin, yield_bps));
    }

    pub fn set_yield_change_policy(
        env: Env,
        admin: Address,
        cooldown_secs: u64,
        max_change_bps: u32,
    ) {
        admin.require_auth();
        bump_instance(&env);
        Self::require_not_paused(&env);
        let mut config: PoolConfig = env.storage().instance().get(&DataKey::Config).unwrap();
        Self::require_admin(&env, &admin);
        if cooldown_secs == 0 {
            panic!("cooldown must be non-zero");
        }
        if max_change_bps == 0 {
            panic!("max change must be non-zero");
        }
        config.yield_change_cooldown_secs = cooldown_secs;
        config.max_yield_change_bps = max_change_bps;
        env.storage().instance().set(&DataKey::Config, &config);
        env.events().publish(
            (EVT, symbol_short!("set_y_pol")),
            (admin, cooldown_secs, max_change_bps),
        );
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

        // Calculate total due to check if fully repaid
        let config: PoolConfig = env.storage().instance().get(&DataKey::Config).unwrap();
        let now = env.ledger().timestamp();
        let elapsed_secs = now - record.funded_at;
        let total_interest = calculate_interest(
            record.principal as u128,
            config.yield_bps,
            elapsed_secs,
            config.compound_interest,
        );
        let total_due = record.principal + total_interest as i128 + record.factoring_fee;

        if record.repaid_amount < total_due {
            panic!("can only cleanup fully repaid invoices");
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
        let total_due = record.principal + interest as i128 + record.factoring_fee;
        // Return remaining amount due (total - already repaid)
        let remaining = total_due - record.repaid_amount;
        if remaining < 0 {
            0
        } else {
            remaining
        }
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
    /// Oracle-backed validation is a planned follow-up; for now the admin must
    /// set explicit per-token bounds before changing a rate.
    pub fn set_rate_bounds(
        env: Env,
        admin: Address,
        token: Address,
        min_bps: u32,
        max_bps: u32,
    ) {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin);
        Self::assert_accepted_token(&env, &token);
        if min_bps == 0 || max_bps == 0 {
            panic!("rate bounds must be positive");
        }
        if min_bps > max_bps {
            panic!("invalid rate bounds");
        }

        env.storage().instance().set(
            &DataKey::ExchangeRateBounds(token.clone()),
            &ExchangeRateBounds { min_bps, max_bps },
        );
        env.events().publish(
            (EVT, symbol_short!("bounds")),
            (admin, token, min_bps, max_bps),
        );
    }

    pub fn set_exchange_rate(env: Env, admin: Address, token: Address, rate_bps: u32) {
        admin.require_auth();
        bump_instance(&env);
        Self::require_admin(&env, &admin);
        Self::assert_accepted_token(&env, &token);
        if rate_bps == 0 {
            panic!("rate must be positive");
        }
        let bounds: ExchangeRateBounds = env
            .storage()
            .instance()
            .get(&DataKey::ExchangeRateBounds(token.clone()))
            .unwrap_or(ExchangeRateBounds {
                min_bps: 10_000u32,
                max_bps: 10_000u32,
            });
        if rate_bps < bounds.min_bps || rate_bps > bounds.max_bps {
            panic!("rate out of bounds");
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

    pub fn get_rate_bounds(env: Env, token: Address) -> ExchangeRateBounds {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::ExchangeRateBounds(token))
            .unwrap_or(ExchangeRateBounds {
                min_bps: 10_000u32,
                max_bps: 10_000u32,
            })
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
        env.deployer().update_current_contract_wasm(wasm_hash);
        env.events()
            .publish((EVT, symbol_short!("upgraded")), (admin, now));
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        BytesN, Env,
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
        let amount_due = client.estimate_repayment(&1u64);
        client.repay_invoice(&1u64, &sme, &amount_due);

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

        client.repay_invoice(&1u64, &sme, &expected_total_due);

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
        let amount_due = client.estimate_repayment(&1u64);
        client.repay_invoice(&1u64, &sme, &amount_due);
        // Second repay must panic
        client.repay_invoice(&1u64, &sme, &amount_due);
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
        // Allow a large one-time step so we can test the 50% ceiling independently.
        client.set_yield_change_policy(&admin, &1u64, &5_000u32);
        env.ledger().with_mut(|l| l.timestamp += 1);
        client.set_yield(&admin, &5_000u32);
        assert_eq!(client.get_config().yield_bps, 5_000);
    }

    #[test]
    #[should_panic(expected = "yield change cooldown active")]
    fn test_set_yield_cooldown_enforced() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);

        // setup() sets timestamp; first change must wait out cooldown
        env.ledger()
            .with_mut(|l| l.timestamp += DEFAULT_YIELD_CHANGE_COOLDOWN_SECS);
        client.set_yield(&admin, &900u32);

        // immediate second change should fail
        client.set_yield(&admin, &950u32);
    }

    #[test]
    #[should_panic(expected = "yield change exceeds maximum step")]
    fn test_set_yield_max_step_enforced() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);

        env.ledger()
            .with_mut(|l| l.timestamp += DEFAULT_YIELD_CHANGE_COOLDOWN_SECS);
        // DEFAULT_YIELD_BPS = 800, max step = 200 => delta 301 should fail
        client.set_yield(&admin, &1_101u32);
    }

    #[test]
    fn test_add_token_and_remove_unused() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let token_admin2 = Address::generate(&env);
        let new_token = env
            .register_stellar_asset_contract_v2(token_admin2)
            .address();
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

    // ---- Collateral Tests ----

    #[test]
    fn test_default_collateral_config() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let cfg = client.get_collateral_config();
        assert_eq!(cfg.threshold, DEFAULT_COLLATERAL_THRESHOLD);
        assert_eq!(cfg.collateral_bps, DEFAULT_COLLATERAL_BPS);
    }

    #[test]
    fn test_set_collateral_config() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        // Set threshold to 5000 USDC, 10% collateral
        client.set_collateral_config(&admin, &50_000_000_000i128, &1_000u32);
        let cfg = client.get_collateral_config();
        assert_eq!(cfg.threshold, 50_000_000_000i128);
        assert_eq!(cfg.collateral_bps, 1_000u32);
    }

    #[test]
    #[should_panic(expected = "collateral ratio cannot exceed 100%")]
    fn test_set_collateral_config_over_100_percent_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        client.set_collateral_config(&admin, &1_000i128, &10_001u32);
    }

    #[test]
    fn test_required_collateral_below_threshold_is_zero() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        // Default threshold is 100_000_000_000 (10,000 USDC); 1000 USDC is below it
        let req = client.required_collateral_for(&1_000_000_000i128);
        assert_eq!(req, 0);
    }

    #[test]
    fn test_required_collateral_above_threshold() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        // Lower threshold to 500 USDC, 20% collateral
        client.set_collateral_config(&admin, &5_000_000_000i128, &2_000u32);
        // 1000 USDC principal → 200 USDC collateral
        let req = client.required_collateral_for(&10_000_000_000i128);
        assert_eq!(req, 2_000_000_000i128); // 20% of 10,000 USDC
    }

    #[test]
    fn test_low_value_invoice_funded_without_collateral() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 5_000);
        mint(&env, &usdc_id, &sme, 5_000);
        client.deposit(&investor, &usdc_id, &5_000);

        // Principal (5000) is well below default threshold (100_000_000_000)
        // so no collateral needed
        client.fund_invoice(
            &admin,
            &1u64,
            &5_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        let fi = client.get_funded_invoice(&1u64).unwrap();
        assert!(!fi.repaid);
    }

    #[test]
    #[should_panic(expected = "collateral required for high-value invoice")]
    fn test_high_value_invoice_requires_collateral() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        // Lower threshold so our test amounts trigger it
        client.set_collateral_config(&admin, &1_000i128, &2_000u32);

        mint(&env, &usdc_id, &investor, 10_000);
        client.deposit(&investor, &usdc_id, &10_000);

        // Try to fund without depositing collateral first — must panic
        client.fund_invoice(
            &admin,
            &1u64,
            &5_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
    }

    #[test]
    fn test_deposit_collateral_and_fund_high_value_invoice() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        // Threshold = 1000, 20% collateral
        client.set_collateral_config(&admin, &1_000i128, &2_000u32);

        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal); // 1000
        assert_eq!(required, 1_000);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, required);

        client.deposit(&investor, &usdc_id, &10_000);

        // SME deposits collateral
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);

        let col = client.get_collateral_deposit(&1u64).unwrap();
        assert_eq!(col.amount, required);
        assert!(!col.settled);

        // Now funding should succeed
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        let fi = client.get_funded_invoice(&1u64).unwrap();
        assert!(!fi.repaid);
    }

    #[test]
    fn test_collateral_returned_on_repayment() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        client.set_collateral_config(&admin, &1_000i128, &2_000u32);

        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, principal * 2 + required);

        client.deposit(&investor, &usdc_id, &10_000);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );

        let sme_balance_before = token::Client::new(&env, &usdc_id).balance(&sme);

        let amount_due = client.estimate_repayment(&1u64);
        client.repay_invoice(&1u64, &sme, &amount_due);

        let sme_balance_after = token::Client::new(&env, &usdc_id).balance(&sme);
        // SME should have gotten collateral back (minus repayment cost)
        // sme_balance_after = sme_balance_before - total_due + collateral_returned
        let col = client.get_collateral_deposit(&1u64).unwrap();
        assert!(col.settled);
        // Net: sme paid total_due but got collateral back
        assert!(sme_balance_after > sme_balance_before - principal);
    }

    #[test]
    fn test_seize_collateral_on_default() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        client.set_collateral_config(&admin, &1_000i128, &2_000u32);

        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, required);

        client.deposit(&investor, &usdc_id, &10_000);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);

        let due_date = env.ledger().timestamp() + 10_000;
        client.fund_invoice(&admin, &1u64, &principal, &sme, &due_date, &usdc_id);

        // Advance past due date (no repayment)
        env.ledger().with_mut(|l| l.timestamp = due_date + 1);

        let tt_before = client.get_token_totals(&usdc_id);

        // Admin seizes collateral
        client.seize_collateral(&admin, &1u64);

        let col = client.get_collateral_deposit(&1u64).unwrap();
        assert!(col.settled);

        // Pool value should have increased by collateral amount, deployed reduced
        let tt_after = client.get_token_totals(&usdc_id);
        assert_eq!(tt_after.pool_value, tt_before.pool_value + required);
        assert_eq!(
            tt_after.total_deployed,
            tt_before.total_deployed - principal
        );
    }

    #[test]
    #[should_panic(expected = "collateral already deposited for this invoice")]
    fn test_double_deposit_collateral_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let sme = Address::generate(&env);

        client.set_collateral_config(&admin, &1_000i128, &2_000u32);
        mint(&env, &usdc_id, &sme, 5_000);

        client.deposit_collateral(&1u64, &sme, &usdc_id, &1_000);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &1_000);
    }

    #[test]
    #[should_panic(expected = "insufficient collateral deposited")]
    fn test_insufficient_collateral_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        // 20% collateral required on anything >= 1000
        client.set_collateral_config(&admin, &1_000i128, &2_000u32);

        let principal: i128 = 5_000;
        // Required = 1000, but we only deposit 500
        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, 500);

        client.deposit(&investor, &usdc_id, &10_000);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &500);

        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
    }

    #[test]
    #[should_panic(expected = "invoice already repaid; collateral was returned on repayment")]
    fn test_seize_collateral_after_repayment_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        client.set_collateral_config(&admin, &1_000i128, &2_000u32);
        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal);

        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, principal * 2 + required);

        client.deposit(&investor, &usdc_id, &10_000);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        let amount_due = client.estimate_repayment(&1u64);
        client.repay_invoice(&1u64, &sme, &amount_due);

        // Trying to seize after repayment must panic
        client.seize_collateral(&admin, &1u64);
    }

    // ---- Issue #105: Comprehensive Access Control Tests ----

    // --- Admin-gated function guards ---

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_pause_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let attacker = Address::generate(&env);
        client.pause(&attacker);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_unpause_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        client.pause(&admin);
        let attacker = Address::generate(&env);
        client.unpause(&attacker);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_add_token_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let attacker = Address::generate(&env);
        let ta = Address::generate(&env);
        let new_token = env.register_stellar_asset_contract_v2(ta).address();
        let new_share = env.register(DummyShare, ());
        client.add_token(&attacker, &new_token, &new_share);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_remove_token_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _usdc_id, _share_token) = setup(&env);
        let ta2 = Address::generate(&env);
        let new_token = env.register_stellar_asset_contract_v2(ta2).address();
        let new_share = env.register(DummyShare, ());
        client.add_token(&admin, &new_token, &new_share);
        let attacker = Address::generate(&env);
        client.remove_token(&attacker, &new_token);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_set_yield_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let attacker = Address::generate(&env);
        client.set_yield(&attacker, &500u32);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_set_factoring_fee_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let attacker = Address::generate(&env);
        client.set_factoring_fee(&attacker, &100u32);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_set_compound_interest_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let attacker = Address::generate(&env);
        client.set_compound_interest(&attacker, &true);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_set_collateral_config_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let attacker = Address::generate(&env);
        client.set_collateral_config(&attacker, &1_000i128, &2_000u32);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_set_exchange_rate_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        client.set_rate_bounds(&admin, &usdc_id, &9_500u32, &10_500u32);
        let attacker = Address::generate(&env);
        client.set_exchange_rate(&attacker, &usdc_id, &10_000u32);
    }

    #[test]
    fn test_set_exchange_rate_within_bounds_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);

        client.set_rate_bounds(&admin, &usdc_id, &9_500u32, &10_500u32);
        client.set_exchange_rate(&admin, &usdc_id, &10_200u32);

        assert_eq!(client.get_exchange_rate(&usdc_id), 10_200u32);
        let bounds = client.get_rate_bounds(&usdc_id);
        assert_eq!(bounds.min_bps, 9_500u32);
        assert_eq!(bounds.max_bps, 10_500u32);
    }

    #[test]
    #[should_panic(expected = "rate out of bounds")]
    fn test_set_exchange_rate_outside_bounds_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);

        client.set_rate_bounds(&admin, &usdc_id, &9_500u32, &10_500u32);
        client.set_exchange_rate(&admin, &usdc_id, &10_600u32);
    }

    #[test]
    #[should_panic(expected = "invalid rate bounds")]
    fn test_set_rate_bounds_invalid_order_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);

        client.set_rate_bounds(&admin, &usdc_id, &10_500u32, &9_500u32);
    }

    #[test]
    fn test_yield_calc_no_overflow_large_principal() {
        let interest = calculate_interest(1_000_000_000_000_000u128, 5_000u32, 5 * SECS_PER_YEAR, false);
        assert!(interest > 0);
        assert!(interest < 3_000_000_000_000_000u128);
    }

    #[test]
    fn test_yield_calc_precision_small_amounts() {
        let interest = calculate_interest(1u128, 800u32, 86_400u64, false);
        assert_eq!(interest, 0u128);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_set_kyc_required_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let attacker = Address::generate(&env);
        client.set_kyc_required(&attacker, &true);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_set_investor_kyc_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let attacker = Address::generate(&env);
        let investor = Address::generate(&env);
        client.set_investor_kyc(&attacker, &investor, &true);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_propose_upgrade_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _usdc_id, _share_token) = setup(&env);
        let attacker = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[0u8; 32]);
        client.propose_upgrade(&attacker, &hash);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_fund_multiple_invoices_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);
        mint(&env, &usdc_id, &investor, 2_000);
        client.deposit(&investor, &usdc_id, &2_000);

        let mut requests = Vec::new(&env);
        requests.push_back(FundingRequest {
            invoice_id: 1u64,
            principal: 500,
            sme,
            due_date: env.ledger().timestamp() + 10_000,
            token: usdc_id,
        });
        let attacker = Address::generate(&env);
        client.fund_multiple_invoices(&attacker, &requests);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_seize_collateral_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        client.set_collateral_config(&admin, &1_000i128, &2_000u32);
        let principal: i128 = 5_000;
        let required = client.required_collateral_for(&principal);
        mint(&env, &usdc_id, &investor, 10_000);
        mint(&env, &usdc_id, &sme, required);
        client.deposit(&investor, &usdc_id, &10_000);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &required);
        client.fund_invoice(
            &admin,
            &1u64,
            &principal,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        let attacker = Address::generate(&env);
        client.seize_collateral(&attacker, &1u64);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_cleanup_funded_invoice_non_admin_panics() {
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
        let amount_due = client.estimate_repayment(&1u64);
        client.repay_invoice(&1u64, &sme, &amount_due);
        let attacker = Address::generate(&env);
        client.cleanup_funded_invoice(&attacker, &1u64);
    }

    // --- Pause mechanism tests ---

    #[test]
    #[should_panic(expected = "contract is paused")]
    fn test_fund_invoice_when_paused_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 2_000);
        client.deposit(&investor, &usdc_id, &2_000);
        client.pause(&admin);
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
    #[should_panic(expected = "contract is paused")]
    fn test_repay_invoice_when_paused_panics() {
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
        client.pause(&admin);
        let amount_due = client.estimate_repayment(&1u64);
        client.repay_invoice(&1u64, &sme, &amount_due);
    }

    #[test]
    #[should_panic(expected = "contract is paused")]
    fn test_deposit_collateral_when_paused_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let sme = Address::generate(&env);

        client.set_collateral_config(&admin, &1_000i128, &2_000u32);
        mint(&env, &usdc_id, &sme, 1_000);
        client.pause(&admin);
        client.deposit_collateral(&1u64, &sme, &usdc_id, &1_000);
    }

    #[test]
    fn test_pause_and_unpause_restores_operations() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);
        let sme = Address::generate(&env);

        mint(&env, &usdc_id, &investor, 2_000);
        mint(&env, &usdc_id, &sme, 2_000);
        client.deposit(&investor, &usdc_id, &2_000);

        client.pause(&admin);
        assert!(client.is_paused());

        client.unpause(&admin);
        assert!(!client.is_paused());

        client.fund_invoice(
            &admin,
            &1u64,
            &1_000i128,
            &sme,
            &(env.ledger().timestamp() + 10_000),
            &usdc_id,
        );
        let amount_due = client.estimate_repayment(&1u64);
        client.repay_invoice(&1u64, &sme, &amount_due);
        let fi = client.get_funded_invoice(&1u64).unwrap();
        assert!(fi.repaid_amount >= amount_due);
    }

    // --- KYC gate tests ---

    #[test]
    #[should_panic(expected = "investor not KYC approved")]
    fn test_deposit_when_kyc_required_unapproved_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        client.set_kyc_required(&admin, &true);
        mint(&env, &usdc_id, &investor, 1_000);
        client.deposit(&investor, &usdc_id, &1_000);
    }

    #[test]
    fn test_deposit_when_kyc_required_and_approved_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        client.set_kyc_required(&admin, &true);
        client.set_investor_kyc(&admin, &investor, &true);
        mint(&env, &usdc_id, &investor, 1_000);
        client.deposit(&investor, &usdc_id, &1_000);

        let tt = client.get_token_totals(&usdc_id);
        assert_eq!(tt.pool_value, 1_000);
    }

    #[test]
    #[should_panic(expected = "investor not KYC approved")]
    fn test_kyc_revocation_blocks_deposit() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        client.set_kyc_required(&admin, &true);
        client.set_investor_kyc(&admin, &investor, &true);
        mint(&env, &usdc_id, &investor, 2_000);
        client.deposit(&investor, &usdc_id, &1_000);

        // Revoke KYC — subsequent deposit must be blocked
        client.set_investor_kyc(&admin, &investor, &false);
        client.deposit(&investor, &usdc_id, &1_000);
    }

    #[test]
    fn test_kyc_not_required_allows_any_investor() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, usdc_id, _share_token) = setup(&env);
        let investor = Address::generate(&env);

        // KYC disabled by default — any investor can deposit
        assert!(!client.kyc_required());
        mint(&env, &usdc_id, &investor, 500);
        client.deposit(&investor, &usdc_id, &500);

        let tt = client.get_token_totals(&usdc_id);
        assert_eq!(tt.pool_value, 500);
    }
}
