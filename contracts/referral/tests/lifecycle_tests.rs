#![cfg(test)]

use referral::{ReferralContract, ReferralContractClient, ReferralError};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, Symbol,
};

fn setup(env: &Env) -> (ReferralContractClient<'_>, Address, Address) {
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);
    let contract_id = env.register(ReferralContract, ());
    let client = ReferralContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let pool = Address::generate(env);
    client.initialize(&admin, &pool);
    (client, admin, pool)
}

fn setup_token(env: &Env) -> Address {
    let token_admin = Address::generate(env);
    env.register_stellar_asset_contract_v2(token_admin)
        .address()
}

#[test]
fn test_initialize_requires_admin_authorization() {
    let env = Env::default();
    let contract_id = env.register(ReferralContract, ());
    let client = ReferralContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let pool = Address::generate(&env);

    assert!(client.try_initialize(&admin, &pool).is_err());

    env.mock_all_auths();
    client.initialize(&admin, &pool);
    assert_eq!(client.get_pool(), pool);
}

fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token_id).mint(to, &amount);
}

#[test]
fn test_register_sets_referrer() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _pool) = setup(&env);
    let referee = Address::generate(&env);
    let referrer = Address::generate(&env);

    client.register(&referee, &referrer);

    assert_eq!(client.get_referrer(&referee), Some(referrer));
}

#[test]
fn test_register_rejects_self_referral() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _pool) = setup(&env);
    let referee = Address::generate(&env);

    let result = client.try_register(&referee, &referee);
    assert_eq!(
        result,
        Err(Ok::<soroban_sdk::Error, _>(
            ReferralError::SelfReferral.into()
        ))
    );
}

#[test]
fn test_register_cannot_change_referrer() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _pool) = setup(&env);
    let referee = Address::generate(&env);
    let referrer_a = Address::generate(&env);
    let referrer_b = Address::generate(&env);

    client.register(&referee, &referrer_a);
    let result = client.try_register(&referee, &referrer_b);

    assert_eq!(
        result,
        Err(Ok::<soroban_sdk::Error, _>(
            ReferralError::AlreadyRegistered.into()
        ))
    );
    assert_eq!(client.get_referrer(&referee), Some(referrer_a));
}

#[test]
fn test_record_activity_rejects_non_pool_caller() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _pool) = setup(&env);
    let referee = Address::generate(&env);
    let referrer = Address::generate(&env);
    let token = setup_token(&env);
    client.register(&referee, &referrer);

    let attacker = Address::generate(&env);
    let result = client.try_record_activity(
        &attacker,
        &referee,
        &Symbol::new(&env, "borrow"),
        &1_000i128,
        &token,
    );
    assert_eq!(
        result,
        Err(Ok::<soroban_sdk::Error, _>(
            ReferralError::Unauthorized.into()
        ))
    );
}

#[test]
fn test_record_activity_no_referrer_returns_zero_reward() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, pool) = setup(&env);
    let referee = Address::generate(&env);
    let token = setup_token(&env);

    let reward = client.record_activity(
        &pool,
        &referee,
        &Symbol::new(&env, "borrow"),
        &1_000i128,
        &token,
    );
    assert_eq!(reward, 0);
}

#[test]
fn test_record_activity_requires_a_token_lifetime_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, pool) = setup(&env);
    let referee = Address::generate(&env);
    let referrer = Address::generate(&env);
    let token = setup_token(&env);
    client.register(&referee, &referrer);

    let reward = client.record_activity(
        &pool,
        &referee,
        &Symbol::new(&env, "borrow"),
        &1_000_0000000i128,
        &token,
    );

    assert_eq!(reward, 0);
    assert_eq!(client.get_stats(&referrer).referral_count, 1);
}

#[test]
fn test_record_activity_activates_on_zero_fee_first_deposit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, pool) = setup(&env);
    let referee = Address::generate(&env);
    let referrer = Address::generate(&env);
    let token = setup_token(&env);
    client.set_lifetime_reward_cap(&admin, &token, &60_0000000i128);
    client.register(&referee, &referrer);

    // A $0-fee first deposit still activates (counts) the referral, even
    // though it earns no reward (no yield fee to share yet).
    let reward = client.record_activity(
        &pool,
        &referee,
        &Symbol::new(&env, "deposit"),
        &0i128,
        &token,
    );
    assert_eq!(reward, 0);
    assert_eq!(client.get_stats(&referrer).referral_count, 1);
    assert_eq!(client.get_pending_reward(&referrer, &token), 0);
}

