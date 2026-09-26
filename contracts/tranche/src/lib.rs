#![no_std]

pub mod deposit;
pub mod errors;
pub mod events;
pub mod funding;
pub mod math;
pub mod repayment;
pub mod state;
pub mod withdraw;

use errors::TrancheError;
use events::{CONFIG, EVT};
use state::{DataKey, TrancheAccounting, TrancheClass, TrancheConfig, TranchePool};

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env};

const REENTRANCY_GUARD: u64 = 1;

// TTL bumping: all of the tranche contract's state (TranchePool,
// InvestorPosition, InvoiceTrancheExposure) lives in instance() storage,
// which is a single ledger entry shared with the contract instance itself.
// Without extending its TTL, that entry eventually falls below the
// persistent-entry threshold and gets archived — after which every read
// silently falls back to the default/missing value and the pool's
// accounting resets. Every state-touching entrypoint therefore bumps the
// instance (and, with it, the whole pool's storage). See #1308.
const LEDGERS_PER_DAY: u32 = 17_280;
const INSTANCE_LIFETIME_THRESHOLD: u32 = LEDGERS_PER_DAY * 7; // bump once TTL drops below ~7 days
const INSTANCE_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 30; // restore TTL to ~30 days

#[contract]
pub struct TrancheContract;

#[contractimpl]
impl TrancheContract {
    fn non_reentrant_start(env: &Env) {
        if env.storage().instance().has(&DataKey::NonReentrantKey) {
            panic_with_error!(env, TrancheError::ReentrancyDetected);
        }
        env.storage()
            .instance()
            .set(&DataKey::NonReentrantKey, &REENTRANCY_GUARD);
    }

    fn non_reentrant_end(env: &Env) {
        env.storage().instance().remove(&DataKey::NonReentrantKey);
    }

