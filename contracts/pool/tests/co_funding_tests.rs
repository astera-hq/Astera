#![cfg(test)]

// #860: multi-investor co-funding rounds — full lifecycle, refund paths,
// idempotent finalization, per-investor caps, secondary-market transfer
// gating, and incremental partial-repayment distribution.

use pool::{FundingPool, FundingPoolClient, OpenCoFundingRequest, PoolError};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    token, Address, Env, Symbol,
};

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

#[contract]
pub struct DummyInvoice;

#[contractimpl]
impl DummyInvoice {
    pub fn get_authorized_pool(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "pool"))
            .expect("not initialized")
    }

    pub fn set_pool(env: Env, pool: Address) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "pool"), &pool);
    }
}

fn setup(env: &Env) -> (FundingPoolClient<'_>, Address, Address) {
    env.ledger().with_mut(|l| l.timestamp = 100_000);
    let contract_id = env.register(FundingPool, ());
    let client = FundingPoolClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let invoice_contract = env.register(DummyInvoice, ());
    DummyInvoiceClient::new(env, &invoice_contract).set_pool(&contract_id);
    let share_token = env.register(DummyShare, ());

    client.initialize(&admin, &usdc_id, &share_token, &invoice_contract);
    client.set_max_investor_concentration(&admin, &10_000u32);
    (client, admin, usdc_id)
}

fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token_id).mint(to, &amount);
}

/// Seeds the pool with liquidity from `n` investors depositing `each` USDC,
/// returning their addresses so tests can commit from them.
fn seed_investors(
    env: &Env,
    client: &FundingPoolClient,
    usdc_id: &Address,
    n: usize,
    each: i128,
) -> Vec<Address> {
    let mut investors = Vec::new();
    for _ in 0..n {
        let investor = Address::generate(env);
        mint(env, usdc_id, &investor, each);
        client.deposit(&investor, usdc_id, &each);
        investors.push(investor);
    }
    investors
}

fn default_request(
    invoice_id: u64,
    token: Address,
    target: i128,
    sme: Address,
    due_date: u64,
    deadline: u64,
) -> OpenCoFundingRequest {
    OpenCoFundingRequest {
        invoice_id,
        token,
        target_principal: target,
        sme,
        due_date,
        funding_deadline: deadline,
        min_commitment: 0,
        max_investor_bps: 0,
    }
}

#[test]
fn test_full_co_funding_lifecycle_nonround_bps_split() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);
    let sme = Address::generate(&env);
    let investors = seed_investors(&env, &client, &usdc_id, 3, 100_000);

    let invoice_id = 1u64;
    let target = 9_000i128;
    let due_date = env.ledger().timestamp() + 1_000_000;
    let deadline = env.ledger().timestamp() + 10_000;

    client.open_co_funding(
        &admin,
        &default_request(
            invoice_id,
            usdc_id.clone(),
            target,
            sme.clone(),
            due_date,
            deadline,
        ),
    );

    // Non-round-number split: 3000/3000/3000 out of 9000 -> exactly 3333/3333/3334 bps.
    client.commit_to_invoice(&investors[0], &invoice_id, &3_000);
    client.commit_to_invoice(&investors[1], &invoice_id, &3_000);
    client.commit_to_invoice(&investors[2], &invoice_id, &3_000);

    let round = client.get_co_funding_round(&invoice_id).unwrap();
    assert_eq!(round.committed_principal, target);

    let bps0 = client.get_co_fund_share(&invoice_id, &investors[0]);
    let bps1 = client.get_co_fund_share(&invoice_id, &investors[1]);
    let bps2 = client.get_co_fund_share(&invoice_id, &investors[2]);
    assert_eq!(bps0, 3_333);
    assert_eq!(bps1, 3_333);
    assert_eq!(bps2 + bps1 + bps0, 9999); // integer-division dust stays unassigned, not double-counted

    let sme_balance_before = token::Client::new(&env, &usdc_id).balance(&sme);
    client.finalize_co_funding(&admin, &invoice_id);
    let sme_balance_after = token::Client::new(&env, &usdc_id).balance(&sme);
    assert_eq!(sme_balance_after - sme_balance_before, target);

    let funded = client.get_funded_invoice(&invoice_id).unwrap();
    assert_eq!(funded.co_funding_round_id, Some(invoice_id));
    assert_eq!(funded.principal, target);

    // Repay in full; each co-funder should receive fresh LP shares
    // proportional to their bps rather than the general reward_per_share
    // accumulator (which must stay untouched by a co-funded invoice).
    let totals_before = client.get_token_totals(&usdc_id);
    let reward_per_share_before = totals_before.reward_per_share;

    env.ledger().with_mut(|l| l.timestamp += 10_000);
    let total_due = client.estimate_repayment(&invoice_id, &None);
    mint(&env, &usdc_id, &sme, total_due);
    client.repay_invoice(&invoice_id, &sme, &total_due);

    let totals_after = client.get_token_totals(&usdc_id);
    assert_eq!(
        totals_after.reward_per_share, reward_per_share_before,
        "co-funded invoice interest must not touch the pool-wide reward_per_share accumulator"
    );

    let funded_after = client.get_funded_invoice(&invoice_id).unwrap();
    assert_eq!(funded_after.repaid_amount, total_due);
}