#[test]
fn test_record_activity_credits_borrow_reward_and_activates_once() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, pool) = setup(&env);
    let referee = Address::generate(&env);
    let referrer = Address::generate(&env);
    let token = setup_token(&env);
    client.set_lifetime_reward_cap(&admin, &token, &60_0000000i128);
    client.register(&referee, &referrer);

    // Default borrow bps is 500 (5%): 5% of 1_000_0000000 = 50_0000000.
    let reward1 = client.record_activity(
        &pool,
        &referee,
        &Symbol::new(&env, "borrow"),
        &1_000_0000000i128,
        &token,
    );
    assert_eq!(reward1, 50_0000000i128);
    assert_eq!(client.get_stats(&referrer).referral_count, 1);
    assert_eq!(client.get_pending_reward(&referrer, &token), 50_0000000i128);

    // A second qualifying activity accrues more reward but does not bump
    // referral_count again (activation is a one-time event).
    let reward2 = client.record_activity(
        &pool,
        &referee,
        &Symbol::new(&env, "borrow"),
        &200_0000000i128,
        &token,
    );
    assert_eq!(reward2, 10_0000000i128);
    assert_eq!(client.get_stats(&referrer).referral_count, 1);
    assert_eq!(client.get_pending_reward(&referrer, &token), 60_0000000i128);
    assert_eq!(
        client.record_activity(
            &pool,
            &referee,
            &Symbol::new(&env, "borrow"),
            &200_0000000i128,
            &token,
        ),
        0
    );
}

#[test]
fn test_record_activity_uses_deposit_bps_for_deposit_kind() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, pool) = setup(&env);
    let referee = Address::generate(&env);
    let referrer = Address::generate(&env);
    let token = setup_token(&env);
    client.set_lifetime_reward_cap(&admin, &token, &i128::MAX);
    client.register(&referee, &referrer);

    // Default deposit bps is 1_000 (10%): 10% of 500_0000000 = 50_0000000.
    let reward = client.record_activity(
        &pool,
        &referee,
        &Symbol::new(&env, "deposit"),
        &500_0000000i128,
        &token,
    );
    assert_eq!(reward, 50_0000000i128);
}

#[test]
fn test_admin_can_configure_reward_bps() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, pool) = setup(&env);
    let referee = Address::generate(&env);
    let referrer = Address::generate(&env);
    let token = setup_token(&env);
    client.set_lifetime_reward_cap(&admin, &token, &i128::MAX);
    client.register(&referee, &referrer);

    client.set_borrow_reward_bps(&admin, &1_000u32); // 10%
    assert_eq!(client.get_borrow_reward_bps(), 1_000u32);

    let reward = client.record_activity(
        &pool,
        &referee,
        &Symbol::new(&env, "borrow"),
        &1_000_0000000i128,
        &token,
    );
    assert_eq!(reward, 100_0000000i128);
}

#[test]
fn test_set_reward_bps_above_max_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _pool) = setup(&env);

    let result = client.try_set_borrow_reward_bps(&admin, &10_001u32);
    assert_eq!(
        result,
        Err(Ok::<soroban_sdk::Error, _>(
            ReferralError::InvalidBps.into()
        ))
    );
}

#[test]
fn test_claim_rewards_transfers_token_and_resets_pending() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, pool) = setup(&env);
    let referee = Address::generate(&env);
    let referrer = Address::generate(&env);
    let token = setup_token(&env);
    client.set_lifetime_reward_cap(&admin, &token, &i128::MAX);
    client.register(&referee, &referrer);

    let reward = client.record_activity(
        &pool,
        &referee,
        &Symbol::new(&env, "borrow"),
        &1_000_0000000i128,
        &token,
    );
    // The pool is responsible for funding this contract with the reward it
    // just credited — mirror that here so claim_rewards has funds to pay out.
    mint(&env, &token, &client.address, reward);

    let token_client = token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&referrer), 0);

    let claimed = client.claim_rewards(&referrer, &token);
    assert_eq!(claimed, reward);
    assert_eq!(token_client.balance(&referrer), reward);
    assert_eq!(client.get_pending_reward(&referrer, &token), 0);

    // Claiming again with nothing pending is a harmless no-op.
    assert_eq!(client.claim_rewards(&referrer, &token), 0);
}