    /// Extend the TTL of the instance storage entry holding all of the
    /// contract's state. Must be called by every entrypoint that reads or
    /// writes pool/position/exposure data so archived entries can never be
    /// read back as default values. (#1308)
    fn bump_instance(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Require authorization from the stored admin, panicking with the typed
    /// `NotInitialized` error when no admin has been set.
    fn require_admin_auth(env: &Env) {
        let admin = Self::get_admin(env.clone()).unwrap_or_else(|e| panic_with_error!(env, e));
        admin.require_auth();
    }

    fn require_not_paused(env: &Env) {
        if env
            .storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            panic_with_error!(env, TrancheError::ContractPaused);
        }
    }

    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        senior_share_token: Address,
        junior_share_token: Address,
        config: TrancheConfig,
    ) {
        Self::bump_instance(&env);

        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, TrancheError::AlreadyInitialized);
        }

        Self::validate_share_tokens(&env, &senior_share_token, &junior_share_token);

        admin.require_auth();

        env.storage().instance().set(&DataKey::Admin, &admin);

        let pool = TranchePool {
            token: token.clone(),
            senior_share_token,
            junior_share_token,
            config,
            senior: TrancheAccounting::default(),
            junior: TrancheAccounting::default(),
        };

        env.storage().instance().set(&DataKey::Pool(token), &pool);
    }

    pub fn get_pool(env: Env, token: Address) -> TranchePool {
        env.storage()
            .instance()
            .get(&DataKey::Pool(token))
            .unwrap_or_else(|| panic_with_error!(&env, TrancheError::PoolNotFound))
    }

    pub fn get_admin(env: Env) -> Result<Address, TrancheError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(TrancheError::NotInitialized)
    }

    pub fn get_config(env: Env, token: Address) -> TrancheConfig {
        let pool = Self::get_pool(env, token);
        pool.config
    }

    pub fn get_totals(env: Env, token: Address, tranche: TrancheClass) -> TrancheAccounting {
        let pool = Self::get_pool(env, token);

        match tranche {
            TrancheClass::Senior => pool.senior,
            TrancheClass::Junior => pool.junior,
        }
    }

    pub fn deposit_tranche(
        env: Env,
        investor: Address,
        token: Address,
        tranche: TrancheClass,
        amount: i128,
    ) {
        Self::bump_instance(&env);
        Self::require_not_paused(&env);
        Self::non_reentrant_start(&env);
        deposit::deposit(&env, investor, token, tranche, amount);
        Self::non_reentrant_end(&env);
    }

    pub fn withdraw_tranche(
        env: Env,
        investor: Address,
        token: Address,
        tranche: TrancheClass,
        amount: i128,
    ) {
        Self::bump_instance(&env);
        Self::require_not_paused(&env);
        Self::non_reentrant_start(&env);
        withdraw::withdraw(&env, investor, token, tranche, amount);
        Self::non_reentrant_end(&env);
    }

    pub fn get_position(
        env: Env,
        investor: Address,
        token: Address,
        tranche: TrancheClass,
    ) -> Option<state::InvestorPosition> {
        env.storage()
            .instance()
            .get(&DataKey::Investor(investor, token, tranche))
    }

    /// Previews how much more the senior tranche can accept before a deposit
    /// would revert with `AdvanceRateExceeded`.
    pub fn get_advance_rate_headroom(env: Env, token: Address) -> i128 {
        let pool = Self::get_pool(env, token);

        let rate_bps = pool.config.senior_advance_rate_bps as i128;
        if rate_bps >= 10_000 {
            return i128::MAX;
        }

        let senior = pool.senior.deposited;
        let junior = pool.junior.deposited;

        // Largest `amount` such that:
        //   senior + amount <= rate_bps * (junior + senior + amount) / 10_000
        //
        // Use checked arithmetic so that a very large deposit base does not
        // cause an overflow panic (overflow-checks = true in the test/debug
        // profile).  If either multiplication overflows the capacity available
        // is effectively unlimited, so we return i128::MAX consistent with the
        // rate_bps >= 10_000 fast-path above.
        let junior_term = match rate_bps.checked_mul(junior) {
            Some(v) => v,
            None => return i128::MAX,
        };
        let senior_term = match (10_000 - rate_bps).checked_mul(senior) {
            Some(v) => v,
            None => return i128::MAX,
        };
        let numerator = match junior_term.checked_sub(senior_term) {
            Some(v) => v,
            None => return 0,
        };
        if numerator <= 0 {
            return 0;
        }

        numerator / (10_000 - rate_bps)
    }

    fn validate_config(env: &Env, config: &TrancheConfig) {
        if config.senior_target_yield_bps > 10_000
            || config.senior_advance_rate_bps > 10_000
            || config.junior_first_loss_bps > 10_000
        {
            panic_with_error!(env, TrancheError::InvalidAmount);
        }
    }

    /// The senior and junior classes must be tracked by *different* share
    /// tokens. If both point at the same address, `deposit` mints and
    /// `withdraw` burns against one token for both classes, so senior and
    /// junior holders end up sharing a single fungible claim and the
    /// waterfall's seniority guarantee is gone. (#1303)
    fn validate_share_tokens(env: &Env, senior_share_token: &Address, junior_share_token: &Address) {
        if senior_share_token == junior_share_token {
            panic_with_error!(env, TrancheError::InvalidShareTokens);
        }
    }

    pub fn set_tranche_config(
        env: Env,
        admin: Address,
        token: Address,
        senior_target_yield_bps: u32,
        senior_advance_rate_bps: u32,
        junior_first_loss_bps: u32,
    ) -> Result<(), TrancheError> {
        Self::bump_instance(&env);

        admin.require_auth();

        let stored_admin = Self::get_admin(env.clone())?;
        if admin != stored_admin {
            panic_with_error!(&env, TrancheError::Unauthorized);
        }

        let config = TrancheConfig {
            senior_target_yield_bps,
            senior_advance_rate_bps,
            junior_first_loss_bps,
        };
        Self::validate_config(&env, &config);

        let mut pool = Self::get_pool(env.clone(), token.clone());
        pool.config = config;

        env.storage()
            .instance()
            .set(&DataKey::Pool(token.clone()), &pool);

        env.events().publish(
            (EVT, CONFIG),
            (
                token,
                senior_target_yield_bps,
                senior_advance_rate_bps,
                junior_first_loss_bps,
            ),
        );

        Ok(())
    }

    pub fn open_tranche_for_token(
        env: Env,
        admin: Address,
        token: Address,
        senior_share_token: Address,
        junior_share_token: Address,
        config: TrancheConfig,
    ) -> Result<(), TrancheError> {
        Self::bump_instance(&env);

        admin.require_auth();

        let stored_admin = Self::get_admin(env.clone())?;
        if admin != stored_admin {
            panic_with_error!(&env, TrancheError::Unauthorized);
        }

        Self::validate_config(&env, &config);
        Self::validate_share_tokens(&env, &senior_share_token, &junior_share_token);

        if env
            .storage()
            .instance()
            .has(&DataKey::Pool(token.clone()))
        {
            panic_with_error!(&env, TrancheError::AlreadyInitialized);
        }

        let pool = TranchePool {
            token: token.clone(),
            senior_share_token,
            junior_share_token,
            config,
            senior: TrancheAccounting::default(),
            junior: TrancheAccounting::default(),
        };

        env.storage()
            .instance()
            .set(&DataKey::Pool(token.clone()), &pool);
        env.storage()
            .instance()
            .set(&DataKey::TrancheEnabled(token.clone()), &true);

        env.events().publish(
            (EVT, CONFIG),
            (
                token,
                config.senior_target_yield_bps,
                config.senior_advance_rate_bps,
                config.junior_first_loss_bps,
            ),
        );
        Ok(())
    }

    pub fn is_tranche_enabled(env: Env, token: Address) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::TrancheEnabled(token))
            .unwrap_or(false)
    }

    pub fn fund_invoice_from_tranches(
        env: Env,
        token: Address,
        invoice_id: u64,
        total_amount: i128,
    ) -> (i128, i128) {
        Self::bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin_auth(&env);
        Self::non_reentrant_start(&env);
        let result = funding::fund_invoice_from_tranches(&env, token, invoice_id, total_amount);
        Self::non_reentrant_end(&env);
        (result.senior_amount, result.junior_amount)
    }

    pub fn get_invoice_exposure(
        env: Env,
        invoice_id: u64,
    ) -> Option<state::InvoiceTrancheExposure> {
        funding::get_invoice_exposure(&env, invoice_id)
    }

    pub fn distribute_waterfall_repayment(
        env: Env,
        token: Address,
        invoice_id: u64,
        total_due: i128,
        elapsed_secs: u64,
    ) -> (i128, i128) {
        Self::bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin_auth(&env);
        Self::non_reentrant_start(&env);
        let result = repayment::distribute_waterfall_repayment(
            &env,
            token,
            invoice_id,
            total_due,
            elapsed_secs,
        );
        Self::non_reentrant_end(&env);
        result
    }

    pub fn allocate_loss(env: Env, token: Address, invoice_id: u64, shortfall: i128) {
        Self::bump_instance(&env);
        Self::require_not_paused(&env);
        Self::require_admin_auth(&env);
        Self::non_reentrant_start(&env);
        repayment::allocate_loss(&env, token, invoice_id, shortfall);
        Self::non_reentrant_end(&env);
    }

    pub fn simulate_waterfall(
        env: Env,
        token: Address,
        invoice_id: u64,
        hypothetical_repayment: i128,
        elapsed_secs: u64,
    ) -> (i128, i128) {
        let pool = Self::get_pool(env.clone(), token);
        let exposure: state::InvoiceTrancheExposure = env
            .storage()
            .instance()
            .get(&DataKey::InvoiceExposure(invoice_id))
            .unwrap_or_else(|| panic_with_error!(&env, TrancheError::ExposureNotFound));

        math::calculate_waterfall_split(
            &env,
            hypothetical_repayment,
            exposure.senior_deployed,
            pool.config.senior_target_yield_bps,
            elapsed_secs,
        )
    }

    /// Lifetime return in basis points: (earned - losses) * 10_000 / deposited.
    /// Not annualized — has no time component, so it is not an APY. A pool
    /// that returned 5% over one week and one that returned 5% over three
    /// years both report 500 here. Callers wanting an annual rate must
    /// weight this by elapsed time themselves.
    pub fn get_lifetime_return_bps(env: Env, token: Address, tranche: TrancheClass) -> u32 {
        let pool = Self::get_pool(env.clone(), token);
        let accounting = match tranche {
            TrancheClass::Senior => pool.senior,
            TrancheClass::Junior => pool.junior,
        };

        if accounting.deposited == 0 {
            return 0;
        }

        let total_return = accounting.earned - accounting.losses;
        if total_return <= 0 {
            return 0;
        }

        // Convert to basis points (lifetime, not annualized — see get_lifetime_return_bps).
        let return_bps = (total_return * 10_000) / accounting.deposited;
        return_bps.min(u32::MAX as i128) as u32
    }

    pub fn pause(env: Env, admin: Address) -> Result<(), TrancheError> {
        Self::bump_instance(&env);
        admin.require_auth();

        let stored_admin = Self::get_admin(env.clone())?;
        if admin != stored_admin {
            panic_with_error!(&env, TrancheError::Unauthorized);
        }

        env.storage().instance().set(&DataKey::Paused, &true);
        env.events()
            .publish((EVT, PAUSED), (admin, env.ledger().timestamp()));
        Ok(())
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), TrancheError> {
        Self::bump_instance(&env);
        admin.require_auth();

        let stored_admin = Self::get_admin(env.clone())?;
        if admin != stored_admin {
            panic_with_error!(&env, TrancheError::Unauthorized);
        }

        env.storage().instance().set(&DataKey::Paused, &false);
        env.events()
            .publish((EVT, PAUSED), (admin, env.ledger().timestamp()));
        Ok(())
    }

    pub fn is_paused(env: Env) -> bool {
        Self::bump_instance(&env);
        env.storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Paused)
            .unwrap_or(false)
    }
}