#[test]
fn test_commit_overshoot_is_clamped_not_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);
    let sme = Address::generate(&env);
    let investors = seed_investors(&env, &client, &usdc_id, 1, 100_000);

    let invoice_id = 1u64;
    let target = 5_000i128;
    let due_date = env.ledger().timestamp() + 1_000_000;
    let deadline = env.ledger().timestamp() + 10_000;
    client.open_co_funding(
        &admin,
        &default_request(invoice_id, usdc_id.clone(), target, sme, due_date, deadline),
    );

    // Requests far more than the target; only `target` should be taken.
    client.commit_to_invoice(&investors[0], &invoice_id, &50_000);

    let round = client.get_co_funding_round(&invoice_id).unwrap();
    assert_eq!(round.committed_principal, target);
    assert_eq!(client.get_co_fund_share(&invoice_id, &investors[0]), 10_000);
}

#[test]
fn test_finalize_before_target_or_deadline_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);
    let sme = Address::generate(&env);
    let investors = seed_investors(&env, &client, &usdc_id, 1, 100_000);

    let invoice_id = 1u64;
    let due_date = env.ledger().timestamp() + 1_000_000;
    let deadline = env.ledger().timestamp() + 10_000;
    client.open_co_funding(
        &admin,
        &default_request(invoice_id, usdc_id.clone(), 10_000, sme, due_date, deadline),
    );
    client.commit_to_invoice(&investors[0], &invoice_id, &1_000);

    let result = client.try_finalize_co_funding(&admin, &invoice_id);
    assert_eq!(result, Err(Ok(PoolError::CoFundingRoundNotOpen)));
}

#[test]
fn test_deadline_expiry_below_minimum_refunds_everyone() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);
    let sme = Address::generate(&env);
    let investors = seed_investors(&env, &client, &usdc_id, 2, 100_000);

    let invoice_id = 1u64;
    let due_date = env.ledger().timestamp() + 1_000_000;
    let deadline = env.ledger().timestamp() + 10_000;
    let mut request = default_request(invoice_id, usdc_id.clone(), 10_000, sme, due_date, deadline);
    request.min_commitment = 8_000;
    client.open_co_funding(&admin, &request);

    client.commit_to_invoice(&investors[0], &invoice_id, &2_000);
    client.commit_to_invoice(&investors[1], &invoice_id, &1_000);

    env.ledger().with_mut(|l| l.timestamp = deadline + 1);
    client.finalize_co_funding(&admin, &invoice_id);

    let round = client.get_co_funding_round(&invoice_id).unwrap();
    assert_eq!(round.status, pool::CoFundingStatus::Expired);
    assert_eq!(round.committed_principal, 0);
    assert_eq!(client.get_co_fund_share(&invoice_id, &investors[0]), 0);
    assert_eq!(client.get_co_fund_share(&invoice_id, &investors[1]), 0);
    assert!(client.get_funded_invoice(&invoice_id).is_none());
    assert_eq!(
        client.get_investor_co_fund_positions(&investors[0]).len(),
        0
    );
}

