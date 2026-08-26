#![cfg(test)]

use access_control::{
    AccessControlContract, AccessControlContractClient, AccessControlError, ActionPayload,
    ProposalStatus, Role,
};

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec, Address, Env,
};

struct Fixture {
    env: Env,
    client: AccessControlContractClient<'static>,
    contract_id: Address,
    s1: Address,
    s2: Address,
    s3: Address,
}

/// SuperAdmin bootstrapped as 2-of-3 (s1, s2, s3). Every lifecycle-mechanics
/// test below proposes/approves/executes self-management actions under
/// `Role::SuperAdmin` using these three signers — it's the one role that's
/// always configured from `initialize()`, so it doesn't need a target
/// contract to actually call out to (self-management actions only ever
/// mutate this contract's own storage). Real cross-contract dispatch into
/// pool/invoice/credit_score is covered separately in
/// `cross_contract_tests.rs`.
fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(AccessControlContract, ());
    let client = AccessControlContractClient::new(&env, &contract_id);

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let s3 = Address::generate(&env);

    let _ = client.initialize(
        &vec![&env, s1.clone(), s2.clone(), s3.clone()],
        &2,
        &604_800,
    );

    Fixture {
        env,
        client,
        contract_id,
        s1,
        s2,
        s3,
    }
}

#[test]
fn test_initialize_can_only_be_called_once() {
    let f = setup();
    let result = f
        .client
        .try_initialize(&vec![&f.env, f.s1.clone()], &1, &604_800);
    assert_eq!(
        result.unwrap_err().unwrap(),
        AccessControlError::AlreadyInitialized.into()
    );
}

#[test]
fn test_initialize_rejects_threshold_above_signer_count() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(AccessControlContract, ());
    let client = AccessControlContractClient::new(&env, &contract_id);
    let s1 = Address::generate(&env);

    let result = client.try_initialize(&vec![&env, s1], &2, &604_800);
    assert_eq!(
        result.unwrap_err().unwrap(),
        AccessControlError::InvalidThreshold.into()
    );
}

#[test]
fn test_initialize_rejects_duplicate_signers() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(AccessControlContract, ());
    let client = AccessControlContractClient::new(&env, &contract_id);
    let s1 = Address::generate(&env);

    let result = client.try_initialize(&vec![&env, s1.clone(), s1], &1, &604_800);
    assert_eq!(
        result.unwrap_err().unwrap(),
        AccessControlError::DuplicateSigner.into()
    );
}

// ── Bootstrapping: SuperAdmin manages every role, including itself ─────────

#[test]
fn test_super_admin_can_configure_a_new_role_via_its_own_multisig() {
    let f = setup();
    let risk1 = Address::generate(&f.env);
    let risk2 = Address::generate(&f.env);

    // Nothing can be proposed under RiskManager yet — it isn't configured.
    let unconfigured = f.client.try_propose_action(
        &Role::RiskManager,
        &risk1,
        &f.contract_id,
        &ActionPayload::SetYield(500),
    );
    assert_eq!(
        unconfigured.unwrap_err().unwrap(),
        AccessControlError::RoleNotConfigured
    );

    let add1 = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::AddSigner(Role::RiskManager, risk1.clone()),
    );
    let _ = f.client.approve_action(&f.s2, &add1);
    let _ = f.client.execute_action(&f.s1, &add1);

    let add2 = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::AddSigner(Role::RiskManager, risk2.clone()),
    );
    let _ = f.client.approve_action(&f.s2, &add2);
    let _ = f.client.execute_action(&f.s1, &add2);

    let set_threshold = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::SetThreshold(Role::RiskManager, 2),
    );
    let _ = f.client.approve_action(&f.s2, &set_threshold);
    let _ = f.client.execute_action(&f.s1, &set_threshold);

    let config = f.client.get_role_config(&Role::RiskManager).unwrap();
    assert_eq!(config.signers.len(), 2);
    assert_eq!(config.threshold, 2);
    assert!(f.client.is_signer(&Role::RiskManager, &risk1));
    assert!(f.client.is_signer(&Role::RiskManager, &risk2));

    // And that newly-configured role can now genuinely propose its own
    // actions, with its own independent (lower) threshold.
    let proposal = f.client.propose_action(
        &Role::RiskManager,
        &risk1,
        &f.contract_id,
        &ActionPayload::SetYield(500),
    );
    assert_eq!(
        f.client.get_proposal(&proposal).unwrap().status,
        ProposalStatus::Pending
    );
    let _ = f.client.approve_action(&risk2, &proposal);
    assert_eq!(
        f.client.get_proposal(&proposal).unwrap().status,
        ProposalStatus::Approved
    );
}

