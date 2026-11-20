#![cfg(test)]

use soroban_sdk::{testutils::Address as _, testutils::Ledger, Address, Env, String};

use invoice::{InvoiceContract, InvoiceContractClient, InvoiceStatus};

const SECS_PER_DAY: u64 = 86400;
const DEFAULT_EXPIRATION_DURATION_SECS: u64 = SECS_PER_DAY * 30;

fn setup(env: &Env) -> (InvoiceContractClient<'_>, Address, Address) {
    let contract_id = env.register(InvoiceContract, ());
    let client = InvoiceContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let pool = Address::generate(env);
    let sme = Address::generate(env);
    client.initialize(
        &admin,
        &pool,
        &i128::MAX,
        &DEFAULT_EXPIRATION_DURATION_SECS,
        &7u32,
    );
    (client, admin, sme)
}

fn create(env: &Env, client: &InvoiceContractClient<'_>, sme: &Address) -> u64 {
    let due = env.ledger().timestamp() + SECS_PER_DAY * 30;
    client.create_invoice(
        sme,
        &String::from_str(env, "Debtor Corp"),
        &1_000_000i128,
        &due,
        &String::from_str(env, "Test invoice"),
        &String::from_str(env, "hash123"),
        &String::from_str(env, "https://example.com/meta"),
    )
}

#[test]
fn test_registry_only_deployment_creates_awaiting_verification() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, sme) = setup(&env);
    // Consensus-only deployment: an oracle registry is configured, but no
    // legacy `Oracle` key is set.
    let registry = Address::generate(&env);
    client.set_oracle_registry(&admin, &registry);

    let id = create(&env, &client, &sme);
    assert_eq!(
        client.get_invoice(&id).status,
        InvoiceStatus::AwaitingVerification
    );
}

#[test]
fn test_no_oracle_or_registry_keeps_pending() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, _admin, sme) = setup(&env);

    let id = create(&env, &client, &sme);
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Pending);
}