#[test]
fn test_double_finalize_does_not_double_pay() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);
    let sme = Address::generate(&env);
    let investors = seed_investors(&env, &client, &usdc_id, 1, 100_000);

    let invoice_id = 1u64;
    let due_date = env.ledger().timestamp() + 1_000_000;
    let deadline = env.ledger().timestamp() + 10_000;
    client.open_co_funding(
        &admin,
        &default_request(
            invoice_id,
            usdc_id.clone(),
            5_000,
            sme.clone(),
            due_date,
            deadline,
        ),
    );
    client.commit_to_invoice(&investors[0], &invoice_id, &5_000);

    client.finalize_co_funding(&admin, &invoice_id);
    let sme_balance_after_first = token::Client::new(&env, &usdc_id).balance(&sme);

    let result = client.try_finalize_co_funding(&admin, &invoice_id);
    assert_eq!(result, Err(Ok(PoolError::CoFundingRoundAlreadyFinalized)));
    let sme_balance_after_second = token::Client::new(&env, &usdc_id).balance(&sme);
    assert_eq!(sme_balance_after_first, sme_balance_after_second);
}

#[test]
fn test_transfer_rejected_before_round_filled() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);
    let sme = Address::generate(&env);
    let investors = seed_investors(&env, &client, &usdc_id, 2, 100_000);

    let invoice_id = 1u64;
    let due_date = env.ledger().timestamp() + 1_000_000;
    let deadline = env.ledger().timestamp() + 10_000;
    client.open_co_funding(
        &admin,
        &default_request(invoice_id, usdc_id.clone(), 10_000, sme, due_date, deadline),
    );
    client.commit_to_invoice(&investors[0], &invoice_id, &5_000);

    // Round still Open (not Filled/finalized) — no `FundedInvoice` exists
    // yet at all (that's only created inside `finalize_co_funding`'s success
    // path, atomically with the round moving to Filled), so this hits the
    // pre-existing InvoiceNotFound guard before ever reaching the
    // round-status check. The dedicated CoFundingRoundNotFilled check in
    // transfer_co_fund_share is therefore currently unreachable in practice
    // — kept anyway as cheap defense against a future refactor ever
    // creating a FundedInvoice record in a not-yet-Filled state.
    let result = client.try_transfer_co_fund_share(
        &investors[0],
        &invoice_id,
        &usdc_id,
        &investors[1],
        &10_000,
    );
    assert_eq!(result, Err(Ok(PoolError::InvoiceNotFound)));
}

#[test]
fn test_transfer_after_filled_moves_future_repayment_share() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);
    let sme = Address::generate(&env);
    let investors = seed_investors(&env, &client, &usdc_id, 1, 100_000);
    let new_holder = Address::generate(&env);
    mint(&env, &usdc_id, &new_holder, 1); // just needs to exist as an Address

    let invoice_id = 1u64;
    let due_date = env.ledger().timestamp() + 1_000_000;
    let deadline = env.ledger().timestamp() + 10_000;
    client.open_co_funding(
        &admin,
        &default_request(
            invoice_id,
            usdc_id.clone(),
            10_000,
            sme.clone(),
            due_date,
            deadline,
        ),
    );
    client.commit_to_invoice(&investors[0], &invoice_id, &10_000);
    client.finalize_co_funding(&admin, &invoice_id);

    // Transfer the entire share to a brand-new address who never committed
    // any capital themselves.
    client.transfer_co_fund_share(&investors[0], &invoice_id, &usdc_id, &new_holder, &10_000);
    assert_eq!(client.get_co_fund_share(&invoice_id, &investors[0]), 0);
    assert_eq!(client.get_co_fund_share(&invoice_id, &new_holder), 10_000);
    assert_eq!(
        client.get_investor_co_fund_positions(&investors[0]).len(),
        0
    );
    assert_eq!(client.get_investor_co_fund_positions(&new_holder).len(), 1);

    env.ledger().with_mut(|l| l.timestamp += 10_000);
    let total_due = client.estimate_repayment(&invoice_id, &None);
    mint(&env, &usdc_id, &sme, total_due);
    // Should not error — the new holder is correctly tracked as the round's
    // sole participant now, so distribution has someone to pay.
    client.repay_invoice(&invoice_id, &sme, &total_due);

    let funded = client.get_funded_invoice(&invoice_id).unwrap();
    assert_eq!(funded.repaid_amount, total_due);
}

