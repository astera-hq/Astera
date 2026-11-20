#![cfg(test)]

//! #1659: every governance setter rejects a caller that is not the configured governance
//! address, and accepts the configured one. Setters with a bound also reject an out-of-range value.

use soroban_sdk::{testutils::Address as _, Address, Env, String};
use invoice::{InvoiceContract, InvoiceContractClient};

fn setup(env: &Env) -> (InvoiceContractClient<'_>, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(InvoiceContract, ());
    let client = InvoiceContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let pool = Address::generate(env);
    let gov = Address::generate(env);
    client.initialize(&admin, &pool, &i128::MAX, &2_592_000u64, &7u32);
    client.set_governance_address(&admin, &gov);
    let stranger = Address::generate(env);
    (client, gov, stranger)
}

#[test]
fn set_grace_period_is_governance_only() {
    let env = Env::default();
    let (client, gov, stranger) = setup(&env);
    assert!(client.try_set_grace_period_via_governance(&stranger, &7u32).is_err());
    client.set_grace_period_via_governance(&gov, &7u32);
}

#[test]
fn set_grace_period_rejects_an_out_of_range_value() {
    let env = Env::default();
    let (client, gov, _stranger) = setup(&env);
    assert!(client.try_set_grace_period_via_governance(&gov, &u32::MAX).is_err());
}

#[test]
fn set_max_sme_outstanding_is_governance_only() {
    let env = Env::default();
    let (client, gov, stranger) = setup(&env);
    assert!(client.try_set_max_sme_outstanding_via_governance(&stranger, &5_000_000i128).is_err());
    client.set_max_sme_outstanding_via_governance(&gov, &5_000_000i128);
}

#[test]
fn set_max_sme_outstanding_rejects_an_out_of_range_value() {
    let env = Env::default();
    let (client, gov, _stranger) = setup(&env);
    assert!(client.try_set_max_sme_outstanding_via_governance(&gov, &0i128).is_err());
}

#[test]
fn set_expiration_duration_is_governance_only() {
    let env = Env::default();
    let (client, gov, stranger) = setup(&env);
    assert!(client.try_set_expiration_duration_via_governance(&stranger, &86_400u64).is_err());
    client.set_expiration_duration_via_governance(&gov, &86_400u64);
}

#[test]
fn set_completed_invoice_ttl_is_governance_only() {
    let env = Env::default();
    let (client, gov, stranger) = setup(&env);
    assert!(client.try_set_completed_invoice_ttl_via_governance(&stranger, &100u32).is_err());
    client.set_completed_invoice_ttl_via_governance(&gov, &100u32);
}

#[test]
fn set_daily_invoice_limit_is_governance_only() {
    let env = Env::default();
    let (client, gov, stranger) = setup(&env);
    assert!(client.try_set_daily_invoice_limit_via_governance(&stranger, &10u32).is_err());
    client.set_daily_invoice_limit_via_governance(&gov, &10u32);
}

#[test]
fn set_dispute_window_is_governance_only() {
    let env = Env::default();
    let (client, gov, stranger) = setup(&env);
    assert!(client.try_set_dispute_window_via_governance(&stranger, &3_600u64).is_err());
    client.set_dispute_window_via_governance(&gov, &3_600u64);
}

#[test]
fn set_oracle_is_governance_only() {
    let env = Env::default();
    let (client, gov, stranger) = setup(&env);
    assert!(client.try_set_oracle_via_governance(&stranger, &Address::generate(&env)).is_err());
    client.set_oracle_via_governance(&gov, &Address::generate(&env));
}

#[test]
fn set_secondary_oracle_is_governance_only() {
    let env = Env::default();
    let (client, gov, stranger) = setup(&env);
    assert!(client.try_set_secondary_oracle_via_governance(&stranger, &Some(Address::generate(&env))).is_err());
    client.set_secondary_oracle_via_governance(&gov, &Some(Address::generate(&env)));
}