#[test]
fn test_self_management_action_rejected_under_a_non_super_admin_role() {
    let f = setup();
    let risk1 = Address::generate(&f.env);
    let add = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::AddSigner(Role::RiskManager, risk1.clone()),
    );
    let _ = f.client.approve_action(&f.s2, &add);
    let _ = f.client.execute_action(&f.s1, &add);
    let set_threshold = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::SetThreshold(Role::RiskManager, 1),
    );
    let _ = f.client.approve_action(&f.s2, &set_threshold);
    let _ = f.client.execute_action(&f.s1, &set_threshold);

    // RiskManager's own signer cannot use its role to add itself elsewhere —
    // self-management is SuperAdmin-only, regardless of which role proposes.
    let attempt = f.client.try_propose_action(
        &Role::RiskManager,
        &risk1,
        &f.contract_id,
        &ActionPayload::AddSigner(Role::TreasuryManager, risk1.clone()),
    );
    assert_eq!(
        attempt.unwrap_err().unwrap(),
        AccessControlError::SelfManagementRequiresSuperAdmin
    );
}

#[test]
fn test_remove_signer_rejects_dropping_below_threshold() {
    let f = setup();
    // SuperAdmin is 2-of-3 (s1, s2, s3). Removing one signer leaves 2
    // signers for a threshold of 2 — still valid — but removing a second
    // would drop to 1 signer under a threshold of 2, which must be rejected.
    let remove_s3 = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::RemoveSigner(Role::SuperAdmin, f.s3.clone()),
    );
    let _ = f.client.approve_action(&f.s2, &remove_s3);
    let _ = f.client.execute_action(&f.s1, &remove_s3);
    assert_eq!(
        f.client
            .get_role_config(&Role::SuperAdmin)
            .unwrap()
            .signers
            .len(),
        2
    );

    let remove_s2 = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::RemoveSigner(Role::SuperAdmin, f.s2.clone()),
    );
    let _ = f.client.approve_action(&f.s2, &remove_s2);
    let result = f.client.try_execute_action(&f.s1, &remove_s2);
    assert_eq!(
        result.unwrap_err().unwrap(),
        AccessControlError::InvalidThreshold.into()
    );
}

// ── Core proposal lifecycle ─────────────────────────────────────────────────

#[test]
fn test_full_lifecycle_propose_approve_execute() {
    let f = setup();
    let target = Address::generate(&f.env);

    let proposal_id = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::AddSigner(Role::OracleManager, target.clone()),
    );
    let proposal = f.client.get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.approvals.len(), 1); // proposer auto-approved
    assert_eq!(proposal.status, ProposalStatus::Pending);

    let _ = f.client.approve_action(&f.s2, &proposal_id);
    let proposal = f.client.get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.approvals.len(), 2);
    assert_eq!(proposal.status, ProposalStatus::Approved);

    let _ = f.client.execute_action(&f.s1, &proposal_id);
    let executed = f.client.get_proposal(&proposal_id).unwrap();
    assert_eq!(executed.status, ProposalStatus::Executed);
    assert!(f.client.is_signer(&Role::OracleManager, &target));
}