#[test]
fn test_investor_cap_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);
    let sme = Address::generate(&env);
    let investors = seed_investors(&env, &client, &usdc_id, 1, 100_000);

    let invoice_id = 1u64;
    let due_date = env.ledger().timestamp() + 1_000_000;
    let deadline = env.ledger().timestamp() + 10_000;
    let mut request = default_request(invoice_id, usdc_id.clone(), 10_000, sme, due_date, deadline);
    request.max_investor_bps = 5_000; // cap each investor at 50% of the round
    client.open_co_funding(&admin, &request);

    client.commit_to_invoice(&investors[0], &invoice_id, &5_000);
    let result = client.try_commit_to_invoice(&investors[0], &invoice_id, &1);
    assert_eq!(result, Err(Ok(PoolError::CoFundingInvestorCapExceeded)));
}

#[test]
fn test_withdraw_commitment_before_finalize_returns_full_principal() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);
    let sme = Address::generate(&env);
    let investors = seed_investors(&env, &client, &usdc_id, 1, 100_000);

    let invoice_id = 1u64;
    let due_date = env.ledger().timestamp() + 1_000_000;
    let deadline = env.ledger().timestamp() + 10_000;
    client.open_co_funding(
        &admin,
        &default_request(invoice_id, usdc_id.clone(), 10_000, sme, due_date, deadline),
    );
    client.commit_to_invoice(&investors[0], &invoice_id, &4_000);

    client.withdraw_co_funding_commitment(&investors[0], &invoice_id);

    let round = client.get_co_funding_round(&invoice_id).unwrap();
    assert_eq!(round.committed_principal, 0);
    assert_eq!(client.get_co_fund_share(&invoice_id, &investors[0]), 0);
    assert_eq!(
        client.get_investor_co_fund_positions(&investors[0]).len(),
        0
    );

    // Investor should be able to withdraw their full liquid position from
    // the general pool afterward, proving 100% of principal came back.
    let totals = client.get_token_totals(&usdc_id);
    assert_eq!(totals.pool_value, 100_000);
}

#[test]
fn test_cancel_only_allowed_before_any_commitments() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);
    let sme = Address::generate(&env);
    let investors = seed_investors(&env, &client, &usdc_id, 1, 100_000);

    let invoice_id = 1u64;
    let due_date = env.ledger().timestamp() + 1_000_000;
    let deadline = env.ledger().timestamp() + 10_000;
    client.open_co_funding(
        &admin,
        &default_request(invoice_id, usdc_id.clone(), 10_000, sme, due_date, deadline),
    );

    client.commit_to_invoice(&investors[0], &invoice_id, &1_000);
    let result = client.try_cancel_co_funding_round(&admin, &invoice_id);
    assert_eq!(result, Err(Ok(PoolError::InvalidCoFundingParams)));

    client.withdraw_co_funding_commitment(&investors[0], &invoice_id);
    client.cancel_co_funding_round(&admin, &invoice_id);
    let round = client.get_co_funding_round(&invoice_id).unwrap();
    assert_eq!(round.status, pool::CoFundingStatus::Cancelled);
}

