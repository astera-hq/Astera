#![cfg(test)]

//! Tests for the governance proposal → vote → timelock → execute cycle
//! Tests governance-gated parameter changes across pool, invoice, oracle_registry, and compliance contracts

use governance::{Governance, GovernanceClient, GovernanceError, GovernanceAction, ProposalCategory};
use pool::{Pool, PoolClient};
use invoice::{Invoice, InvoiceClient};
use oracle_registry::{OracleRegistry, OracleRegistryClient};
use compliance::{Compliance, ComplianceClient};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

struct Fixture {
    env: Env,
    governance_client: GovernanceClient<'static>,
    governance_id: Address,
    pool_id: Address,
    pool_client: PoolClient<'static>,
    invoice_id: Address,
    invoice_client: InvoiceClient<'static>,
    oracle_registry_id: Address,
    oracle_registry_client: OracleRegistryClient<'static>,
    compliance_id: Address,
    compliance_client: ComplianceClient<'static>,
    admin: Address,
    voter: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let voter = Address::generate(&env);
    let share_token = Address::generate(&env);

    // Register contracts
    let governance_id = env.register(Governance, ());
    let pool_id = env.register(Pool, ());
    let invoice_id = env.register(Invoice, ());
    let oracle_registry_id = env.register(OracleRegistry, ());
    let compliance_id = env.register(Compliance, ());

    let governance_client = GovernanceClient::new(&env, &governance_id);
    let pool_client = PoolClient::new(&env, &pool_id);
    let invoice_client = InvoiceClient::new(&env, &invoice_id);
    let oracle_registry_client = OracleRegistryClient::new(&env, &oracle_registry_id);
    let compliance_client = ComplianceClient::new(&env, &compliance_id);

    // Initialize governance
    governance_client.initialize(
        &admin,
        &share_token,
        &86400u64,  // 1 day min voting period
        &86400u64,  // 1 day default execution delay
        &5000u32,  // 50% parameter change quorum
        &6000u32,  // 60% treasury quorum
        &8000u32,  // 80% critical quorum
        &5000u32,  // 50% parameter change pass
        &6000u32,  // 60% treasury pass
        &8000u32,  // 80% critical pass
        &604800u64, // 7 days execution expiry
    );

    // Bootstrap governance addresses on target contracts
    pool_client.set_governance_address(&admin, &governance_id);
    invoice_client.set_governance_address(&admin, &governance_id);
    oracle_registry_client.set_governance_address(&admin, &governance_id);
    compliance_client.set_governance_address(&admin, &governance_id);

    // Initialize pool (minimal setup for testing)
    pool_client.initialize(
        &admin,
        &invoice_id,
        &Address::generate(&env), // treasury
        &1000u32, // yield bps
        &50u32,   // factoring fee bps
    );

    // Initialize invoice (minimal setup)
    invoice_client.initialize(&admin, &pool_id, &oracle_registry_id);

    // Initialize oracle registry (minimal setup)
    oracle_registry_client.initialize(&admin, &invoice_id);

    // Initialize compliance (minimal setup)
    compliance_client.initialize(&admin);

    Fixture {
        env,
        governance_client,
        governance_id,
        pool_id,
        pool_client,
        invoice_id,
        invoice_client,
        oracle_registry_id,
        oracle_registry_client,
        compliance_id,
        compliance_client,
        admin,
        voter,
    }
}

// ── Basic Governance Flow Tests ─────────────────────────────────────────────

#[test]
fn test_create_proposal() {
    let f = setup();
    
    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &"Test proposal",
        &f.pool_id,
        &GovernanceAction::SetPoolYield(1500u32),
        &ProposalCategory::ParameterChange,
        &86400u64, // voting period
        &86400u64, // execution delay
    );

    let proposal = f.governance_client.get_proposal(&proposal_id);
    assert_eq!(proposal.proposer, f.admin);
    assert_eq!(proposal.description, "Test proposal");
    assert_eq!(proposal.target_contract, f.pool_id);
}