#[test]
fn test_single_compromised_signer_cannot_execute_alone_at_threshold_two() {
    let f = setup();
    let target = Address::generate(&f.env);

    // SuperAdmin requires 2-of-3. A single signer (even one raising and
    // "approving" via their own proposal auto-approval) never reaches that
    // threshold alone.
    let proposal_id = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::AddSigner(Role::OracleManager, target.clone()),
    );
    let result = f.client.try_execute_action(&f.s1, &proposal_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        AccessControlError::ProposalNotApproved.into()
    );

    let proposal = f.client.get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Pending);
    assert!(!f.client.is_signer(&Role::OracleManager, &target));
}

#[test]
fn test_duplicate_approval_rejected() {
    let f = setup();
    let target = Address::generate(&f.env);
    let proposal_id = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::AddSigner(Role::OracleManager, target),
    );
    // s1 is already recorded as an approval via propose_action; approving
    // again must fail rather than double-counting.
    let result = f.client.try_approve_action(&f.s1, &proposal_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        AccessControlError::AlreadyApproved.into()
    );
}

#[test]
fn test_non_signer_cannot_approve() {
    let f = setup();
    let target = Address::generate(&f.env);
    let outsider = Address::generate(&f.env);
    let proposal_id = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::AddSigner(Role::OracleManager, target),
    );
    let result = f.client.try_approve_action(&outsider, &proposal_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        AccessControlError::NotASigner.into()
    );
}

#[test]
fn test_reject_action_blocks_further_approval_and_execution() {
    let f = setup();
    let target = Address::generate(&f.env);
    let proposal_id = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::AddSigner(Role::OracleManager, target),
    );
    let _ = f.client.reject_action(&f.s2, &proposal_id);

    let rejected = f.client.get_proposal(&proposal_id).unwrap();
    assert_eq!(rejected.status, ProposalStatus::Rejected);

    let approve_after_reject = f.client.try_approve_action(&f.s2, &proposal_id);
    assert_eq!(
        approve_after_reject.unwrap_err().unwrap(),
        AccessControlError::ProposalNotPending.into()
    );
    let execute_after_reject = f.client.try_execute_action(&f.s1, &proposal_id);
    assert_eq!(
        execute_after_reject.unwrap_err().unwrap(),
        AccessControlError::ProposalNotApproved.into()
    );
}

#[test]
fn test_revoke_approval_removes_own_approval_only() {
    let f = setup();
    let target = Address::generate(&f.env);
    let proposal_id = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::AddSigner(Role::OracleManager, target),
    );
    let _ = f.client.approve_action(&f.s2, &proposal_id);
    assert_eq!(
        f.client.get_proposal(&proposal_id).unwrap().approvals.len(),
        2
    );

    let _ = f.client.revoke_approval(&f.s2, &proposal_id);
    let after = f.client.get_proposal(&proposal_id).unwrap();
    assert_eq!(after.approvals.len(), 1);
    assert!(after.approvals.contains(&f.s1));
    assert!(!after.approvals.contains(&f.s2));
    // Revoking one approval un-does the threshold too — it must go back to
    // Pending, not stay Approved with a stale approval count.
    assert_eq!(after.status, ProposalStatus::Pending);
}

#[test]
fn test_revoke_approval_rejects_when_no_prior_approval() {
    let f = setup();
    let target = Address::generate(&f.env);
    let proposal_id = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::AddSigner(Role::OracleManager, target),
    );
    let result = f.client.try_revoke_approval(&f.s3, &proposal_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        AccessControlError::NoApprovalToRevoke.into()
    );
}

#[test]
fn test_proposal_expires_and_cannot_be_approved_or_executed_after_window() {
    let f = setup();
    let target = Address::generate(&f.env);
    let proposal_id = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::AddSigner(Role::OracleManager, target),
    );
    f.env.ledger().with_mut(|li| li.timestamp += 604_800 + 1);

    let approve_result = f.client.try_approve_action(&f.s2, &proposal_id);
    assert_eq!(
        approve_result.unwrap_err().unwrap(),
        AccessControlError::ProposalExpired.into()
    );
}

