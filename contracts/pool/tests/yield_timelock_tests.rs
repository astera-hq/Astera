#![cfg(test)]

use pool::{FundingPool, FundingPoolClient, PoolError};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

fn create_token_contract<'a>(env: &Env, admin: &Address) -> token::StellarAssetClient<'a> {
    token::StellarAssetClient::new(
        env,
        &env.register_stellar_asset_contract_v2(admin.clone())
            .address(),
    )
}

fn setup(env: &Env) -> (FundingPoolClient, Address) {
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token = create_token_contract(env, &token_admin);
    let share_token = create_token_contract(env, &token_admin);
    let invoice_contract = Address::generate(env);

    let pool_id = env.register(FundingPool, ());
    let client = FundingPoolClient::new(env, &pool_id);

    client.initialize(
        &admin,
        &token.address,
        &share_token.address,
        &invoice_contract,
    );
    (client, admin)
}

// `initialize` seeds `last_yield_change_at` to the init timestamp, so the
// very first yield-change proposal is itself subject to the standard
// cooldown between changes. Every test here proposes at `PROPOSE_AT`
// (safely past that cooldown) rather than immediately at init time.
const INIT_AT: u64 = 1_000_000;
const YIELD_CHANGE_COOLDOWN_SECS: u64 = 86_400;
const YIELD_TIMELOCK_SECS: u64 = 172_800;
const PROPOSE_AT: u64 = INIT_AT + YIELD_CHANGE_COOLDOWN_SECS + 1;

#[test]
fn test_execute_yield_change_rejected_before_timelock() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = INIT_AT);

    let (client, admin) = setup(&env);

    // Propose a yield change
    let new_yield_bps = 1000u32;
    env.ledger().with_mut(|l| l.timestamp = PROPOSE_AT);
    client.propose_yield_change(&admin, &new_yield_bps);

    // Try to execute before the 48h yield timelock has elapsed
    env.ledger().with_mut(|l| l.timestamp = PROPOSE_AT + 86_400); // +24 hours
    let result = client.try_execute_yield_change();
    assert_eq!(
        result.unwrap_err().unwrap(),
        PoolError::YieldChangeNotReady.into()
    );
}

#[test]
fn test_execute_yield_change_succeeds_at_timelock_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = INIT_AT);

    let (client, admin) = setup(&env);

    // Propose a yield change
    let new_yield_bps = 1000u32;
    env.ledger().with_mut(|l| l.timestamp = PROPOSE_AT);
    client.propose_yield_change(&admin, &new_yield_bps);

    // Execute exactly at timelock boundary (48 hours = 172,800 seconds)
    env.ledger()
        .with_mut(|l| l.timestamp = PROPOSE_AT + YIELD_TIMELOCK_SECS);
    client.execute_yield_change();

    // Verify the yield was updated
    let config = client.get_config();
    assert_eq!(config.yield_bps, new_yield_bps);
}

#[test]
fn test_execute_yield_change_succeeds_after_timelock_elapsed() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = INIT_AT);

    let (client, admin) = setup(&env);

    // Propose a yield change
    let new_yield_bps = 1000u32;
    env.ledger().with_mut(|l| l.timestamp = PROPOSE_AT);
    client.propose_yield_change(&admin, &new_yield_bps);

    // Execute well after timelock (72 hours)
    env.ledger()
        .with_mut(|l| l.timestamp = PROPOSE_AT + 259_200);
    client.execute_yield_change();

    // Verify the yield was updated
    let config = client.get_config();
    assert_eq!(config.yield_bps, new_yield_bps);
}

#[test]
fn test_cancel_yield_proposal_clears_pending() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = INIT_AT);

    let (client, admin) = setup(&env);

    // Propose a yield change
    let new_yield_bps = 1000u32;
    env.ledger().with_mut(|l| l.timestamp = PROPOSE_AT);
    client.propose_yield_change(&admin, &new_yield_bps);

    // Cancel the proposal
    client.cancel_yield_proposal(&admin);

    // Try to execute should now fail with YieldProposalNotFound
    env.ledger()
        .with_mut(|l| l.timestamp = PROPOSE_AT + YIELD_TIMELOCK_SECS);
    let result = client.try_execute_yield_change();
    assert_eq!(
        result.unwrap_err().unwrap(),
        PoolError::YieldProposalNotFound.into()
    );
}

#[test]
fn test_cancel_yield_proposal_allows_new_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = INIT_AT);

    let (client, admin) = setup(&env);

    // Propose first yield change
    env.ledger().with_mut(|l| l.timestamp = PROPOSE_AT);
    client.propose_yield_change(&admin, &1000u32);

    // Cancel the proposal
    client.cancel_yield_proposal(&admin);

    // Propose a new yield change. Must stay within max_yield_change_bps
    // (200) of the still-unchanged current yield (800 default).
    let new_yield_bps = 950u32;
    client.propose_yield_change(&admin, &new_yield_bps);

    // Execute the new proposal after timelock
    env.ledger()
        .with_mut(|l| l.timestamp = PROPOSE_AT + YIELD_TIMELOCK_SECS);
    client.execute_yield_change();

    // Verify the second yield was applied
    let config = client.get_config();
    assert_eq!(config.yield_bps, new_yield_bps);
}

#[test]
fn test_execute_yield_change_without_proposal_fails() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = INIT_AT);

    let (client, _admin) = setup(&env);

    // Try to execute without proposing
    let result = client.try_execute_yield_change();
    assert_eq!(
        result.unwrap_err().unwrap(),
        PoolError::YieldProposalNotFound.into()
    );
}