#[test]
fn test_vote_on_proposal() {
    let f = setup();
    
    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &"Test proposal",
        &f.pool_id,
        &GovernanceAction::SetPoolYield(1500u32),
        &ProposalCategory::ParameterChange,
        &86400u64,
        &86400u64,
    );

    f.governance_client.vote(&f.voter, &proposal_id, &true, &1000i128);
    
    let proposal = f.governance_client.get_proposal(&proposal_id);
    assert_eq!(proposal.votes_for, 1000i128);
    assert_eq!(proposal.votes_against, 0i128);
}

#[test]
fn test_execute_proposal_after_timelock() {
    let f = setup();
    
    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &"Test proposal",
        &f.pool_id,
        &GovernanceAction::SetPoolYield(1500u32),
        &ProposalCategory::ParameterChange,
        &86400u64,
        &86400u64,
    );

    // Vote to pass
    f.governance_client.vote(&f.voter, &proposal_id, &true, &1000i128);

    // Fast-forward past voting period and timelock
    f.env.ledger().set_timestamp(200000);

    f.governance_client.execute_proposal(&f.admin, &proposal_id);
    
    let proposal = f.governance_client.get_proposal(&proposal_id);
    assert_eq!(proposal.status, 3); // Executed
}

// ── Pool Parameter Change Tests ─────────────────────────────────────────────

#[test]
fn test_set_pool_yield_via_governance() {
    let f = setup();
    
    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &"Update pool yield",
        &f.pool_id,
        &GovernanceAction::SetPoolYield(1500u32),
        &ProposalCategory::ParameterChange,
        &86400u64,
        &86400u64,
    );

    f.governance_client.vote(&f.voter, &proposal_id, &true, &1000i128);
    f.env.ledger().set_timestamp(200000);
    f.governance_client.execute_proposal(&f.admin, &proposal_id);

    let config = f.pool_client.get_config();
    assert_eq!(config.yield_bps, 1500u32);
}

#[test]
fn test_set_pool_treasury_via_governance() {
    let f = setup();
    let new_treasury = Address::generate(&f.env);
    
    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &"Update pool treasury",
        &f.pool_id,
        &GovernanceAction::SetPoolTreasury(new_treasury.clone()),
        &ProposalCategory::ParameterChange,
        &86400u64,
        &86400u64,
    );

    f.governance_client.vote(&f.voter, &proposal_id, &true, &1000i128);
    f.env.ledger().set_timestamp(200000);
    f.governance_client.execute_proposal(&f.admin, &proposal_id);

    let config = f.pool_client.get_config();
    assert_eq!(config.treasury, new_treasury);
}

#[test]
fn test_set_pool_fee_tier_via_governance() {
    let f = setup();
    
    let fee_tier = governance::FeeTier {
        min_amount: 1000i128,
        max_amount: 10000i128,
        min_credit_score: 700u32,
        fee_bps: 50u32,
    };
    
    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &"Update fee tier",
        &f.pool_id,
        &GovernanceAction::SetPoolFeeTier(1u32, fee_tier.clone()),
        &ProposalCategory::ParameterChange,
        &86400u64,
        &86400u64,
    );

    f.governance_client.vote(&f.voter, &proposal_id, &true, &1000i128);
    f.env.ledger().set_timestamp(200000);
    f.governance_client.execute_proposal(&f.admin, &proposal_id);

    let retrieved_tier = f.pool_client.get_fee_tier(&1u32);
    assert_eq!(retrieved_tier.min_amount, 1000i128);
    assert_eq!(retrieved_tier.fee_bps, 50u32);
}

#[test]
fn test_set_collateral_config_via_governance() {
    let f = setup();
    
    let collateral_config = governance::CollateralConfig {
        threshold: 100000i128,
        collateral_bps: 150u32,
    };
    
    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &"Update collateral config",
        &f.pool_id,
        &GovernanceAction::SetPoolCollateralConfig(collateral_config.clone()),
        &ProposalCategory::ParameterChange,
        &86400u64,
        &86400u64,
    );

    f.governance_client.vote(&f.voter, &proposal_id, &true, &1000i128);
    f.env.ledger().set_timestamp(200000);
    f.governance_client.execute_proposal(&f.admin, &proposal_id);

    let config = f.pool_client.get_collateral_config();
    assert_eq!(config.threshold, 100000i128);
    assert_eq!(config.collateral_bps, 150u32);
}