#[test]
fn test_expired_approved_proposal_cannot_execute() {
    let f = setup();
    let target = Address::generate(&f.env);
    let proposal_id = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::AddSigner(Role::OracleManager, target),
    );
    let _ = f.client.approve_action(&f.s2, &proposal_id);
    assert_eq!(
        f.client.get_proposal(&proposal_id).unwrap().status,
        ProposalStatus::Approved
    );

    f.env.ledger().with_mut(|li| li.timestamp += 604_800 + 1);
    let result = f.client.try_execute_action(&f.s1, &proposal_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        AccessControlError::ProposalExpired.into()
    );
}

// ── #1135: payload/target coherence validation ────────────────────────────

/// A self-management payload (AddSigner) proposed against an external address
/// must be rejected immediately — executing it against `this_contract != target`
/// would route to `execute_cross_contract` whose self-management catch-all arm
/// silently does nothing, leaving the proposal with `Executed` status but zero
/// real effect.
#[test]
fn test_self_management_payload_with_external_target_is_rejected() {
    let f = setup();
    let external = Address::generate(&f.env);
    let victim = Address::generate(&f.env);

    let result = f.client.try_propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &external, // wrong: self-management must target this_contract
        &ActionPayload::AddSigner(Role::TreasuryManager, victim),
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        AccessControlError::IncoherentProposal
    );
}

/// A RemoveSigner payload proposed against an external address must likewise
/// be rejected — same silent-no-op risk as AddSigner above.
#[test]
fn test_remove_signer_payload_with_external_target_is_rejected() {
    let f = setup();
    let external = Address::generate(&f.env);

    let result = f.client.try_propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &external,
        &ActionPayload::RemoveSigner(Role::SuperAdmin, f.s2.clone()),
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        AccessControlError::IncoherentProposal
    );
}

/// A SetThreshold payload proposed against an external address must be
/// rejected — same silent-no-op risk.
#[test]
fn test_set_threshold_payload_with_external_target_is_rejected() {
    let f = setup();
    let external = Address::generate(&f.env);

    let result = f.client.try_propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &external,
        &ActionPayload::SetThreshold(Role::SuperAdmin, 1),
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        AccessControlError::IncoherentProposal
    );
}

/// A cross-contract payload (SetYield) proposed with `target == this_contract`
/// must be rejected — executing it would route to `execute_self_management`
/// whose `_ => Ok(())` catch-all silently does nothing, leaving the proposal
/// with `Executed` status but zero real effect on the intended external target.
#[test]
fn test_cross_contract_payload_with_self_as_target_is_rejected() {
    let f = setup();

    let result = f.client.try_propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id, // wrong: cross-contract payload must NOT target this_contract
        &ActionPayload::SetYield(500),
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        AccessControlError::IncoherentProposal
    );
}

/// Another cross-contract payload (SetPaused) with this_contract as target —
/// verifies the check applies regardless of which pool action is used.
#[test]
fn test_set_paused_payload_with_self_as_target_is_rejected() {
    let f = setup();

    let result = f.client.try_propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::SetPaused(true),
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        AccessControlError::IncoherentProposal
    );
}

/// Coherent self-management proposals (target == this_contract) must still work
/// correctly — regression guard to confirm the validation only blocks bad cases.
#[test]
fn test_coherent_self_management_proposal_is_accepted() {
    let f = setup();
    let new_signer = Address::generate(&f.env);

    // This is the correct pairing: AddSigner payload + this_contract as target.
    let proposal_id = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::AddSigner(Role::OracleManager, new_signer.clone()),
    );
    assert_eq!(
        f.client.get_proposal(&proposal_id).unwrap().status,
        ProposalStatus::Pending
    );
}

