#![cfg(test)]

use proptest::prelude::*;
use share::{ShareToken, ShareTokenClient};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn setup(env: &Env) -> (ShareTokenClient<'_>, Address) {
    let contract_id = env.register(ShareToken, ());
    let client = ShareTokenClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(
        &admin,
        &7u32,
        &String::from_str(env, "Pool Shares"),
        &String::from_str(env, "POOL"),
    );
    (client, admin)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Invariant: total_supply always equals the sum of every holder's balance.
    ///
    /// After any sequence of mints and burns the accounting identity must hold.
    /// This catches off-by-one errors in supply tracking and ensures governance
    /// voting power (derived from share balances) cannot be manufactured out of
    /// thin air.
    #[test]
    fn prop_total_supply_equals_sum_of_balances(
        amounts in prop::collection::vec(1i128..10_000_000i128, 1..8)
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);

        let holders: Vec<Address> = (0..amounts.len())
            .map(|_| Address::generate(&env))
            .collect();

        let mut expected_total = 0i128;
        for (holder, &amount) in holders.iter().zip(amounts.iter()) {
            client.mint(holder, &amount);
            expected_total += amount;
        }

        prop_assert_eq!(client.total_supply(), expected_total,
            "total_supply diverged from sum of minted amounts");

        let sum_of_balances: i128 = holders.iter().map(|h| client.balance(h)).sum();
        prop_assert_eq!(sum_of_balances, expected_total,
            "sum of individual balances diverged from total_supply");
    }

    /// Invariant: balance(addr) >= 0 after any combination of mint and partial burn.
    ///
    /// The contract guards against over-burn, so the balance floor is always 0.
    #[test]
    fn prop_balance_never_negative(
        mint_amount in 1i128..100_000_000i128,
        burn_fraction_pct in 0u32..100u32
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let holder = Address::generate(&env);

        client.mint(&holder, &mint_amount);
        prop_assert!(client.balance(&holder) >= 0);

        // Burn a fraction so we never exceed the balance (avoiding panic from
        // insufficient balance — that is tested separately).
        let burn = (mint_amount * burn_fraction_pct as i128 / 100).max(0);
        if burn > 0 {
            client.burn(&holder, &burn);
        }

        prop_assert!(
            client.balance(&holder) >= 0,
            "balance went negative after burn of {} from {}",
            burn, mint_amount
        );
    }

    /// Invariant: allowance never underflows below 0 after transfer_from.
    ///
    /// Each transfer_from deducts exactly `amount` from the stored allowance.
    /// This property verifies the deduction never produces a negative value,
    /// guarding against allowance-accounting bugs that could allow unlimited
    /// spending from a finite approval.
    #[test]
    fn prop_allowance_never_underflows(
        mint_amount in 1i128..100_000_000i128,
        approve_amount in 1i128..50_000_000i128,
        transfer_pct in 1u32..100u32
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);
        let recipient = Address::generate(&env);

        client.mint(&owner, &mint_amount);
        client.approve(&owner, &spender, &approve_amount);

        // Transfer only what both the allowance and balance can cover.
        let safe_transfer = (approve_amount * transfer_pct as i128 / 100)
            .min(mint_amount)
            .max(1);

        client.transfer_from(&spender, &owner, &recipient, &safe_transfer);

        let remaining = client.allowance(&owner, &spender);
        prop_assert!(
            remaining >= 0,
            "allowance underflowed to {} after transfer_from of {}",
            remaining, safe_transfer
        );
        prop_assert_eq!(
            remaining,
            approve_amount - safe_transfer,
            "allowance deduction was not exactly transfer amount"
        );
    }

    /// Invariant: transfer is net-zero on total_supply.
    ///
    /// Moving tokens between accounts must never create or destroy supply.
    #[test]
    fn prop_transfer_is_net_zero_on_supply(
        mint_amount in 1i128..100_000_000i128,
        transfer_pct in 1u32..100u32
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        client.mint(&alice, &mint_amount);
        let supply_before = client.total_supply();

        let transfer_amount = (mint_amount * transfer_pct as i128 / 100).max(1);
        client.transfer(&alice, &bob, &transfer_amount);

        prop_assert_eq!(
            client.total_supply(),
            supply_before,
            "transfer changed total_supply"
        );
        prop_assert_eq!(
            client.balance(&alice) + client.balance(&bob),
            mint_amount,
            "sum of alice+bob balances must equal minted amount after transfer"
        );
    }

    /// Invariant: burn reduces total_supply by exactly the burned amount.
    #[test]
    fn prop_burn_reduces_supply_exactly(
        mint_amount in 2i128..100_000_000i128,
        burn_pct in 1u32..99u32
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let holder = Address::generate(&env);

        client.mint(&holder, &mint_amount);
        let supply_before = client.total_supply();

        let burn_amount = (mint_amount * burn_pct as i128 / 100).max(1);
        client.burn(&holder, &burn_amount);

        prop_assert_eq!(
            client.total_supply(),
            supply_before - burn_amount,
            "supply delta after burn must equal burn_amount exactly"
        );
        prop_assert_eq!(
            client.balance(&holder),
            mint_amount - burn_amount,
            "holder balance after burn must be mint_amount - burn_amount"
        );
    }
}
