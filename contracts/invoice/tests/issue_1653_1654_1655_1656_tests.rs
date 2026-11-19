#![cfg(test)]

use invoice::{InvoiceContract, InvoiceContractClient, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec,
};

fn setup(env: &Env) -> (InvoiceContractClient<'_>, Address, Address, Address) {
    let contract_id = env.register(InvoiceContract, ());
    let client = InvoiceContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let pool = Address::generate(env);
    let sme = Address::generate(env);
    client.initialize(&admin, &pool, &i128::MAX, &2_592_000u64, &7u32);
    (client, admin, pool, sme)
}

// Issue #1653: Verified invoices never expire
#[test]
fn test_verified_invoices_expire() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, _pool, sme) = setup(&env);

    // Create invoice that will be verified
    let id = client.create_invoice(
        &sme,
        &String::from_str(&env, "Debtor1"),
        &1_000_000i128,
        &(1_000_000 + 86_400),
        &String::from_str(&env, "desc1"),
        &String::from_str(&env, "hash1"),
        &String::from_str(&env, "https://example.com/meta1"),
    );

    // Verify the invoice
    client.verify_invoice(&id, &true, &String::from_str(&env, ""), &String::from_str(&env, "hash1"));

    let inv = client.get_invoice(&id);
    assert_eq!(inv.status, InvoiceStatus::Verified);

    // Move time past expiration (30 days + 1 second)
    env.ledger()
        .with_mut(|l| l.timestamp = 1_000_000 + 2_592_001);

    // Check expiration — verified invoice should expire
    let expired = client.check_expiration(&id);
    assert!(expired, "Verified invoice should expire");

    // Invoice should now be Expired
    let inv = client.get_invoice(&id);
    assert_eq!(inv.status, InvoiceStatus::Expired);
}

// Issue #1654: invoice expiry removes the owner index entry but leaves the debtor index entry
#[test]
fn test_expiry_removes_both_index_entries() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, _pool, sme) = setup(&env);
    let debtor_name = String::from_str(&env, "Debtor1");

    // Create invoice
    let id = client.create_invoice(
        &sme,
        &debtor_name,
        &1_000_000i128,
        &(1_000_000 + 86_400),
        &String::from_str(&env, "desc1"),
        &String::from_str(&env, "hash1"),
        &String::from_str(&env, "https://example.com/meta1"),
    );

    // Get SME invoices before expiry
    let sme_invoices_before = client.get_sme_invoices(&sme);
    assert!(sme_invoices_before.contains(&id), "Invoice should be in SME invoices");

    // Get debtor invoices before expiry
    let debtor_invoices_before = client.get_debtor_invoices(&debtor_name);
    assert!(debtor_invoices_before.contains(&id), "Invoice should be in debtor invoices");

    // Move time past expiration
    env.ledger()
        .with_mut(|l| l.timestamp = 1_000_000 + 2_592_001);

    // Check expiration
    client.check_expiration(&id);

    // Get SME invoices after expiry
    let sme_invoices_after = client.get_sme_invoices(&sme);
    assert!(!sme_invoices_after.contains(&id), "Invoice should be removed from SME invoices");

    // Get debtor invoices after expiry
    let debtor_invoices_after = client.get_debtor_invoices(&debtor_name);
    assert!(!debtor_invoices_after.contains(&id), "Invoice should be removed from debtor invoices");
}

// Issue #1655: record_funding doesn't cap funded_amount or apply expiry like mark_funded does
#[test]
fn test_record_funding_applies_expiry() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, pool, sme) = setup(&env);

    // Create invoice
    let id = client.create_invoice(
        &sme,
        &String::from_str(&env, "Debtor1"),
        &1_000_000i128,
        &(1_000_000 + 86_400),
        &String::from_str(&env, "desc1"),
        &String::from_str(&env, "hash1"),
        &String::from_str(&env, "https://example.com/meta1"),
    );

    // Move time past expiration (30 days + 1 second)
    env.ledger()
        .with_mut(|l| l.timestamp = 1_000_000 + 2_592_001);

    // Try to record funding on expired invoice — should fail
    let result = client.try_record_funding(&id, &100_000i128, &pool);
    assert!(result.is_err(), "record_funding should fail on expired invoice");
}

// Issue #1655: record_funding should cap funded_amount to invoice amount
#[test]
fn test_record_funding_caps_funded_amount() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, pool, sme) = setup(&env);

    // Create invoice with 1000 amount
    let id = client.create_invoice(
        &sme,
        &String::from_str(&env, "Debtor1"),
        &1_000i128,
        &(1_000_000 + 86_400),
        &String::from_str(&env, "desc1"),
        &String::from_str(&env, "hash1"),
        &String::from_str(&env, "https://example.com/meta1"),
    );

    // Record funding with 500
    client.record_funding(&id, &500i128, &pool).unwrap();
    let inv = client.get_invoice(&id);
    assert_eq!(inv.funded_amount, 500i128);

    // Record funding with 600 more (would be 1100 total, but should be capped at 1000)
    client.record_funding(&id, &600i128, &pool).unwrap();
    let inv = client.get_invoice(&id);
    assert_eq!(inv.funded_amount, 1_000i128, "funded_amount should be capped at invoice amount");
}

// Issue #1656: add_funding function removed, verify record_funding is the single source of truth
#[test]
fn test_record_funding_uses_single_counter() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, pool, sme) = setup(&env);

    // Create invoice
    let id = client.create_invoice(
        &sme,
        &String::from_str(&env, "Debtor1"),
        &1_000_000i128,
        &(1_000_000 + 86_400),
        &String::from_str(&env, "desc1"),
        &String::from_str(&env, "hash1"),
        &String::from_str(&env, "https://example.com/meta1"),
    );

    // Record funding
    client.record_funding(&id, &300_000i128, &pool).unwrap();

    // get_funded_amount should return the same value as invoice.funded_amount
    let funded_from_function = client.get_funded_amount(&id);
    let inv = client.get_invoice(&id);
    assert_eq!(funded_from_function, inv.funded_amount, "get_funded_amount should match invoice.funded_amount");
    assert_eq!(funded_from_function, 300_000i128);

    // Record more funding
    client.record_funding(&id, &200_000i128, &pool).unwrap();
    let funded_from_function = client.get_funded_amount(&id);
    let inv = client.get_invoice(&id);
    assert_eq!(funded_from_function, inv.funded_amount, "get_funded_amount should still match invoice.funded_amount");
    assert_eq!(funded_from_function, 500_000i128);
}
