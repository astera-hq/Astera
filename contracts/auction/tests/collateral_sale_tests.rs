#![cfg(test)]

use auction::{AuctionContract, AuctionContractClient, AuctionError, CollateralSaleParams};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

fn setup_sale(
    env: &Env,
) -> (
    AuctionContractClient<'_>,
    Address,
    Address,
    Address,
    Address,
) {
    env.ledger().with_mut(|l| l.timestamp = 100_000);
    env.mock_all_auths();
    let auction_id = env.register(AuctionContract, ());
    let client = AuctionContractClient::new(env, &auction_id);
    let seized_token = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();
    let proceeds_token = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();
    (
        client,
        seized_token,
        proceeds_token,
        Address::generate(env),
        Address::generate(env),
    )
}

#[test]
fn test_current_sale_price_at_open_and_expiry_boundaries() {
    let env = Env::default();
    let (client, seized_token, proceeds_token, seller, recipient) = setup_sale(&env);
    token::StellarAssetClient::new(&env, &seized_token).mint(&seller, &1_000);
    let sale_id = client.open_collateral_sale(&CollateralSaleParams {
        seller,
        token: seized_token,
        amount: 1_000,
        proceeds_token,
        proceeds_recipient: recipient,
        start_price: 900,
        floor_price: 500,
        duration_secs: 3_600,
    });

    assert_eq!(client.current_sale_price(&sale_id), 900);
    env.ledger().with_mut(|l| l.timestamp += 3_600);
    assert_eq!(client.current_sale_price(&sale_id), 500);
}

#[test]
fn test_take_collateral_sale_one_second_past_expiry_is_rejected() {
    let env = Env::default();
    let (client, seized_token, proceeds_token, seller, recipient) = setup_sale(&env);
    let taker = Address::generate(&env);
    token::StellarAssetClient::new(&env, &seized_token).mint(&seller, &1_000);
    token::StellarAssetClient::new(&env, &proceeds_token).mint(&taker, &1_000);
    let sale_id = client.open_collateral_sale(&CollateralSaleParams {
        seller,
        token: seized_token,
        amount: 1_000,
        proceeds_token,
        proceeds_recipient: recipient,
        start_price: 900,
        floor_price: 500,
        duration_secs: 3_600,
    });

    env.ledger().with_mut(|l| l.timestamp += 3_601);
    assert_eq!(
        client.try_take_collateral_sale(&taker, &sale_id),
        Err(Ok(AuctionError::SaleExpired))
    );
}
