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
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        senior_share_token: Address,
        junior_share_token: Address,
        config: TrancheConfig,
    ) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, TrancheError::AlreadyInitialized);
        }

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
        let numerator = rate_bps * junior - (10_000 - rate_bps) * senior;
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

    pub fn set_tranche_config(
        env: Env,
        admin: Address,
        token: Address,
        senior_target_yield_bps: u32,
        senior_advance_rate_bps: u32,
        junior_first_loss_bps: u32,
    ) -> Result<(), TrancheError> {
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
        admin.require_auth();

        let stored_admin = Self::get_admin(env.clone())?;
        if admin != stored_admin {
            panic_with_error!(&env, TrancheError::Unauthorized);
        }

        Self::validate_config(&env, &config);

        if env
            .storage()
            .instance()
            .has(&DataKey::TrancheEnabled(token.clone()))
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
            .unwrap_or_else(|| panic_with_error!(&env, TrancheError::PoolNotFound));

        math::calculate_waterfall_split(
            &env,
            hypothetical_repayment,
            exposure.senior_deployed,
            pool.config.senior_target_yield_bps,
            elapsed_secs,
        )
    }

    pub fn get_effective_apy(env: Env, token: Address, tranche: TrancheClass) -> u32 {
        let pool = Self::get_pool(env.clone(), token);
        let accounting = match tranche {
            TrancheClass::Senior => pool.senior,
            TrancheClass::Junior => pool.junior,
        };

        if accounting.deposited == 0 {
            return 0;
        }

        // Calculate realized APY based on earned vs deposited
        // This is a simplified calculation - in production would use time-weighted returns
        let total_return = accounting.earned - accounting.losses;
        if total_return <= 0 {
            return 0;
        }

        // Convert to basis points (annualized)
        let return_bps = (total_return * 10_000) / accounting.deposited;
        return_bps as u32
    }
}
