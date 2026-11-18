use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{Address, Env, IntoVal, Val, Vec};
use tranche::events::{CONFIG, EVT, OPENED};
use tranche::state::TrancheConfig;
use tranche::{TrancheContract, TrancheContractClient};

fn config() -> TrancheConfig {
    TrancheConfig {
        senior_target_yield_bps: 1_000,
        senior_advance_rate_bps: 8_000,
        junior_first_loss_bps: 10_000,
    }
}

#[test]
fn opening_and_reconfiguring_publish_distinct_event_topics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(TrancheContract, ());
    let client = TrancheContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let initial_token = Address::generate(&env);

    client.initialize(
        &admin,
        &initial_token,
        &Address::generate(&env),
        &Address::generate(&env),
        &config(),
    );

    let token = Address::generate(&env);
    client.open_tranche_for_token(
        &admin,
        &token,
        &Address::generate(&env),
        &Address::generate(&env),
        &config(),
    );
    let opened_topics: Vec<Val> = (EVT, OPENED).into_val(&env);
    assert!(env
        .events()
        .all()
        .iter()
        .any(|event| event.1 == opened_topics));

    client.set_tranche_config(&admin, &token, &500, &7_500, &9_000);
    let config_topics: Vec<Val> = (EVT, CONFIG).into_val(&env);
    assert!(env
        .events()
        .all()
        .iter()
        .any(|event| event.1 == config_topics));
    assert_ne!(opened_topics, config_topics);
}