// ── Invoice Parameter Change Tests ───────────────────────────────────────────

#[test]
fn test_set_invoice_grace_period_via_governance() {
    let f = setup();
    
    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &"Update grace period",
        &f.invoice_id,
        &GovernanceAction::SetInvoiceGracePeriod(7u32),
        &ProposalCategory::ParameterChange,
        &86400u64,
        &86400u64,
    );

    f.governance_client.vote(&f.voter, &proposal_id, &true, &1000i128);
    f.env.ledger().set_timestamp(200000);
    f.governance_client.execute_proposal(&f.admin, &proposal_id);

    let config = f.invoice_client.get_config();
    assert_eq!(config.grace_period_days, 7u32);
}

#[test]
fn test_set_invoice_max_amount_via_governance() {
    let f = setup();
    
    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &"Update max invoice amount",
        &f.invoice_id,
        &GovernanceAction::SetInvoiceMaxAmount(1000000i128),
        &ProposalCategory::ParameterChange,
        &86400u64,
        &86400u64,
    );

    f.governance_client.vote(&f.voter, &proposal_id, &true, &1000i128);
    f.env.ledger().set_timestamp(200000);
    f.governance_client.execute_proposal(&f.admin, &proposal_id);

    let config = f.invoice_client.get_config();
    assert_eq!(config.max_invoice_amount, 1000000i128);
}

// ── Oracle Registry Parameter Change Tests ───────────────────────────────────

#[test]
fn test_set_oracle_registry_invoice_contract_via_governance() {
    let f = setup();
    let new_invoice_contract = Address::generate(&f.env);
    
    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &"Update invoice contract",
        &f.oracle_registry_id,
        &GovernanceAction::SetOracleRegistryInvoiceContract(new_invoice_contract.clone()),
        &ProposalCategory::ParameterChange,
        &86400u64,
        &86400u64,
    );

    f.governance_client.vote(&f.voter, &proposal_id, &true, &1000i128);
    f.env.ledger().set_timestamp(200000);
    f.governance_client.execute_proposal(&f.admin, &proposal_id);

    let config = f.oracle_registry_client.get_config();
    assert_eq!(config.invoice_contract, new_invoice_contract);
}

#[test]
fn test_set_oracle_registry_quorum_tiers_via_governance() {
    let f = setup();
    
    let quorum_tier = governance::QuorumTier {
        min_invoice_amount: 1000i128,
        quorum_bps: 6000u32,
    };
    
    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &"Update quorum tiers",
        &f.oracle_registry_id,
        &GovernanceAction::SetOracleRegistryQuorumTiers(vec![&f.env, quorum_tier.clone()]),
        &ProposalCategory::ParameterChange,
        &86400u64,
        &86400u64,
    );

    f.governance_client.vote(&f.voter, &proposal_id, &true, &1000i128);
    f.env.ledger().set_timestamp(200000);
    f.governance_client.execute_proposal(&f.admin, &proposal_id);

    let tiers = f.oracle_registry_client.get_quorum_tiers();
    assert_eq!(tiers.len(), 1);
    assert_eq!(tiers[0].quorum_bps, 6000u32);
}

// ── Compliance Parameter Change Tests ─────────────────────────────────────────

#[test]
fn test_set_compliance_rescreening_interval_via_governance() {
    let f = setup();
    
    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &"Update rescreening interval",
        &f.compliance_id,
        &GovernanceAction::SetComplianceRescreeningInterval(604800u64), // 7 days
        &ProposalCategory::ParameterChange,
        &86400u64,
        &86400u64,
    );

    f.governance_client.vote(&f.voter, &proposal_id, &true, &1000i128);
    f.env.ledger().set_timestamp(200000);
    f.governance_client.execute_proposal(&f.admin, &proposal_id);

    let config = f.compliance_client.get_config();
    assert_eq!(config.rescreening_interval_secs, 604800u64);
}

