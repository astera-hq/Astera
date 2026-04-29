#![cfg(test)]

use proptest::prelude::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

use credit_score::{
    CreditScoreContract, CreditScoreContractClient, PaymentStatus, MAX_SCORE, MIN_SCORE,
};

fn setup(env: &Env) -> (CreditScoreContractClient<'_>, Address, Address, Address) {
    let contract_id = env.register(CreditScoreContract, ());
    let client = CreditScoreContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let invoice_contract = Address::generate(env);
    let pool_contract = Address::generate(env);
    client.initialize(&admin, &invoice_contract, &pool_contract);
    (client, admin, invoice_contract, pool_contract)
}

#[derive(Debug, Clone)]
enum Action {
    Payment { amount: i128, days_late: i64 },
    Default { amount: i128 },
}

fn any_action() -> impl Strategy<Value = Action> {
    prop_oneof![
        any::<i128>().prop_map(|amount| Action::Default {
            // 0-amount invoices aren't meaningful and can create weird edge cases.
            amount: (amount.abs() % 1_000_000_000_000) + 1
        }),
        (any::<i128>(), -30..60i64).prop_map(|(amount, days_late)| Action::Payment {
            amount: (amount.abs() % 1_000_000_000_000) + 1,
            days_late
        }),
    ]
}

proptest! {
    // Keep this property test reasonably fast for CI.
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_credit_score_invariants(actions in prop::collection::vec(any_action(), 1..20)) {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);

        let mut current_score = MIN_SCORE;
        let mut total_invoices = 0;

        for (i, action) in actions.into_iter().enumerate() {
            let invoice_id = i as u64 + 1;
            let due_date = 100_000u64;

            match action {
                Action::Payment { amount, days_late } => {
                    // Avoid extremely large timestamps that could overflow
                    let paid_at = if days_late >= 0 {
                        due_date.saturating_add(days_late as u64 * 86400)
                    } else {
                        due_date.saturating_sub(days_late.abs() as u64 * 86400)
                    };

                    client.record_payment(&pool, &invoice_id, &sme, &amount, &due_date, &paid_at);

                    let new_data = client.get_credit_score(&sme);

                    // Invariant 1: Bounds
                    prop_assert!(new_data.score >= MIN_SCORE && new_data.score <= MAX_SCORE);

                    // Invariant 2: Monotonicity
                    // On-time payment (including early) should never decrease score
                    // Skip monotonic checks on the first ever recorded invoice since the
                    // model moves from MIN_SCORE (no history) to BASE_SCORE (has history).
                    if total_invoices > 0 && days_late <= 0 {
                        prop_assert!(new_data.score >= current_score,
                            "Score decreased on on-time payment: {} -> {} (days_late: {})",
                            current_score, new_data.score, days_late);
                    }
                    // Default via late payment should never increase score
                    if total_invoices > 0 && days_late > 7 {
                         prop_assert!(new_data.score <= current_score,
                            "Score increased on late default: {} -> {} (days_late: {})",
                            current_score, new_data.score, days_late);
                    }

                    current_score = new_data.score;
                }
                Action::Default { amount } => {
                    client.record_default(&pool, &invoice_id, &sme, &amount, &due_date);

                    let new_data = client.get_credit_score(&sme);

                    // Invariant 1: Bounds
                    prop_assert!(new_data.score >= MIN_SCORE && new_data.score <= MAX_SCORE);

                    // Invariant 2: Monotonicity
                    // Default should never increase score
                    if total_invoices > 0 {
                        prop_assert!(new_data.score <= current_score,
                        "Score increased on default: {} -> {}",
                        current_score, new_data.score);
                    }

                    current_score = new_data.score;
                }
            }

            total_invoices += 1;
            let data = client.get_credit_score(&sme);

            // Invariant 3: Counter consistency
            prop_assert_eq!(data.total_invoices, total_invoices);
            prop_assert_eq!(data.paid_on_time + data.paid_late + data.defaulted, total_invoices);

            // Invariant 4: History integrity
            let history = client.get_payment_history(&sme);
            prop_assert_eq!(history.len(), total_invoices);
        }
    }
}
