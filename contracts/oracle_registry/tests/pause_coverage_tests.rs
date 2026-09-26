#![cfg(test)]

// #1409: oracle_registry has 40 public entrypoints and only a handful of
// `require_not_paused` call sites. This file is a single table-driven test
// documenting, for every *state-changing* entrypoint, whether it is actually
// refused while the registry is paused. Read-only/view entrypoints (getters
// like `get_oracle_info`, `list_active_oracles`, `get_verification_round`,
// `get_round_votes`, `get_oracle_round_history`, `get_oracle_reputation`,
// `get_invoice_contract`, `get_registry_config`, `get_quorum_tiers`,
// `is_paused`, `get_access_control`, `get_governance_address`) are
// intentionally excluded — they don't mutate state and pausing them would
// not protect anything.

use oracle_registry::{OracleRegistryContract, OracleRegistryContractClient, OracleRegistryError};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    token, Address, Env, String,
};

/// Minimal invoice-contract stand-in, same shape as the one used in
/// registration_tests.rs — always reports the invoice as awaiting
/// verification, and accepts the consensus_verify callback unconditionally.
#[contract]
pub struct DummyInvoice;

#[contractimpl]
impl DummyInvoice {
    pub fn consensus_verify(
        _env: Env,
        _id: u64,
        registry: Address,
        _approved: bool,
        _reason: String,
        _oracle_hash: String,
    ) {
        registry.require_auth();
    }

    pub fn get_invoice_verification_state(_env: Env, _id: u64) -> (bool, i128) {
        (true, 0)
    }
}

struct Fixture<'a> {
    client: OracleRegistryContractClient<'a>,
    admin: Address,
    stake_token: Address,
}

fn setup(env: &Env) -> Fixture<'_> {
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);
    let contract_id = env.register(OracleRegistryContract, ());
    let client = OracleRegistryContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let stake_token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let min_stake = 1_000i128;
    client.initialize(&admin, &stake_token, &min_stake);
    let invoice_id = env.register(DummyInvoice, ());
    client.set_invoice_contract(&admin, &invoice_id);
    Fixture {
        client,
        admin,
        stake_token,
    }
}

fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token_id).mint(to, &amount);
}