#[test]
fn test_get_top_referrers_empty_by_default() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _pool) = setup(&env);

    assert_eq!(client.get_top_referrers(&0).len(), 0);
    assert_eq!(client.get_top_referrers(&5).len(), 0);
}

#[test]
fn test_get_top_referrers_ranks_by_referral_count_descending() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, pool) = setup(&env);
    let token = setup_token(&env);

    let referrer_a = Address::generate(&env);
    let referrer_b = Address::generate(&env);
    let referrer_c = Address::generate(&env);

    // referrer_a: 3 activated referees, referrer_b: 1, referrer_c: 2.
    for _ in 0..3 {
        let referee = Address::generate(&env);
        client.register(&referee, &referrer_a);
        client.record_activity(
            &pool,
            &referee,
            &Symbol::new(&env, "deposit"),
            &0i128,
            &token,
        );
    }
    let referee_b = Address::generate(&env);
    client.register(&referee_b, &referrer_b);
    client.record_activity(
        &pool,
        &referee_b,
        &Symbol::new(&env, "deposit"),
        &0i128,
        &token,
    );

    for _ in 0..2 {
        let referee = Address::generate(&env);
        client.register(&referee, &referrer_c);
        client.record_activity(
            &pool,
            &referee,
            &Symbol::new(&env, "deposit"),
            &0i128,
            &token,
        );
    }

    let board = client.get_top_referrers(&0);
    assert_eq!(board.len(), 3);
    assert_eq!(board.get(0).unwrap().referrer, referrer_a);
    assert_eq!(board.get(0).unwrap().referral_count, 3);
    assert_eq!(board.get(1).unwrap().referrer, referrer_c);
    assert_eq!(board.get(1).unwrap().referral_count, 2);
    assert_eq!(board.get(2).unwrap().referrer, referrer_b);
    assert_eq!(board.get(2).unwrap().referral_count, 1);
}

#[test]
fn test_get_top_referrers_respects_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, pool) = setup(&env);
    let token = setup_token(&env);

    for _ in 0..4 {
        let referrer = Address::generate(&env);
        let referee = Address::generate(&env);
        client.register(&referee, &referrer);
        client.record_activity(
            &pool,
            &referee,
            &Symbol::new(&env, "deposit"),
            &0i128,
            &token,
        );
    }

    assert_eq!(client.get_top_referrers(&2).len(), 2);
    // A limit larger than the tracked set just returns everything tracked.
    assert_eq!(client.get_top_referrers(&100).len(), 4);
}

#[test]
fn test_get_top_referrers_does_not_double_count_repeat_activity() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, pool) = setup(&env);
    let token = setup_token(&env);
    let referee = Address::generate(&env);
    let referrer = Address::generate(&env);
    client.register(&referee, &referrer);

    // Activation (first qualifying activity) plus a second activity from the
    // same already-activated referee must only ever count once.
    client.record_activity(
        &pool,
        &referee,
        &Symbol::new(&env, "borrow"),
        &1_000_0000000i128,
        &token,
    );
    client.record_activity(
        &pool,
        &referee,
        &Symbol::new(&env, "borrow"),
        &500_0000000i128,
        &token,
    );

    let board = client.get_top_referrers(&0);
    assert_eq!(board.len(), 1);
    assert_eq!(board.get(0).unwrap().referral_count, 1);
}

