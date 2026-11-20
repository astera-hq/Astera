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

#[test]
fn test_cancel_unfunded_invoice_releases_debtor_exposure() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let (client, admin, sme) = setup(&env);
    let debtor = String::from_str(&env, "Debtor Corp");
    let amount = 1_000_000i128;

    client.set_require_registered_debtor(&admin, &true);
    client.register_debtor(
        &admin,
        &debtor,
        &String::from_str(&env, "Debtor Corp Ltd"),
        &(amount * 2),
    );

    let due = env.ledger().timestamp() + SECS_PER_DAY * 30;
    let id = client.create_invoice(
        &sme,
        &debtor,
        &amount,
        &due,
        &String::from_str(&env, "Test invoice"),
        &String::from_str(&env, "hash123"),
        &String::from_str(&env, "https://example.com/meta"),
    );
    // Creation consumes exposure even though the invoice is not funded.
    assert_eq!(client.get_debtor(&debtor).current_exposure, amount);

    client.cancel_invoice(&id, &sme);
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Cancelled);
    assert_eq!(client.get_debtor(&debtor).current_exposure, 0);

    // The released exposure can be used again for a new invoice.
    let id2 = client.create_invoice(
        &sme,
        &debtor,
        &(amount * 2),
        &due,
        &String::from_str(&env, "Replacement invoice"),
        &String::from_str(&env, "hash456"),
        &String::from_str(&env, "https://example.com/meta2"),
    );
    assert_eq!(client.get_invoice(&id2).status, InvoiceStatus::Pending);
    assert_eq!(client.get_debtor(&debtor).current_exposure, amount * 2);
}