/// #1409: table-driven pause-surface test. Sets up whatever minimal state
/// each state-changing entrypoint needs to be callable *before* pausing, then
/// pauses, then calls every state-changing entrypoint via its `try_*` variant
/// so a panic-based rejection surfaces as `Err` instead of aborting the test.
#[test]
fn test_pause_surface_across_all_state_changing_entrypoints() {
    let env = Env::default();
    env.mock_all_auths();
    let f = setup(&env);

    // --- Pre-pause state setup ---
    // A registered, active oracle with enough stake to vote/be slashed.
    let oracle = Address::generate(&env);
    mint(&env, &f.stake_token, &oracle, 1_000);
    f.client.register_oracle(&oracle, &1_000);

    // A second oracle used only for deregister_oracle coverage, so we don't
    // disturb `oracle`'s state (needed later for submit_vote/slash_oracle).
    let deregistering_oracle = Address::generate(&env);
    mint(&env, &f.stake_token, &deregistering_oracle, 1_000);
    f.client.register_oracle(&deregistering_oracle, &1_000);

    // An open verification round `oracle` can vote on, and that
    // slash_oracle/expire_round can reference.
    let caller = Address::generate(&env);
    let hash = String::from_str(&env, "h1");
    f.client.open_verification_round(&caller, &7u64, &hash);

    // A second, distinct invoice id for open_verification_round coverage
    // (round 7 is already open, so re-opening it would fail with
    // RoundAlreadyOpen regardless of pause state).
    let hash2 = String::from_str(&env, "h2");

    // access_control / governance trust anchors, bootstrapped pre-pause since
    // their own setters (`set_access_control`, `set_governance_address`) are
    // admin-gated, not pause-gated, and we want the `_via_ac`/`_via_governance`
    // entrypoints to be reachable at all once paused.
    let access_control = Address::generate(&env);
    f.client.set_access_control(&f.admin, &access_control);
    let governance = Address::generate(&env);
    f.client.set_governance_address(&f.admin, &governance);

    // --- Pause ---
    f.client.pause(&f.admin);
    assert!(f.client.is_paused());

    // === Entrypoints that DO call `require_not_paused` — must be refused ===

    assert_eq!(
        f.client
            .try_register_oracle(&Address::generate(&env), &1_000),
        Err(Ok(OracleRegistryError::ContractPaused)),
        "register_oracle is guarded and must be refused while paused"
    );

    assert_eq!(
        f.client.try_deregister_oracle(&deregistering_oracle),
        Err(Ok(OracleRegistryError::ContractPaused)),
        "deregister_oracle is guarded and must be refused while paused"
    );

    assert_eq!(
        f.client.try_open_verification_round(&caller, &8u64, &hash2),
        Err(Ok(OracleRegistryError::ContractPaused)),
        "open_verification_round is guarded and must be refused while paused"
    );

    assert_eq!(
        f.client
            .try_submit_vote(&oracle, &7u64, &true, &String::from_str(&env, "e"),),
        Err(Ok(OracleRegistryError::ContractPaused)),
        "submit_vote is guarded and must be refused while paused"
    );

    assert_eq!(
        f.client
            .try_withdraw_slashed_funds(&f.admin, &Address::generate(&env), &1i128),
        Err(Ok(OracleRegistryError::ContractPaused)),
        "withdraw_slashed_funds is guarded and must be refused while paused"
    );

    // #1371: slash_oracle burns a portion of an operator's stake, so it must
    // also be refused while the registry is paused (via the shared
    // slash_oracle_internal helper).
    assert_eq!(
        f.client.try_slash_oracle(
            &f.admin,
            &oracle,
            &1_000u32,
            &7u64,
            &String::from_str(&env, "evidence"),
        ),
        Err(Ok(OracleRegistryError::ContractPaused)),
        "slash_oracle is guarded and must be refused while paused"
    );

    // #1370: expire_round moves a round to a terminal state, so it must be
    // refused while paused.
    assert_eq!(
        f.client.try_expire_round(&7u64),
        Err(Ok(OracleRegistryError::ContractPaused)),
        "expire_round is guarded and must be refused while paused"
    );

    // === Entrypoints that do NOT call `require_not_paused` — currently pass
    // === through while paused. This is the exact gap #1409 asks us to
    // === surface, not paper over; each is annotated with why.

    // #1371: slash_oracle is now pause-guarded (asserted in the guarded
    // section above), so it no longer appears here as an unguarded gap.

    // admin_resolve_round: no require_not_paused guard either. Round 7 hasn't
    // expired, so the unguarded call still runs its normal logic and fails
    // with RoundNotExpired (not ContractPaused) — proving pause had no effect.
    assert_eq!(
        f.client.try_admin_resolve_round(
            &f.admin,
            &7u64,
            &true,
            &String::from_str(&env, "manual review"),
        ),
        Err(Ok(OracleRegistryError::RoundNotExpired)),
        "admin_resolve_round has no pause guard (#1409 gap) and runs its normal logic while paused"
    );

    // set_invoice_contract: admin-gated config setter, no pause guard.
    f.client
        .set_invoice_contract(&f.admin, &Address::generate(&env));

    // set_treasury: admin-gated config setter, no pause guard.
    f.client.set_treasury(&f.admin, &None);

    // set_registry_config: admin-gated config setter, no pause guard.
    f.client.set_registry_config(
        &f.admin,
        &1_000i128,
        &3u32,
        &6_600u32,
        &(3 * 86_400u64),
        &(7 * 86_400u64),
    );

    // set_quorum_tiers: admin-gated config setter, no pause guard.
    f.client
        .set_quorum_tiers(&f.admin, &soroban_sdk::Vec::new(&env));

    // set_access_control: admin-gated bootstrap setter, no pause guard.
    // Use the pre-pause `access_control` anchor here rather than a fresh
    // address: the `_via_ac` calls below authenticate against the *stored*
    // anchor, and overwriting it with an unknown address would make those
    // calls fail with Unauthorized instead of exercising their pause-surface
    // behavior.
    f.client.set_access_control(&f.admin, &access_control);

    // set_governance_address: admin-gated bootstrap setter, no pause guard.
    // Same anchor-preservation rationale as set_access_control above.
    f.client.set_governance_address(&f.admin, &governance);

    // set_access_control_via_ac: access-control-gated, no pause guard.
    // Re-anchor to the same address (a rotation to a fresh address would
    // invalidate the anchor that every later `_via_ac` call authenticates
    // against).
    f.client
        .set_access_control_via_ac(&access_control, &access_control);

    // set_invoice_contract_via_ac: access-control-gated, no pause guard.
    f.client
        .set_invoice_contract_via_ac(&access_control, &Address::generate(&env));

    // set_treasury_via_ac: access-control-gated, no pause guard.
    f.client.set_treasury_via_ac(&access_control, &None);

    // set_registry_config_via_ac: access-control-gated, no pause guard.
    f.client.set_registry_config_via_ac(
        &access_control,
        &1_000i128,
        &3u32,
        &6_600u32,
        &(3 * 86_400u64),
        &(7 * 86_400u64),
    );

    // set_paused_via_ac: this is the pause switch itself; not expected to be
    // pause-gated. Flip it to false then back to true so the rest of the
    // suite (and any tests appended below) still observe a paused registry.
    f.client.set_paused_via_ac(&access_control, &false);
    f.client.set_paused_via_ac(&access_control, &true);

    // #1371: slash_oracle_via_ac shares slash_oracle_internal, so it is
    // pause-guarded too — must be refused while paused.
    assert_eq!(
        f.client.try_slash_oracle_via_ac(
            &access_control,
            &oracle,
            &1_000u32,
            &7u64,
            &String::from_str(&env, "evidence"),
        ),
        Err(Ok(OracleRegistryError::ContractPaused)),
        "slash_oracle_via_ac is guarded and must be refused while paused"
    );

    // admin_resolve_round_via_ac: access-control-gated, no pause guard. Round
    // 7 still hasn't expired, so this fails on RoundNotExpired regardless of
    // pause state.
    assert_eq!(
        f.client.try_admin_resolve_round_via_ac(
            &access_control,
            &7u64,
            &true,
            &String::from_str(&env, "manual review"),
        ),
        Err(Ok(OracleRegistryError::RoundNotExpired)),
        "admin_resolve_round_via_ac has no pause guard (#1409 gap)"
    );

    // set_invoice_contract_gov: governance-gated, no pause guard.
    f.client
        .set_invoice_contract_gov(&governance, &Address::generate(&env));

    // set_treasury_via_governance: governance-gated, no pause guard.
    f.client.set_treasury_via_governance(&governance, &None);

    // set_registry_config_gov: governance-gated, no pause guard.
    f.client.set_registry_config_gov(
        &governance,
        &1_000i128,
        &3u32,
        &6_600u32,
        &(3 * 86_400u64),
        &(7 * 86_400u64),
    );

    // set_quorum_tiers_via_governance: governance-gated, no pause guard.
    // Unlike set_quorum_tiers, this variant rejects an empty vec, so pass a
    // single valid tier.
    f.client.set_quorum_tiers_via_governance(
        &governance,
        &soroban_sdk::Vec::from_array(
            &env,
            [oracle_registry::QuorumTier {
                min_invoice_amount: 0,
                quorum_bps: 5_000,
            }],
        ),
    );

    // Sanity check: the registry is still paused after all of the above
    // (none of the unguarded calls above touch the Paused flag itself,
    // except the deliberate flip/restore around set_paused_via_ac).
    assert!(f.client.is_paused());
}

