#![cfg(test)]

//! #1657: debtor identifiers are normalized on every entry point, so case, surrounding
//! whitespace and repeated inner spaces cannot create a second debtor.

use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env, String};

use invoice::{InvoiceContract, InvoiceContractClient};

const SECS_PER_DAY: u64 = 86_400;

fn setup(env: &Env) -> (InvoiceContractClient<'_>, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(InvoiceContract, ());
    let client = InvoiceContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let pool = Address::generate(env);
    let sme = Address::generate(env);
    client.initialize(&admin, &pool, &i128::MAX, &(SECS_PER_DAY * 30), &7u32);
    (client, admin, sme)
}

#[test]
fn register_then_get_with_variant_spelling_finds_the_same_debtor() {
    let env = Env::default();
    let (client, admin, _sme) = setup(&env);
    client.register_debtor(
        &admin,
        &String::from_str(&env, "ACME Ltd"),
        &String::from_str(&env, "Acme Limited"),
        &50_000i128,
    );
    let record = client.get_debtor(&String::from_str(&env, "  acme   LTD "));
    assert_eq!(record.debtor_id, String::from_str(&env, "acme ltd"));
    assert!(record.is_active);
}

#[test]
fn list_debtors_holds_only_the_normalized_key() {
    let env = Env::default();
    let (client, admin, _sme) = setup(&env);
    client.register_debtor(
        &admin,
        &String::from_str(&env, "ACME Ltd"),
        &String::from_str(&env, "Acme Limited"),
        &50_000i128,
    );
    let list = client.list_debtors();
    assert_eq!(list.len(), 1);
    assert_eq!(list.get(0).unwrap(), String::from_str(&env, "acme ltd"));
}

#[test]
fn deactivate_with_variant_spelling_deactivates_the_same_debtor() {
    let env = Env::default();
    let (client, admin, _sme) = setup(&env);
    client.register_debtor(
        &admin,
        &String::from_str(&env, "Acme Ltd"),
        &String::from_str(&env, "Acme Limited"),
        &50_000i128,
    );
    client.deactivate_debtor(&admin, &String::from_str(&env, "acme   ltd"));
    assert!(!client.get_debtor(&String::from_str(&env, "ACME LTD")).is_active);
}

#[test]
fn invoice_created_under_a_variant_is_indexed_under_the_normalized_debtor() {
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);
    let (client, _admin, sme) = setup(&env);
    let due = env.ledger().timestamp() + SECS_PER_DAY * 30;
    let id = client.create_invoice(
        &sme,
        &String::from_str(&env, "  ACME   ltd "),
        &1_000_000i128,
        &due,
        &String::from_str(&env, "desc"),
        &String::from_str(&env, "hash"),
        &String::from_str(&env, "https://example.com/meta"),
    );
    let by_debtor = client.get_invoices_by_debtor(&String::from_str(&env, "acme ltd"));
    assert_eq!(by_debtor.len(), 1);
    assert_eq!(by_debtor.get(0).unwrap(), id);
}

#[test]
fn blank_debtor_identifier_is_rejected() {
    let env = Env::default();
    let (client, admin, _sme) = setup(&env);
    let result = client.try_register_debtor(
        &admin,
        &String::from_str(&env, "   "),
        &String::from_str(&env, "Blank"),
        &50_000i128,
    );
    assert!(result.is_err());
}
