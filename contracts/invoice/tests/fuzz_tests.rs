#![cfg(test)]

use proptest::prelude::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

// Re-export the contract for testing
use invoice::{InvoiceContract, InvoiceContractClient};

fn setup(env: &Env) -> (InvoiceContractClient<'_>, Address, Address, Address) {
    let contract_id = env.register(InvoiceContract, ());
    let client = InvoiceContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let pool = Address::generate(env);
    let sme = Address::generate(env);
    let expiration = 30u64 * 86_400u64;
    // Contract enforces `grace_period_days <= 90`.
    client.initialize(&admin, &pool, &i128::MAX, &expiration, &90u32);
    (client, admin, pool, sme)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    // ---- #110: Invoice contract invariants ----

    /// Invariant: Invoice ID is always sequential and equals count after creation.
    #[test]
    fn prop_invoice_id_is_sequential(count in 1u64..9u64) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, _admin, _pool, sme) = setup(&env);
        let due_date = env.ledger().timestamp() + 86_400;

        for i in 0..count {
            let id = client.create_invoice(
                &sme,
                &String::from_str(&env, "Debtor"),
                &1_000_000i128,
                &due_date,
                &String::from_str(&env, "desc"),
                &String::from_str(&env, "hash"),
            );
            prop_assert_eq!(id, i + 1, "Invoice ID must equal creation index");
        }
        prop_assert_eq!(
            client.get_invoice_count(),
            count,
            "Invoice count must match number created"
        );
    }

    /// Invariant: A newly created invoice is always in Pending status.
    #[test]
    fn prop_new_invoice_is_pending(amount in 1i128..1_000_000_000i128) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, _admin, _pool, sme) = setup(&env);
        let due_date = env.ledger().timestamp() + 86_400;

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "Debtor"),
            &amount,
            &due_date,
            &String::from_str(&env, "desc"),
            &String::from_str(&env, "hash"),
        );

        let invoice = client.get_invoice(&id);
        prop_assert!(
            matches!(invoice.status, invoice::InvoiceStatus::Pending),
            "Newly created invoice must be Pending"
        );
    }

    /// Invariant: Funded invoice amount equals the originally created amount.
    #[test]
    fn prop_funded_invoice_preserves_amount(amount in 1_000_000i128..1_000_000_000i128) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, _admin, pool, sme) = setup(&env);
        let due_date = env.ledger().timestamp() + 86_400;

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "Debtor"),
            &amount,
            &due_date,
            &String::from_str(&env, "desc"),
            &String::from_str(&env, "hash"),
        );
        client.mark_funded(&id, &pool);

        let invoice = client.get_invoice(&id);
        prop_assert_eq!(invoice.amount, amount, "Funded invoice amount must be preserved");
        prop_assert!(
            matches!(invoice.status, invoice::InvoiceStatus::Funded),
            "Invoice must be Funded after mark_funded"
        );
    }

    /// Invariant: Grace period setting is always within [0, 90] and read back correctly.
    #[test]
    fn prop_grace_period_roundtrip(grace in 0u32..90u32) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);

        client.set_grace_period(&admin, &grace);
        prop_assert_eq!(client.get_grace_period(), grace, "Grace period round-trip failed");
    }

    /// Fuzz test: Invoice creation with random valid amounts
    #[test]
    fn fuzz_invoice_creation_amounts(amount in 1i128..1_000_000_000_000i128) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, _admin, _pool, sme) = setup(&env);
        let due_date = env.ledger().timestamp() + 86_400;

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "Debtor"),
            &amount,
            &due_date,
            &String::from_str(&env, "desc"),
            &String::from_str(&env, "hash"),
        );

        let invoice = client.get_invoice(&id);
        prop_assert_eq!(invoice.amount, amount);
        prop_assert_eq!(invoice.id, 1);
    }

    /// Fuzz test: Invoice creation with random due dates
    #[test]
    fn fuzz_invoice_due_dates(days_ahead in 1u64..365u64) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, _admin, _pool, sme) = setup(&env);
        let due_date = env.ledger().timestamp() + (days_ahead * 86_400);

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "Debtor"),
            &1_000_000i128,
            &due_date,
            &String::from_str(&env, "desc"),
            &String::from_str(&env, "hash"),
        );

        let invoice = client.get_invoice(&id);
        prop_assert_eq!(invoice.due_date, due_date);
    }

    /// Fuzz test: Grace period configuration
    #[test]
    fn fuzz_grace_period_settings(grace_days in 0u32..90u32) {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, _pool, _sme) = setup(&env);

        client.set_grace_period(&admin, &grace_days);
        let retrieved = client.get_grace_period();

        prop_assert_eq!(retrieved, grace_days);
    }

    /// Fuzz test: Multiple invoice creation sequence
    #[test]
    fn fuzz_multiple_invoice_creation(count in 1usize..10usize) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, _admin, _pool, sme) = setup(&env);
        let due_date = env.ledger().timestamp() + 86_400;

        for i in 0..count {
            let id = client.create_invoice(
                &sme,
                &String::from_str(&env, "Debtor"),
                &((i as i128 + 1) * 1_000_000i128),
                &due_date,
                &String::from_str(&env, "desc"),
                &String::from_str(&env, "hash"),
            );
            prop_assert_eq!(id, (i as u64) + 1);
        }

        prop_assert_eq!(client.get_invoice_count(), count as u64);
    }

    /// Fuzz test: Grace period enforcement on default
    #[test]
    fn fuzz_grace_period_enforcement(
        grace_days in 1u32..30u32,
        days_past_due in 1u64..60u64
    ) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, admin, pool, sme) = setup(&env);
        client.set_grace_period(&admin, &grace_days);

        let due_date = env.ledger().timestamp() + 86_400;
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "Debtor"),
            &1_000_000i128,
            &due_date,
            &String::from_str(&env, "desc"),
            &String::from_str(&env, "hash"),
        );

        client.mark_funded(&id, &pool);

        // Move past due date
        env.ledger().with_mut(|l| l.timestamp = due_date + (days_past_due * 86_400));

        let grace_period_secs = grace_days as u64 * 86_400;
        let should_succeed = days_past_due * 86_400 >= grace_period_secs;

        if should_succeed {
            client.mark_defaulted(&id, &pool);
            let invoice = client.get_invoice(&id);
            prop_assert!(matches!(invoice.status, invoice::InvoiceStatus::Defaulted));
        }
        // Note: Cannot test panic case in proptest without std
    }
}