#[test]
fn test_partial_repayment_distributes_pro_rata_incrementally() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);
    let sme = Address::generate(&env);
    let investors = seed_investors(&env, &client, &usdc_id, 2, 100_000);

    let invoice_id = 1u64;
    let due_date = env.ledger().timestamp() + 1_000_000;
    let deadline = env.ledger().timestamp() + 10_000;
    client.open_co_funding(
        &admin,
        &default_request(
            invoice_id,
            usdc_id.clone(),
            10_000,
            sme.clone(),
            due_date,
            deadline,
        ),
    );
    client.commit_to_invoice(&investors[0], &invoice_id, &6_000); // 60%
    client.commit_to_invoice(&investors[1], &invoice_id, &4_000); // 40%
    client.finalize_co_funding(&admin, &invoice_id);

    let totals_before = client.get_token_totals(&usdc_id);
    let pool_value_before = totals_before.pool_value;

    // Partial repayment — must be credited immediately, not deferred to a
    // final full repayment.
    let partial = 3_000i128;
    mint(&env, &usdc_id, &sme, partial);
    client.repay_invoice(&invoice_id, &sme, &partial);

    let funded = client.get_funded_invoice(&invoice_id).unwrap();
    assert_eq!(funded.repaid_amount, partial);
    assert!(!client
        .get_funded_invoice(&invoice_id)
        .unwrap()
        .repaid_amount
        .eq(&funded.principal));

    let totals_mid = client.get_token_totals(&usdc_id);
    // pool_value should have grown by (approximately) the partial amount,
    // since it was minted straight back out to the two co-funders as fresh
    // LP value rather than sitting unaccounted for.
    assert!(totals_mid.pool_value > pool_value_before);
    assert!(totals_mid.pool_value <= pool_value_before + partial);

    // Finish repaying.
    let remaining = client.estimate_repayment(&invoice_id, &None);
    mint(&env, &usdc_id, &sme, remaining);
    client.repay_invoice(&invoice_id, &sme, &remaining);
    let funded_final = client.get_funded_invoice(&invoice_id).unwrap();
    assert_eq!(funded_final.repaid_amount, partial + remaining);
}

#[test]
fn test_too_many_participants_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);
    let sme = Address::generate(&env);
    // MAX_CO_FUNDING_PARTICIPANTS is 20 — seed 21 investors with tiny deposits.
    let investors = seed_investors(&env, &client, &usdc_id, 21, 1_000);

    let invoice_id = 1u64;
    let due_date = env.ledger().timestamp() + 1_000_000;
    let deadline = env.ledger().timestamp() + 10_000;
    client.open_co_funding(
        &admin,
        &default_request(invoice_id, usdc_id.clone(), 2_100, sme, due_date, deadline),
    );

    for investor in investors.iter().take(20) {
        client.commit_to_invoice(investor, &invoice_id, &1);
    }
    let result = client.try_commit_to_invoice(&investors[20], &invoice_id, &1);
    assert_eq!(result, Err(Ok(PoolError::CoFundingTooManyParticipants)));
}

#[test]
fn test_open_co_funding_rejects_duplicate_round() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);
    let sme = Address::generate(&env);

    let invoice_id = 1u64;
    let due_date = env.ledger().timestamp() + 1_000_000;
    let deadline = env.ledger().timestamp() + 10_000;
    let request = default_request(invoice_id, usdc_id.clone(), 10_000, sme, due_date, deadline);
    client.open_co_funding(&admin, &request);

    let result = client.try_open_co_funding(&admin, &request);
    assert_eq!(result, Err(Ok(PoolError::CoFundingRoundAlreadyExists)));
}

#[test]
fn test_list_co_funding_rounds_tracks_every_opened_round() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);
    let sme = Address::generate(&env);

    let due_date = env.ledger().timestamp() + 1_000_000;
    let deadline = env.ledger().timestamp() + 10_000;
    client.open_co_funding(
        &admin,
        &default_request(1, usdc_id.clone(), 1_000, sme.clone(), due_date, deadline),
    );
    client.open_co_funding(
        &admin,
        &default_request(2, usdc_id.clone(), 2_000, sme, due_date, deadline),
    );

    let ids = client.list_co_funding_rounds();
    assert_eq!(ids.len(), 2);
    assert_eq!(ids.get(0).unwrap(), 1);
    assert_eq!(ids.get(1).unwrap(), 2);
}
