#![cfg(test)]

//! #1658: tests for cleanup_invoice, mark_cancelled, run_migration, list_debtors and the
//! invoice index getters.

use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env, String};

use invoice::{InvoiceContract, InvoiceContractClient};

const SECS_PER_DAY: u64 = 86_400;

fn setup(env: &Env) -> (InvoiceContractClient<'_>, Address, Address, Address) {
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);
    let contract_id = env.register(InvoiceContract, ());
    let client = InvoiceContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let pool = Address::generate(env);
    let sme = Address::generate(env);
    client.initialize(&admin, &pool, &i128::MAX, &(SECS_PER_DAY * 30), &7u32);
    (client, admin, pool, sme)
}

fn create_pending(
    env: &Env,
    client: &InvoiceContractClient<'_>,
    sme: &Address,
    debtor: &str,
    amount: i128,
) -> u64 {
    let due = env.ledger().timestamp() + SECS_PER_DAY * 30;
    client.create_invoice(
        sme,
        &String::from_str(env, debtor),
        &amount,
        &due,
        &String::from_str(env, "desc"),
        &String::from_str(env, "hash"),
        &String::from_str(env, "https://example.com/meta"),
    )
}

// ── cleanup_invoice ──────────────────────────────────────────────────────────

#[test]
fn cleanup_removes_a_cancelled_invoice_and_its_index_entries() {
    let env = Env::default();
    let (client, admin, _pool, sme) = setup(&env);
    let id = create_pending(&env, &client, &sme, "Debtor Corp", 1_000_000);
    client.cancel_invoice(&id, &sme);
    client.cleanup_invoice(&id, &admin);
    assert!(client.try_get_invoice(&id).is_err());
    assert_eq!(client.get_invoices_by_owner(&sme).len(), 0);
}

#[test]
fn cleanup_rejects_an_invoice_that_is_not_completed() {
    let env = Env::default();
    let (client, admin, _pool, sme) = setup(&env);
    let id = create_pending(&env, &client, &sme, "Debtor Corp", 1_000_000);
    assert!(client.try_cleanup_invoice(&id, &admin).is_err());
}

#[test]
fn cleanup_rejects_a_non_admin_caller() {
    let env = Env::default();
    let (client, _admin, _pool, sme) = setup(&env);
    let id = create_pending(&env, &client, &sme, "Debtor Corp", 1_000_000);
    client.cancel_invoice(&id, &sme);
    assert!(client.try_cleanup_invoice(&id, &sme).is_err());
}

// ── mark_cancelled ───────────────────────────────────────────────────────────

#[test]
fn mark_cancelled_rejects_a_caller_that_is_not_the_authorized_pool() {
    let env = Env::default();
    let (client, _admin, _pool, sme) = setup(&env);
    let id = create_pending(&env, &client, &sme, "Debtor Corp", 1_000_000);
    let other = Address::generate(&env);
    assert!(client.try_mark_cancelled(&id, &other).is_err());
}

#[test]
fn mark_cancelled_rejects_an_invoice_that_is_not_funded() {
    let env = Env::default();
    let (client, _admin, pool, sme) = setup(&env);
    let id = create_pending(&env, &client, &sme, "Debtor Corp", 1_000_000);
    assert!(client.try_mark_cancelled(&id, &pool).is_err());
}

// ── run_migration / migration_version ────────────────────────────────────────

#[test]
fn run_migration_advances_the_version_and_is_idempotent() {
    let env = Env::default();
    let (client, admin, _pool, _sme) = setup(&env);
    client.run_migration(&admin);
    let first = client.migration_version();
    assert!(first >= 1);
    client.run_migration(&admin);
    assert_eq!(client.migration_version(), first);
}

#[test]
fn run_migration_rejects_a_non_admin_caller() {
    let env = Env::default();
    let (client, _admin, _pool, sme) = setup(&env);
    assert!(client.try_run_migration(&sme).is_err());
}

// ── list_debtors ─────────────────────────────────────────────────────────────

#[test]
fn list_debtors_returns_every_registered_debtor() {
    let env = Env::default();
    let (client, admin, _pool, _sme) = setup(&env);
    client.register_debtor(
        &admin,
        &String::from_str(&env, "debtor-a"),
        &String::from_str(&env, "A"),
        &10_000i128,
    );
    client.register_debtor(
        &admin,
        &String::from_str(&env, "debtor-b"),
        &String::from_str(&env, "B"),
        &10_000i128,
    );
    assert_eq!(client.list_debtors().len(), 2);
}

// ── index getters ────────────────────────────────────────────────────────────

#[test]
fn get_invoices_by_owner_lists_the_created_invoice() {
    let env = Env::default();
    let (client, _admin, _pool, sme) = setup(&env);
    let id = create_pending(&env, &client, &sme, "Debtor Corp", 1_000_000);
    let owned = client.get_invoices_by_owner(&sme);
    assert_eq!(owned.len(), 1);
    assert_eq!(owned.get(0).unwrap(), id);
}

#[test]
fn get_invoices_by_debtor_lists_the_created_invoice() {
    let env = Env::default();
    let (client, _admin, _pool, sme) = setup(&env);
    let id = create_pending(&env, &client, &sme, "Debtor Corp", 1_000_000);
    let by_debtor = client.get_invoices_by_debtor(&String::from_str(&env, "Debtor Corp"));
    assert_eq!(by_debtor.len(), 1);
    assert_eq!(by_debtor.get(0).unwrap(), id);
}

#[test]
fn get_invoice_verification_state_is_not_awaiting_for_a_pending_invoice() {
    let env = Env::default();
    let (client, _admin, _pool, sme) = setup(&env);
    let id = create_pending(&env, &client, &sme, "Debtor Corp", 1_000_000);
    assert_eq!(client.get_invoice_verification_state(&id), (false, 1_000_000i128));
}

#[test]
fn is_invoice_defaulted_is_false_for_a_pending_invoice() {
    let env = Env::default();
    let (client, _admin, _pool, sme) = setup(&env);
    let id = create_pending(&env, &client, &sme, "Debtor Corp", 1_000_000);
    assert!(!client.is_invoice_defaulted(&id));
}

#[test]
fn get_authorized_pool_returns_the_configured_pool() {
    let env = Env::default();
    let (client, _admin, pool, _sme) = setup(&env);
    assert_eq!(client.get_authorized_pool(), pool);
}