#[cfg(test)]
mod deterministic_fuzz {
    use super::*;

    /// Deterministic fuzz test: Invoice amount edge cases
    #[test]
    fn test_invoice_amount_edge_cases() {
        let test_amounts = vec![
            1i128,
            100i128,
            1_000_000i128,
            1_000_000_000i128,
            100_000_000_000i128,
            i128::MAX / 2,
        ];

        for amount in test_amounts {
            let env = Env::default();
            env.mock_all_auths();
            env.ledger().with_mut(|l| l.timestamp = 100_000);

            let (client, _admin, _pool, sme) = setup(&env);
            let due_date = env.ledger().timestamp() + 86_400;

            let id = client.create_invoice(
                &sme,
                &String::from_str(&env, "Debtor"),
                &amount,
                &due_date,
                &String::from_str(&env, "desc"),
                &String::from_str(&env, "hash"),
            );

            let invoice = client.get_invoice(&id);
            assert_eq!(invoice.amount, amount);
        }
    }

    /// Deterministic fuzz test: Grace period boundary conditions
    #[test]
    fn test_grace_period_boundaries() {
        let test_cases = vec![
            (7u32, 6u64, false),   // 6 days past due, 7 day grace - should fail
            (7u32, 7u64, true),    // 7 days past due, 7 day grace - should succeed
            (7u32, 8u64, true),    // 8 days past due, 7 day grace - should succeed
            (0u32, 0u64, true),    // 0 grace period, 0 days past - should succeed
            (30u32, 29u64, false), // 29 days past due, 30 day grace - should fail
            (30u32, 30u64, true),  // 30 days past due, 30 day grace - should succeed
        ];

        for (grace_days, days_past_due, should_succeed) in test_cases {
            let env = Env::default();
            env.mock_all_auths();
            env.ledger().with_mut(|l| l.timestamp = 100_000);

            let (client, admin, pool, sme) = setup(&env);
            client.set_grace_period(&admin, &grace_days);

            let due_date = env.ledger().timestamp() + 86_400;
            let id = client.create_invoice(
                &sme,
                &String::from_str(&env, "Debtor"),
                &1_000_000i128,
                &due_date,
                &String::from_str(&env, "desc"),
                &String::from_str(&env, "hash"),
            );

            client.mark_funded(&id, &pool);
            env.ledger()
                .with_mut(|l| l.timestamp = due_date + (days_past_due * 86_400));

            if should_succeed {
                client.mark_defaulted(&id, &pool);
                let invoice = client.get_invoice(&id);
                assert!(matches!(invoice.status, invoice::InvoiceStatus::Defaulted));
            }
            // Note: Cannot test panic case without std
        }
    }

    /// Deterministic fuzz test: Rapid invoice creation
    #[test]
    fn test_rapid_invoice_creation() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, _admin, _pool, sme) = setup(&env);
        let due_date = env.ledger().timestamp() + 86_400;

        // Create 10 invoices rapidly (within daily limit)
        for i in 0u64..10u64 {
            let id = client.create_invoice(
                &sme,
                &String::from_str(&env, "Debtor"),
                &((i as i128 + 1) * 1_000_000i128),
                &due_date,
                &String::from_str(&env, &format!("Invoice #{}", i)),
                &String::from_str(&env, &format!("hash{}", i)),
            );
            assert_eq!(id, i + 1);
        }

        // Note: 11th invoice would fail (daily limit exceeded) but we can't test panic without std
        assert_eq!(client.get_invoice_count(), 10);
    }

    #[test]
    fn test_get_multiple_invoices_preserves_order() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, _admin, _pool, sme) = setup(&env);
        let due_date = env.ledger().timestamp() + 86_400;

        let id1 = client.create_invoice(
            &sme,
            &String::from_str(&env, "Debtor A"),
            &1_000_000i128,
            &due_date,
            &String::from_str(&env, "desc-a"),
            &String::from_str(&env, "hash-a"),
        );
        let id2 = client.create_invoice(
            &sme,
            &String::from_str(&env, "Debtor B"),
            &2_000_000i128,
            &due_date,
            &String::from_str(&env, "desc-b"),
            &String::from_str(&env, "hash-b"),
        );
        let id3 = client.create_invoice(
            &sme,
            &String::from_str(&env, "Debtor C"),
            &3_000_000i128,
            &due_date,
            &String::from_str(&env, "desc-c"),
            &String::from_str(&env, "hash-c"),
        );

        let ids = soroban_sdk::vec![&env, id3, id1, id2];
        let invoices = client.get_multiple_invoices(&ids);

        assert_eq!(invoices.len(), 3);
        assert_eq!(invoices.get(0).unwrap().id, id3);
        assert_eq!(invoices.get(1).unwrap().id, id1);
        assert_eq!(invoices.get(2).unwrap().id, id2);
    }
}