#[test]
fn set_oracle_registry_is_governance_only() {
    let env = Env::default();
    let (client, gov, stranger) = setup(&env);
    assert!(client.try_set_oracle_registry_via_governance(&stranger, &Address::generate(&env)).is_err());
    client.set_oracle_registry_via_governance(&gov, &Address::generate(&env));
}

#[test]
fn set_consensus_required_is_governance_only() {
    let env = Env::default();
    let (client, gov, stranger) = setup(&env);
    assert!(client.try_set_consensus_required_via_governance(&stranger, &true).is_err());
    client.set_consensus_required_via_governance(&gov, &true);
}

#[test]
fn set_compliance_registry_is_governance_only() {
    let env = Env::default();
    let (client, gov, stranger) = setup(&env);
    assert!(client.try_set_compliance_registry_via_governance(&stranger, &Address::generate(&env)).is_err());
    client.set_compliance_registry_via_governance(&gov, &Address::generate(&env));
}

#[test]
fn set_require_compliance_check_is_governance_only() {
    let env = Env::default();
    let (client, gov, stranger) = setup(&env);
    assert!(client.try_set_require_compliance_check_via_governance(&stranger, &true).is_err());
    client.set_require_compliance_check_via_governance(&gov, &true);
}

#[test]
fn set_require_registered_debtor_is_governance_only() {
    let env = Env::default();
    let (client, gov, stranger) = setup(&env);
    assert!(client.try_set_require_registered_debtor_via_governance(&stranger, &true).is_err());
    client.set_require_registered_debtor_via_governance(&gov, &true);
}

#[test]
fn set_oracle_verified_funding_only_is_governance_only() {
    let env = Env::default();
    let (client, gov, stranger) = setup(&env);
    assert!(client.try_set_oracle_verified_funding_only_via_governance(&stranger, &true).is_err());
    client.set_oracle_verified_funding_only_via_governance(&gov, &true);
}

#[test]
fn set_arbitration_contract_is_governance_only() {
    let env = Env::default();
    let (client, gov, stranger) = setup(&env);
    assert!(client.try_set_arbitration_contract_via_governance(&stranger, &Address::generate(&env)).is_err());
    client.set_arbitration_contract_via_governance(&gov, &Address::generate(&env));
}

#[test]
fn set_dispute_value_threshold_is_governance_only() {
    let env = Env::default();
    let (client, gov, stranger) = setup(&env);
    assert!(client.try_set_dispute_value_threshold_via_governance(&stranger, &1_000i128).is_err());
    client.set_dispute_value_threshold_via_governance(&gov, &1_000i128);
}

#[test]
fn set_metadata_image_uri_is_governance_only() {
    let env = Env::default();
    let (client, gov, stranger) = setup(&env);
    assert!(client.try_set_metadata_image_uri_via_governance(&stranger, &String::from_str(&env, "https://example.com/img.png")).is_err());
    client.set_metadata_image_uri_via_governance(&gov, &String::from_str(&env, "https://example.com/img.png"));
}

#[test]
fn set_min_due_date_window_is_governance_only() {
    let env = Env::default();
    let (client, gov, stranger) = setup(&env);
    assert!(client.try_set_min_due_date_window_via_governance(&stranger, &86_400u64).is_err());
    client.set_min_due_date_window_via_governance(&gov, &86_400u64);
}

#[test]
fn set_max_invoice_amount_is_governance_only() {
    let env = Env::default();
    let (client, gov, stranger) = setup(&env);
    assert!(client.try_set_max_invoice_amount_gov(&stranger, &5_000_000i128).is_err());
    client.set_max_invoice_amount_gov(&gov, &5_000_000i128);
}

#[test]
fn set_max_invoice_amount_rejects_an_out_of_range_value() {
    let env = Env::default();
    let (client, gov, _stranger) = setup(&env);
    assert!(client.try_set_max_invoice_amount_gov(&gov, &0i128).is_err());
}