#[test]
fn test_set_compliance_screener_timelock_via_governance() {
    let f = setup();
    
    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &"Update screener timelock",
        &f.compliance_id,
        &GovernanceAction::SetComplianceScreenerTimelock(3600u64), // 1 hour
        &ProposalCategory::ParameterChange,
        &86400u64,
        &86400u64,
    );

    f.governance_client.vote(&f.voter, &proposal_id, &true, &1000i128);
    f.env.ledger().set_timestamp(200000);
    f.governance_client.execute_proposal(&f.admin, &proposal_id);

    let config = f.compliance_client.get_config();
    assert_eq!(config.screener_timelock_secs, 3600u64);
}

// ── Governance Gating Tests ─────────────────────────────────────────────────

#[test]
fn test_governance_gated_setters_reject_non_governance_caller() {
    let f = setup();
    let impostor = Address::generate(&f.env);
    
    // Try to call governance-gated setter directly without governance
    let result = f.pool_client.try_set_yield_via_governance(&impostor, &1500u32);
    assert_eq!(result.unwrap_err().unwrap(), pool::PoolError::GovernanceNotConfigured);
}

#[test]
fn test_governance_gated_setters_require_governance_address() {
    let f = setup();
    
    // Remove governance address from pool
    f.pool_client.set_governance_address(&f.admin, &Address::generate(&f.env));
    
    // Governance contract should fail to call setter
    let result = f.pool_client.try_set_yield_via_governance(&f.governance_id, &1500u32);
    assert_eq!(result.unwrap_err().unwrap(), pool::PoolError::GovernanceNotConfigured);
}

// ── Quorum and Pass Threshold Tests ─────────────────────────────────────────

#[test]
fn test_proposal_rejected_when_quorum_not_met() {
    let f = setup();
    
    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &"Test proposal",
        &f.pool_id,
        &GovernanceAction::SetPoolYield(1500u32),
        &ProposalCategory::ParameterChange,
        &86400u64,
        &86400u64,
    );

    // Vote with insufficient shares to meet quorum
    f.governance_client.vote(&f.voter, &proposal_id, &true, &100i128);
    f.env.ledger().set_timestamp(200000);

    // Should fail to execute due to quorum not met
    let result = f.governance_client.try_execute_proposal(&f.admin, &proposal_id);
    assert_eq!(result.unwrap_err().unwrap(), GovernanceError::QuorumNotMet);
}

#[test]
fn test_proposal_rejected_when_pass_threshold_not_met() {
    let f = setup();
    
    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &"Test proposal",
        &f.pool_id,
        &GovernanceAction::SetPoolYield(1500u32),
        &ProposalCategory::ParameterChange,
        &86400u64,
        &86400u64,
    );

    // Vote against (pass threshold not met)
    f.governance_client.vote(&f.voter, &proposal_id, &false, &1000i128);
    f.env.ledger().set_timestamp(200000);

    let result = f.governance_client.try_execute_proposal(&f.admin, &proposal_id);
    assert_eq!(result.unwrap_err().unwrap(), GovernanceError::QuorumNotMet); // Also fails quorum/pass check
}

#[test]
fn test_proposal_cannot_execute_before_timelock() {
    let f = setup();
    
    let proposal_id = f.governance_client.create_proposal(
        &f.admin,
        &"Test proposal",
        &f.pool_id,
        &GovernanceAction::SetPoolYield(1500u32),
        &ProposalCategory::ParameterChange,
        &86400u64,
        &86400u64,
    );

    f.governance_client.vote(&f.voter, &proposal_id, &true, &1000i128);
    
    // Don't fast-forward - timelock still active
    let result = f.governance_client.try_execute_proposal(&f.admin, &proposal_id);
    assert_eq!(result.unwrap_err().unwrap(), GovernanceError::TimelockActive);
}
