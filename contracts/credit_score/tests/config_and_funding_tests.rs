#![cfg(test)]

// Tests for #1680: missing coverage for record_funding, list_all_sme_stats,
// and set_pool_contract entrypoints.

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

use credit_score::CreditScoreContract;
use credit_score::CreditScoreContractClient;

fn setup(env: &Env) -> (CreditScoreContractClient<'_>, Address, Address, Address) {
    let contract_id = env.register(CreditScoreContract, ());
    let client = CreditScoreContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let invoice_contract = Address::generate(env);
    let pool_contract = Address::generate(env);
    client.initialize(&admin, &invoice_contract, &pool_contract);
    (client, admin, invoice_contract, pool_contract)
}

#[test]
fn test_record_funding_updates_volume_and_is_idempotent_1680() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _invoice, pool) = setup(&env);

    let sme = Address::generate(&env);
    let invoice_id = 100u64;
    let funding_amount = 500_000_000i128;

    // Before any funding, SME has no history
    let initial = client.get_credit_score(&sme);
    assert_eq!(initial.total_volume, 0);
    assert_eq!(initial.total_invoices, 0);

    // Record a funding event
    client.record_funding(&pool, &invoice_id, &sme, &funding_amount);

    let after_funding = client.get_credit_score(&sme);
    assert_eq!(after_funding.total_volume, funding_amount);
    assert_eq!(after_funding.total_invoices, 0); // funding doesn't count as an invoice

    // Record the same funding again (idempotent)
    client.record_funding(&pool, &invoice_id, &sme, &funding_amount);

    let after_dupe = client.get_credit_score(&sme);
    // Volume should not double since it's idempotent
    assert_eq!(after_dupe.total_volume, funding_amount);

    // Record a payment for the same invoice marks it processed
    let due_date = 500_000u64;
    let paid_at = 500_000u64;
    client.record_payment(&pool, &invoice_id, &sme, &funding_amount, &due_date, &paid_at);

    let after_payment = client.get_credit_score(&sme);
    assert_eq!(after_payment.total_invoices, 1);
    assert_eq!(after_payment.total_volume, funding_amount);

    // Further funding attempts for this invoice should be ignored
    // (invoice already processed)
    client.record_funding(&pool, &invoice_id, &sme, &funding_amount);
    let final_state = client.get_credit_score(&sme);
    assert_eq!(final_state.total_volume, funding_amount);
}

#[test]
fn test_record_funding_requires_pool_auth_1680() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _invoice, pool) = setup(&env);

    let sme = Address::generate(&env);
    let impostor = Address::generate(&env);
    let invoice_id = 101u64;
    let amount = 1_000_000i128;

    // Non-pool caller should be rejected
    let result = client.try_record_funding(&impostor, &invoice_id, &sme, &amount);
    assert!(result.is_err());
}

#[test]
fn test_list_all_sme_stats_pagination_1680() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _invoice, pool) = setup(&env);

    // Create stats for 15 unique SMEs
    for i in 0..15u64 {
        let sme = Address::generate(&env);
        let amount = 100_000_000i128 + (i as i128);
        client.record_funding(&pool, &i, &sme, &amount);
    }

    // Page 0 with size 10 should return 10 entries
    let page0 = client.list_all_sme_stats(&0u32, &10u32);
    assert_eq!(page0.len(), 10);

    // Page 1 with size 10 should return the remaining 5
    let page1 = client.list_all_sme_stats(&1u32, &10u32);
    assert_eq!(page1.len(), 5);

    // Page 2 should be empty
    let page2 = client.list_all_sme_stats(&2u32, &10u32);
    assert_eq!(page2.len(), 0);

    // Page size > MAX_SME_PAGE_SIZE should be clamped
    let large_page = client.list_all_sme_stats(&0u32, &10_000u32);
    // Should be at most 100 (the max per page), but may be less
    assert!(large_page.len() <= 100);
    assert!(large_page.len() > 0);

    // Each entry should have a score and address
    for entry in page0.iter() {
        assert!(entry.score >= 200); // MIN_SCORE
        assert!(entry.score <= 850); // MAX_SCORE
    }
}

#[test]
fn test_list_all_sme_stats_includes_funding_volume_1680() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _invoice, pool) = setup(&env);

    let sme1 = Address::generate(&env);
    let sme2 = Address::generate(&env);

    // Record funding for two different SMEs
    client.record_funding(&pool, &1u64, &sme1, &500_000_000i128);
    client.record_funding(&pool, &2u64, &sme2, &300_000_000i128);

    let list = client.list_all_sme_stats(&0u32, &100u32);
    assert!(list.len() >= 2);

    // Find our SMEs in the list
    let found_sme1 = list.iter().find(|e| e.sme == sme1);
    let found_sme2 = list.iter().find(|e| e.sme == sme2);

    assert!(found_sme1.is_some());
    assert!(found_sme2.is_some());

    // Both should have non-MIN_SCORE since they have funding/volume
    assert!(found_sme1.unwrap().score > 200);
    assert!(found_sme2.unwrap().score > 200);
}

#[test]
fn test_set_pool_contract_updates_and_requires_admin_1680() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _invoice, pool) = setup(&env);

    // Admin can set a new pool contract
    let new_pool = Address::generate(&env);
    client.set_pool_contract(&admin, &new_pool);

    // Verify the new pool is now configured
    let (_, _, configured_pool) = client.get_config();
    assert_eq!(configured_pool, new_pool);

    // Non-admin cannot set the pool contract
    let impostor = Address::generate(&env);
    let another_pool = Address::generate(&env);
    let result = client.try_set_pool_contract(&impostor, &another_pool);
    assert!(result.is_err());

    // Pool should still be the one the admin set
    let (_, _, still_new) = client.get_config();
    assert_eq!(still_new, new_pool);
}

#[test]
fn test_set_pool_contract_affects_authorization_1680() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _invoice, old_pool) = setup(&env);

    let sme = Address::generate(&env);
    let new_pool = Address::generate(&env);

    // Record funding with the original pool contract
    client.record_funding(&old_pool, &1u64, &sme, &100_000_000i128);
    let initial = client.get_credit_score(&sme);
    assert!(initial.total_volume > 0);

    // Switch the pool contract
    client.set_pool_contract(&admin, &new_pool);

    // The old pool should no longer be able to record operations
    let result = client.try_record_funding(&old_pool, &2u64, &sme, &200_000_000i128);
    assert!(result.is_err());

    // The new pool should be able to
    client.record_funding(&new_pool, &2u64, &sme, &200_000_000i128);
    let after_switch = client.get_credit_score(&sme);
    assert_eq!(after_switch.total_volume, 300_000_000i128);
}
