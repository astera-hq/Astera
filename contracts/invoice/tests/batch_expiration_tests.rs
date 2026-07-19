#![cfg(test)]

use invoice::{InvoiceContract, InvoiceContractClient, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec,
};

fn setup(env: &Env) -> (InvoiceContractClient, Address, Address) {
    let contract_id = env.register(InvoiceContract, ());
    let client = InvoiceContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let pool = Address::generate(env);
    client.initialize(&admin, &pool, &i128::MAX, &2_592_000u64, &7u32);
    (client, admin, pool)
}

#[test]
fn test_batch_check_expiration_mixed_invoices() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, _pool) = setup(&env);
    let sme = Address::generate(&env);

    // Create 3 expired invoices
    let mut ids: Vec<u64> = Vec::new(&env);

    // Create invoice 1 (will be expired)
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);
    let id1 = client.create_invoice(
        &sme,
        &String::from_str(&env, "Debtor1"),
        &1_000_000i128,
        &(1_000_000 + 86_400),
        &String::from_str(&env, "desc1"),
        &String::from_str(&env, "hash1"),
        &String::from_str(&env, "https://example.com/meta1"),
    );
    ids.push_back(id1);

    // Create invoice 2 (will be expired)
    let id2 = client.create_invoice(
        &sme,
        &String::from_str(&env, "Debtor2"),
        &2_000_000i128,
        &(1_000_000 + 86_400),
        &String::from_str(&env, "desc2"),
        &String::from_str(&env, "hash2"),
        &String::from_str(&env, "https://example.com/meta2"),
    );
    ids.push_back(id2);

    // Create invoice 3 (will be expired)
    let id3 = client.create_invoice(
        &sme,
        &String::from_str(&env, "Debtor3"),
        &3_000_000i128,
        &(1_000_000 + 86_400),
        &String::from_str(&env, "desc3"),
        &String::from_str(&env, "hash3"),
        &String::from_str(&env, "https://example.com/meta3"),
    );
    ids.push_back(id3);

    // Move time past expiration (30 days + 1 second)
    env.ledger()
        .with_mut(|l| l.timestamp = 1_000_000 + 2_592_001);

    // Create 2 active invoices
    let id4 = client.create_invoice(
        &sme,
        &String::from_str(&env, "Debtor4"),
        &4_000_000i128,
        &(1_000_000 + 2_592_001 + 86_400),
        &String::from_str(&env, "desc4"),
        &String::from_str(&env, "hash4"),
        &String::from_str(&env, "https://example.com/meta4"),
    );
    ids.push_back(id4);

    let id5 = client.create_invoice(
        &sme,
        &String::from_str(&env, "Debtor5"),
        &5_000_000i128,
        &(1_000_000 + 2_592_001 + 86_400),
        &String::from_str(&env, "desc5"),
        &String::from_str(&env, "hash5"),
        &String::from_str(&env, "https://example.com/meta5"),
    );
    ids.push_back(id5);

    // Check expiration count
    let expired_count = client.batch_check_expiration(&ids);
    assert_eq!(expired_count, 3u32);
}

#[test]
fn test_batch_check_expiration_empty_list() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _pool) = setup(&env);

    // Check expiration with empty list
    let ids: Vec<u64> = Vec::new(&env);
    let expired_count = client.batch_check_expiration(&ids);
    assert_eq!(expired_count, 0u32);
}

#[test]
fn test_batch_check_expiration_repeated_calls_no_double_count() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, _pool) = setup(&env);
    let sme = Address::generate(&env);

    // Create 2 invoices that will expire
    let mut ids: Vec<u64> = Vec::new(&env);

    let id1 = client.create_invoice(
        &sme,
        &String::from_str(&env, "Debtor1"),
        &1_000_000i128,
        &(1_000_000 + 86_400),
        &String::from_str(&env, "desc1"),
        &String::from_str(&env, "hash1"),
        &String::from_str(&env, "https://example.com/meta1"),
    );
    ids.push_back(id1);

    let id2 = client.create_invoice(
        &sme,
        &String::from_str(&env, "Debtor2"),
        &2_000_000i128,
        &(1_000_000 + 86_400),
        &String::from_str(&env, "desc2"),
        &String::from_str(&env, "hash2"),
        &String::from_str(&env, "https://example.com/meta2"),
    );
    ids.push_back(id2);

    // Move time past expiration
    env.ledger()
        .with_mut(|l| l.timestamp = 1_000_000 + 2_592_001);

    // First call
    let count1 = client.batch_check_expiration(&ids);
    assert_eq!(count1, 2u32);

    // check_expiration only counts invoices it *just* transitioned out of
    // Pending this call (it early-returns false once status != Pending), so
    // repeated calls on already-expired invoices must report 0 new
    // transitions, not re-count the same 2 invoices again.
    let count2 = client.batch_check_expiration(&ids);
    assert_eq!(count2, 0u32);

    // Third call should still report 0 new transitions.
    let count3 = client.batch_check_expiration(&ids);
    assert_eq!(count3, 0u32);

    // The invoices themselves remain Expired (not reverted or double-processed).
    let inv1 = client.get_invoice(&id1);
    let inv2 = client.get_invoice(&id2);
    assert_eq!(inv1.status, InvoiceStatus::Expired);
    assert_eq!(inv2.status, InvoiceStatus::Expired);
}

#[test]
fn test_batch_check_expiration_all_active() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, _pool) = setup(&env);
    let sme = Address::generate(&env);

    // Create 5 active invoices
    let mut ids: Vec<u64> = Vec::new(&env);
    for i in 1..=5 {
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, &format!("Debtor{}", i)),
            &(i as i128 * 1_000_000),
            &(1_000_000 + 86_400),
            &String::from_str(&env, &format!("desc{}", i)),
            &String::from_str(&env, &format!("hash{}", i)),
            &String::from_str(&env, "https://example.com/meta"),
        );
        ids.push_back(id);
    }

    // Check expiration immediately (all should be active)
    let expired_count = client.batch_check_expiration(&ids);
    assert_eq!(expired_count, 0u32);
}

#[test]
fn test_batch_check_expiration_all_expired() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, _pool) = setup(&env);
    let sme = Address::generate(&env);

    // Create 5 invoices
    let mut ids: Vec<u64> = Vec::new(&env);
    for i in 1..=5 {
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, &format!("Debtor{}", i)),
            &(i as i128 * 1_000_000),
            &(1_000_000 + 86_400),
            &String::from_str(&env, &format!("desc{}", i)),
            &String::from_str(&env, &format!("hash{}", i)),
            &String::from_str(&env, "https://example.com/meta"),
        );
        ids.push_back(id);
    }

    // Move time past expiration for all
    env.ledger()
        .with_mut(|l| l.timestamp = 1_000_000 + 2_592_001);

    // Check expiration (all 5 should be expired)
    let expired_count = client.batch_check_expiration(&ids);
    assert_eq!(expired_count, 5u32);
}