#[test]
fn test_get_top_referrers_evicts_lowest_when_full() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, pool) = setup(&env);
    let token = setup_token(&env);

    // Fill the leaderboard to its MAX_LEADERBOARD_SIZE (25) cap, each with a
    // distinct referral_count so ranking (and eviction) is unambiguous.
    let mut referrers = std::vec::Vec::new();
    for i in 0..25u32 {
        let referrer = Address::generate(&env);
        for _ in 0..=i {
            let referee = Address::generate(&env);
            client.register(&referee, &referrer);
            client.record_activity(
                &pool,
                &referee,
                &Symbol::new(&env, "deposit"),
                &0i128,
                &token,
            );
        }
        referrers.push(referrer);
    }
    // Lowest-ranked tracked referrer so far has referral_count == 1.
    assert_eq!(client.get_top_referrers(&0).len(), 25);
    assert_eq!(
        client.get_top_referrers(&0).get(24).unwrap().referral_count,
        1
    );

    // A brand-new referrer with 2 activated referees beats the current
    // lowest entry (count 1) and should bump it off the board.
    let challenger = Address::generate(&env);
    for _ in 0..2 {
        let referee = Address::generate(&env);
        client.register(&referee, &challenger);
        client.record_activity(
            &pool,
            &referee,
            &Symbol::new(&env, "deposit"),
            &0i128,
            &token,
        );
    }

    let board = client.get_top_referrers(&0);
    assert_eq!(board.len(), 25);
    let mut found_challenger = false;
    let mut found_evicted = false;
    for i in 0..board.len() {
        let entry = board.get(i).unwrap();
        if entry.referrer == challenger {
            found_challenger = true;
        }
        if entry.referrer == referrers.get(0).unwrap().clone() {
            found_evicted = true;
        }
    }
    assert!(found_challenger, "challenger should be on the leaderboard");
    assert!(
        !found_evicted,
        "lowest-ranked referrer (count 1) should have been evicted"
    );
}

#[test]
fn test_pause_blocks_register_and_claim() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _pool) = setup(&env);
    let referee = Address::generate(&env);
    let referrer = Address::generate(&env);
    let token = setup_token(&env);
    client.set_lifetime_reward_cap(&admin, &token, &i128::MAX);

    client.pause(&admin);

    let result = client.try_register(&referee, &referrer);
    assert_eq!(
        result,
        Err(Ok::<soroban_sdk::Error, _>(
            ReferralError::ContractPaused.into()
        ))
    );

    let claim_result = client.try_claim_rewards(&referrer, &token);
    assert_eq!(
        claim_result,
        Err(Ok::<soroban_sdk::Error, _>(
            ReferralError::ContractPaused.into()
        ))
    );
}

#[test]
fn test_register_rejects_two_party_referral_cycle() {
    // #1348: register() rejects a two-party cycle — once A names B as their
    // referrer, B cannot name A back, since a mutual A->B, B->A pair would
    // let rewards accrue in both directions for the same cycle.
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _pool) = setup(&env);
    let party_a = Address::generate(&env);
    let party_b = Address::generate(&env);

    client.register(&party_a, &party_b);

    let result = client.try_register(&party_b, &party_a);
    assert_eq!(
        result,
        Err(Ok::<soroban_sdk::Error, _>(ReferralError::ReferralCycle.into()))
    );

    // The cycle was rejected, so B keeps no referrer and the first
    // registration is untouched.
    assert_eq!(client.get_referrer(&party_a), Some(party_b.clone()));
    assert_eq!(client.get_referrer(&party_b), None);
}

#[test]
fn test_record_activity_rejects_unrecognised_kind() {
    // #1404: only "borrow" and "deposit" are valid activity kinds. Any other
    // value — including a typo — is rejected with InvalidActivityKind instead
    // of silently pricing at the (2x larger) deposit rate.
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, pool) = setup(&env);
    let referee = Address::generate(&env);
    let referrer = Address::generate(&env);
    let token = setup_token(&env);
    client.set_lifetime_reward_cap(&admin, &token, &i128::MAX);
    client.register(&referee, &referrer);

    let result = client.try_record_activity(
        &pool,
        &referee,
        &Symbol::new(&env, "unknown"),
        &1_000_0000000i128,
        &token,
    );
    assert_eq!(
        result,
        Err(Ok::<soroban_sdk::Error, _>(
            ReferralError::InvalidActivityKind.into()
        ))
    );
}