/// #1371: a focused regression for the `slash_oracle` pause gap. The broad
/// pause-surface test above asserts the `ContractPaused` rejection; this test
/// additionally proves that (a) no stake is burned while paused, (b) the
/// `_via_ac` path is guarded as well, and (c) the refusal really is the pause
/// gate — unpausing lets the very same call through and only then does the
/// stake get reduced.
#[test]
fn test_slash_oracle_refused_while_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let f = setup(&env);

    // An active oracle with stake, plus an open round a slash can cite.
    let oracle = Address::generate(&env);
    mint(&env, &f.stake_token, &oracle, 1_000);
    f.client.register_oracle(&oracle, &1_000);

    let caller = Address::generate(&env);
    f.client
        .open_verification_round(&caller, &7u64, &String::from_str(&env, "h1"));

    // Bootstrap the access-control trust anchor so the `_via_ac` variant is
    // reachable at all.
    let access_control = Address::generate(&env);
    f.client.set_access_control(&f.admin, &access_control);

    let stake_before = f.client.get_oracle_info(&oracle).unwrap().stake_amount;
    assert_eq!(stake_before, 1_000);

    f.client.pause(&f.admin);
    assert!(f.client.is_paused());

    let evidence = String::from_str(&env, "evidence");

    // Admin path: refused with ContractPaused while paused.
    assert_eq!(
        f.client
            .try_slash_oracle(&f.admin, &oracle, &1_000u32, &7u64, &evidence),
        Err(Ok(OracleRegistryError::ContractPaused)),
        "slash_oracle must be refused while the registry is paused"
    );

    // Access-control path: refused with ContractPaused while paused.
    assert_eq!(
        f.client
            .try_slash_oracle_via_ac(&access_control, &oracle, &1_000u32, &7u64, &evidence),
        Err(Ok(OracleRegistryError::ContractPaused)),
        "slash_oracle_via_ac must be refused while the registry is paused"
    );

    // Nothing was burned by the refused attempts.
    assert_eq!(
        f.client.get_oracle_info(&oracle).unwrap().stake_amount,
        stake_before,
        "refused slashes must not mutate the operator's stake"
    );

    // Unpause: the same call now succeeds, proving the rejection above was the
    // pause gate and not an unrelated validation error.
    f.client.unpause(&f.admin);
    assert!(!f.client.is_paused());

    f.client
        .slash_oracle(&f.admin, &oracle, &1_000u32, &7u64, &evidence);

    // 1_000 bps of 1_000 stake = 100 slashed.
    assert_eq!(
        f.client.get_oracle_info(&oracle).unwrap().stake_amount,
        stake_before - 100
    );
}

/// #1370: `expire_round` must not move a past-deadline round to `Expired`
/// while the registry is paused; once unpaused the same call succeeds.
#[test]
fn test_expire_round_refused_while_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let f = setup(&env);

    let caller = Address::generate(&env);
    f.client
        .open_verification_round(&caller, &7u64, &String::from_str(&env, "h1"));
    let deadline = f.client.get_verification_round(&7u64).unwrap().deadline;

    // Move past the deadline so the round is otherwise expirable.
    env.ledger().with_mut(|l| l.timestamp = deadline + 1);

    f.client.pause(&f.admin);
    assert_eq!(
        f.client.try_expire_round(&7u64),
        Err(Ok(OracleRegistryError::ContractPaused)),
        "expire_round must be refused while the registry is paused"
    );
    assert_eq!(
        f.client.get_verification_round(&7u64).unwrap().status,
        oracle_registry::RoundStatus::Open,
        "a refused expire_round must leave the round open"
    );

    f.client.unpause(&f.admin);
    f.client.expire_round(&7u64);
    assert_eq!(
        f.client.get_verification_round(&7u64).unwrap().status,
        oracle_registry::RoundStatus::Expired
    );
}
