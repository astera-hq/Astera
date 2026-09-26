use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};
use tranche::{state::TrancheConfig, TrancheContract, TrancheContractClient};

fn setup(env: &Env) -> (TrancheContractClient<'_>, Address) {
    env.mock_all_auths();

    let contract_id = env.register(TrancheContract, ());
    let client = TrancheContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let token = Address::generate(env);
    let senior_share_token = Address::generate(env);
    let junior_share_token = Address::generate(env);

    client.initialize(
        &admin,
        &token,
        &senior_share_token,
        &junior_share_token,
        &TrancheConfig {
            senior_target_yield_bps: 1_000,
            senior_advance_rate_bps: 8_000,
            junior_first_loss_bps: 10_000,
        },
    );

    env.set_auths(&[]);
    (client, token)
}

#[test]
fn funding_requires_admin_auth() {
    let env = Env::default();
    let (client, token) = setup(&env);

    assert!(client
        .try_fund_invoice_from_tranches(&token, &1, &1000)
        .is_err());
}

#[test]
fn repayment_requires_admin_auth() {
    let env = Env::default();
    let (client, token) = setup(&env);

    assert!(client
        .try_distribute_waterfall_repayment(&token, &1, &1000, &0)
        .is_err());
}

#[test]
fn loss_allocation_requires_admin_auth() {
    let env = Env::default();
    let (client, token) = setup(&env);

    assert!(client.try_allocate_loss(&token, &1, &1000).is_err());
}