/// Coherent cross-contract proposals (target != this_contract) must still work
/// correctly — regression guard to confirm the validation only blocks bad cases.
#[test]
fn test_coherent_cross_contract_proposal_is_accepted() {
    let f = setup();
    let external = Address::generate(&f.env);

    // This is the correct pairing: SetYield payload + an external target.
    let proposal_id = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &external,
        &ActionPayload::SetYield(800),
    );
    assert_eq!(
        f.client.get_proposal(&proposal_id).unwrap().status,
        ProposalStatus::Pending
    );
}

// ── #1136: update proposal_expiry_secs after initialize ───────────────────

/// SuperAdmin can update the expiry window via the multisig lifecycle, and
/// newly created proposals use the new window immediately.
#[test]
fn test_super_admin_can_update_proposal_expiry_secs() {
    let f = setup();
    assert_eq!(f.client.get_proposal_expiry_secs(), 604_800);

    // Propose + approve + execute a new 7-day window of 3600s (1 hour).
    let new_expiry: u64 = 3_600;
    let proposal_id = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::SetProposalExpiry(new_expiry),
    );
    let _ = f.client.approve_action(&f.s2, &proposal_id);
    let _ = f.client.execute_action(&f.s1, &proposal_id);

    // Accessor reflects the new value.
    assert_eq!(f.client.get_proposal_expiry_secs(), new_expiry);

    // A proposal created after the change uses the new window.
    let target = Address::generate(&f.env);
    let pid2 = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::AddSigner(Role::OracleManager, target.clone()),
    );
    let proposal = f.client.get_proposal(&pid2).unwrap();
    // expires_at should be approximately created_at + 3600, not + 604800.
    assert_eq!(proposal.expires_at, proposal.created_at + new_expiry);
}

/// SetProposalExpiry(0) must be rejected at execution time — zero is not a
/// valid expiry window (mirrors the check in `initialize`).
#[test]
fn test_set_proposal_expiry_rejects_zero() {
    let f = setup();

    let proposal_id = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::SetProposalExpiry(0),
    );
    let _ = f.client.approve_action(&f.s2, &proposal_id);

    let result = f.client.try_execute_action(&f.s1, &proposal_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        AccessControlError::InvalidExpiryWindow.into()
    );
    // Value must be unchanged after a failed execution.
    assert_eq!(f.client.get_proposal_expiry_secs(), 604_800);
}

/// SetProposalExpiry proposed under a non-SuperAdmin role must be rejected —
/// same SuperAdmin-only gate that protects AddSigner / SetThreshold.
#[test]
fn test_set_proposal_expiry_requires_super_admin() {
    let f = setup();

    // Bootstrap a RiskManager with threshold 1 so it can propose.
    let risk1 = Address::generate(&f.env);
    let add = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::AddSigner(Role::RiskManager, risk1.clone()),
    );
    let _ = f.client.approve_action(&f.s2, &add);
    let _ = f.client.execute_action(&f.s1, &add);
    let set_t = f.client.propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &f.contract_id,
        &ActionPayload::SetThreshold(Role::RiskManager, 1),
    );
    let _ = f.client.approve_action(&f.s2, &set_t);
    let _ = f.client.execute_action(&f.s1, &set_t);

    // RiskManager must not be able to change the expiry window.
    let result = f.client.try_propose_action(
        &Role::RiskManager,
        &risk1,
        &f.contract_id,
        &ActionPayload::SetProposalExpiry(1_800),
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        AccessControlError::SelfManagementRequiresSuperAdmin
    );
}

/// SetProposalExpiry proposed with an external target must be rejected —
/// same coherence check that protects AddSigner / SetThreshold.
#[test]
fn test_set_proposal_expiry_with_external_target_is_rejected() {
    let f = setup();
    let external = Address::generate(&f.env);

    let result = f.client.try_propose_action(
        &Role::SuperAdmin,
        &f.s1,
        &external, // wrong: must target this_contract
        &ActionPayload::SetProposalExpiry(3_600),
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        AccessControlError::IncoherentProposal
    );
}
