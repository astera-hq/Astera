use soroban_sdk::{panic_with_error, token, Address, Env, IntoVal, Symbol, Vec};

use crate::{
    errors::TrancheError,
    events::{DEPOSIT, EVT},
    math::calculate_shares_to_mint,
    state::{DataKey, InvestorPosition, TrancheAccounting, TrancheClass, TranchePool},
};

pub fn deposit(env: &Env, investor: Address, token: Address, tranche: TrancheClass, amount: i128) {
    if amount <= 0 {
        panic_with_error!(env, TrancheError::InvalidAmount);
    }

    investor.require_auth();

    let token_client = token::Client::new(env, &token);
    token_client.transfer(&investor, &env.current_contract_address(), &amount);

    let mut pool: TranchePool = env
        .storage()
        .instance()
        .get(&DataKey::Pool(token.clone()))
        .unwrap_or_else(|| panic_with_error!(env, TrancheError::PoolNotFound));

    let key = DataKey::Investor(investor.clone(), token.clone(), tranche);

    let mut position: InvestorPosition = env.storage().instance().get(&key).unwrap_or_default();

    match tranche {
        TrancheClass::Senior => {
            // Enforce senior advance rate: seniors can't exceed their configured
            // percentage of total pool value
            let new_total_deposited = pool.junior.deposited + pool.senior.deposited + amount;
            let senior_target =
                pool.config.senior_advance_rate_bps as i128 * new_total_deposited / 10_000;
            if pool.senior.deposited + amount > senior_target {
                panic_with_error!(env, TrancheError::AdvanceRateExceeded);
            }
            update_accounting(&mut pool.senior, amount);
        }
        TrancheClass::Junior => {
            update_accounting(&mut pool.junior, amount);
        }
    }

    position.deposited += amount;

    let (tranche_accounting, share_token) = match tranche {
        TrancheClass::Senior => (&pool.senior, pool.senior_share_token.clone()),
        TrancheClass::Junior => (&pool.junior, pool.junior_share_token.clone()),
    };

    let pool_value = tranche_accounting.deposited
        + tranche_accounting.earned
        - tranche_accounting.losses;
    let shares_to_mint = calculate_shares_to_mint(amount, pool_value, tranche_accounting.total_shares);
    position.shares += shares_to_mint;

    match tranche {
        TrancheClass::Senior => pool.senior.total_shares += shares_to_mint,
        TrancheClass::Junior => pool.junior.total_shares += shares_to_mint,
    }

    env.storage().instance().set(&key, &position);
    env.storage()
        .instance()
        .set(&DataKey::Pool(token.clone()), &pool);

    let mut mint_args = Vec::new(env);
    mint_args.push_back(investor.clone().into_val(env));
    mint_args.push_back(shares_to_mint.into_val(env));
    let _: () = env.invoke_contract(&share_token, &Symbol::new(env, "mint"), mint_args);

    env.events()
        .publish((EVT, DEPOSIT), (investor, token, tranche, amount));
}

fn update_accounting(accounting: &mut TrancheAccounting, amount: i128) {
    accounting.deposited += amount;
    accounting.available += amount;
}
