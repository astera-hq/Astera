use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};
use tranche::{state::TrancheClass, TrancheContract};

/// Registers a real token contract: `deposit_tranche`/`withdraw_tranche`
/// perform actual token transfers, so a plain generated address won't do.
fn create_token(env: &Env) -> Address {
    env.register_stellar_asset_contract_v2(Address::generate(env))
        .address()
}

fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token_id).mint(to, &amount);
}

#[contract]
pub struct DummyShare;

#[contractimpl]
impl DummyShare {
    pub fn total_supply(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "tot"))
            .unwrap_or(0)
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage().persistent().get(&id).unwrap_or(0)
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        let total = Self::total_supply(env.clone());
        let balance = Self::balance(env.clone(), to.clone());
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "tot"), &(total + amount));
        env.storage().persistent().set(&to, &(balance + amount));
    }

    pub fn burn(env: Env, from: Address, amount: i128) {
        let total = Self::total_supply(env.clone());
        let balance = Self::balance(env.clone(), from.clone());
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "tot"), &(total - amount));
        env.storage().persistent().set(&from, &(balance - amount));
    }
}

#[test]
fn test_advance_rate_enforcement() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = create_token(&env);
    let senior_share_token = env.register(DummyShare, ());
    let junior_share_token = env.register(DummyShare, ());

    let contract_id = env.register(TrancheContract, ());

    env.as_contract(&contract_id, || {
        TrancheContract::initialize(
            env.clone(),
            admin.clone(),
            token.clone(),
            senior_share_token.clone(),
            junior_share_token.clone(),
            tranche::state::TrancheConfig {
                senior_target_yield_bps: 1000,
                senior_advance_rate_bps: 8000,
                junior_first_loss_bps: 10000,
            },
        );
    });

    env.as_contract(&contract_id, || {
        TrancheContract::open_tranche_for_token(
            env.clone(),
            admin.clone(),
            token.clone(),
            senior_share_token,
            junior_share_token,
            tranche::state::TrancheConfig {
                senior_target_yield_bps: 1000,
                senior_advance_rate_bps: 8000,
                junior_first_loss_bps: 10000,
            },
        )
        .unwrap();
    });

    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);
    mint(&env, &token, &investor1, 1_000_000);
    mint(&env, &token, &investor2, 1_000_000);

    // First, deposit junior to create capacity
    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor2.clone(),
            token.clone(),
            TrancheClass::Junior,
            2000,
        );
    });

    // Senior can deposit up to 80% of total (2000 junior + X senior)
    // 0.8 * (2000 + X) = X => 1600 + 0.8X = X => 1600 = 0.2X => X = 8000
    // So senior can deposit up to 8000 with 2000 junior

    // This should succeed
    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor1.clone(),
            token.clone(),
            TrancheClass::Senior,
            8000,
        );
    });

    // Verify the pool state
    env.as_contract(&contract_id, || {
        let pool = TrancheContract::get_pool(env.clone(), token.clone());
        assert_eq!(pool.senior.deposited, 8000);
        assert_eq!(pool.junior.deposited, 2000);
    });
}

#[test]
fn test_advance_rate_100_percent() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = create_token(&env);
    let senior_share_token = env.register(DummyShare, ());
    let junior_share_token = env.register(DummyShare, ());

    let contract_id = env.register(TrancheContract, ());

    env.as_contract(&contract_id, || {
        TrancheContract::initialize(
            env.clone(),
            admin.clone(),
            token.clone(),
            senior_share_token.clone(),
            junior_share_token.clone(),
            tranche::state::TrancheConfig {
                senior_target_yield_bps: 1000,
                senior_advance_rate_bps: 10000,
                junior_first_loss_bps: 10000,
            },
        );
    });

    env.as_contract(&contract_id, || {
        TrancheContract::open_tranche_for_token(
            env.clone(),
            admin.clone(),
            token.clone(),
            senior_share_token,
            junior_share_token,
            tranche::state::TrancheConfig {
                senior_target_yield_bps: 1000,
                senior_advance_rate_bps: 10000,
                junior_first_loss_bps: 10000,
            },
        )
        .unwrap();
    });

    let investor1 = Address::generate(&env);
    mint(&env, &token, &investor1, 1_000_000);

    // With 100% advance rate, senior can deposit without junior
    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor1.clone(),
            token.clone(),
            TrancheClass::Senior,
            10000,
        );
    });

    env.as_contract(&contract_id, || {
        let pool = TrancheContract::get_pool(env.clone(), token.clone());
        assert_eq!(pool.senior.deposited, 10000);
    });
}

#[test]
fn test_get_advance_rate_headroom() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = create_token(&env);
    let senior_share_token = env.register(DummyShare, ());
    let junior_share_token = env.register(DummyShare, ());

    let contract_id = env.register(TrancheContract, ());

    env.as_contract(&contract_id, || {
        TrancheContract::initialize(
            env.clone(),
            admin.clone(),
            token.clone(),
            senior_share_token.clone(),
            junior_share_token.clone(),
            tranche::state::TrancheConfig {
                senior_target_yield_bps: 1000,
                senior_advance_rate_bps: 8000,
                junior_first_loss_bps: 10000,
            },
        );
    });

    env.as_contract(&contract_id, || {
        TrancheContract::open_tranche_for_token(
            env.clone(),
            admin.clone(),
            token.clone(),
            senior_share_token,
            junior_share_token,
            tranche::state::TrancheConfig {
                senior_target_yield_bps: 1000,
                senior_advance_rate_bps: 8000,
                junior_first_loss_bps: 10000,
            },
        )
        .unwrap();
    });

    // No deposits yet: headroom is 0 (no junior capacity to support senior).
    env.as_contract(&contract_id, || {
        let headroom = TrancheContract::get_advance_rate_headroom(env.clone(), token.clone());
        assert_eq!(headroom, 0);
    });

    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);
    mint(&env, &token, &investor1, 1_000_000);
    mint(&env, &token, &investor2, 1_000_000);

    // Junior deposits 2000: senior can take up to 8000 at an 80% advance rate.
    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor2.clone(),
            token.clone(),
            TrancheClass::Junior,
            2000,
        );
    });

    env.as_contract(&contract_id, || {
        let headroom = TrancheContract::get_advance_rate_headroom(env.clone(), token.clone());
        assert_eq!(headroom, 8000);
    });

    // Depositing exactly the headroom should succeed and leave zero headroom.
    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor1.clone(),
            token.clone(),
            TrancheClass::Senior,
            8000,
        );
    });

    env.as_contract(&contract_id, || {
        let headroom = TrancheContract::get_advance_rate_headroom(env.clone(), token.clone());
        assert_eq!(headroom, 0);
    });
}
