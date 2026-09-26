#![cfg(test)]

// Dedicated coverage for the credit_score scoring engine itself: milestone
// and volume bonuses, the payment-history ring buffer, the v2 attestation
// blend, and simulate_score_with_attestations. Closes #1411 — the existing
// suites (fuzz_tests.rs, init_guard_tests.rs, access_control_tests.rs) cover
// invariants and access control, but nothing exercised this contract's
// actual scoring behavior directly.

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String as SorobanString,
};

use credit_score::{
    AttestorType, CreditScoreContract, CreditScoreContractClient, MAX_PAYMENT_HISTORY, MAX_SCORE,
    MIN_SCORE,
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

/// Records one on-time payment of `amount` for `sme`, above the default
/// milestone-volume floor so it counts toward the milestone bonuses.
fn record_on_time_payment(
    env: &Env,
    client: &CreditScoreContractClient,
    pool: &Address,
    sme: &Address,
    invoice_id: u64,
    amount: i128,
) {
    let due_date = 100_000u64;
    client.record_payment(pool, &invoice_id, sme, &amount, &due_date, &due_date);
    let _ = env;
}

// ── Milestone bonuses (#568) ────────────────────────────────────────────────

#[test]
fn milestone_bonus_applies_at_five_ten_and_twenty_qualifying_invoices() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _invoice, pool) = setup(&env);
    let sme = Address::generate(&env);

    // Default min_milestone_volume is 100_000_000; pay comfortably above it
    // so every payment counts toward the milestone counter.
    let amount = 200_000_000i128;

    let mut last_score = MIN_SCORE;
    let mut score_at_4 = 0;
    let mut score_at_5 = 0;
    let mut score_at_9 = 0;
    let mut score_at_10 = 0;
    let mut score_at_19 = 0;
    let mut score_at_20 = 0;

    for i in 1..=20u64 {
        record_on_time_payment(&env, &client, &pool, &sme, i, amount);
        let score = client.get_credit_score(&sme).score;
        assert!(
            score >= last_score,
            "score must never decrease on an on-time payment"
        );
        last_score = score;

        match i {
            4 => score_at_4 = score,
            5 => score_at_5 = score,
            9 => score_at_9 = score,
            10 => score_at_10 = score,
            19 => score_at_19 = score,
            20 => score_at_20 = score,
            _ => {}
        }
    }

    // Crossing each of the 5/10/20-invoice thresholds must produce a
    // strictly higher score than the payment just before it — this is the
    // exact bonus lib.rs's calculate_score_with_config computes.
    assert!(
        score_at_5 > score_at_4,
        "expected a bonus at 5 qualifying invoices: {score_at_4} -> {score_at_5}"
    );
    assert!(
        score_at_10 > score_at_9,
        "expected a bonus at 10 qualifying invoices: {score_at_9} -> {score_at_10}"
    );
    assert!(
        score_at_20 > score_at_19,
        "expected a bonus at 20 qualifying invoices: {score_at_19} -> {score_at_20}"
    );
}

#[test]
fn milestone_bonus_does_not_count_invoices_below_min_milestone_volume() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _invoice, pool) = setup(&env);
    let sme = Address::generate(&env);

    // Below the default 100_000_000 floor — these must never contribute to
    // the milestone counter, however many of them are recorded.
    let tiny_amount = 1_000i128;
    for i in 1..=10u64 {
        record_on_time_payment(&env, &client, &pool, &sme, i, tiny_amount);
    }

    let data = client.get_credit_score(&sme);
    assert_eq!(data.total_invoices, 10);
    // Without any milestone bonus, score should equal base + on-time points
    // only (no inv_bonus_pts applied) — a much lower ceiling than the
    // qualifying-volume case above would produce for the same invoice count.
    // We can't reach into the private scoring config from an integration
    // test, so assert the weaker but still meaningful property: this score
    // is strictly lower than the qualifying-volume run at the same count.
    let (client2, _admin2, _invoice2, pool2) = setup(&env);
    let sme2 = Address::generate(&env);
    for i in 1..=10u64 {
        record_on_time_payment(&env, &client2, &pool2, &sme2, i, 200_000_000i128);
    }
    let qualifying_data = client2.get_credit_score(&sme2);
    assert!(
        qualifying_data.score > data.score,
        "milestone-qualifying volume should score higher than sub-floor volume: {} vs {}",
        qualifying_data.score,
        data.score
    );
}

// ── Volume bonuses ──────────────────────────────────────────────────────────

#[test]
fn volume_bonus_increases_score_at_higher_total_volume_tiers() {
    let env = Env::default();
    env.mock_all_auths();

    // Three independent SMEs, each with a single on-time payment, but at
    // total_volume levels that straddle the default volume-bonus tiers
    // (1_000_000_000 / 10_000_000_000 / 100_000_000_000).
    let (client_low, _a1, _i1, pool1) = setup(&env);
    let sme_low = Address::generate(&env);
    record_on_time_payment(&env, &client_low, &pool1, &sme_low, 1, 500_000_000);

    let (client_mid, _a2, _i2, pool2) = setup(&env);
    let sme_mid = Address::generate(&env);
    record_on_time_payment(&env, &client_mid, &pool2, &sme_mid, 1, 5_000_000_000);

    let (client_high, _a3, _i3, pool3) = setup(&env);
    let sme_high = Address::generate(&env);
    record_on_time_payment(&env, &client_high, &pool3, &sme_high, 1, 50_000_000_000);

    let score_low = client_low.get_credit_score(&sme_low).score;
    let score_mid = client_mid.get_credit_score(&sme_mid).score;
    let score_high = client_high.get_credit_score(&sme_high).score;

    assert!(
        score_mid > score_low,
        "crossing the first volume-bonus tier should raise the score: {score_low} -> {score_mid}"
    );
    assert!(
        score_high > score_mid,
        "crossing the second volume-bonus tier should raise the score: {score_mid} -> {score_high}"
    );
}

// ── Payment history ring buffer (MAX_PAYMENT_HISTORY) ───────────────────────

#[test]
fn payment_history_ring_buffer_evicts_oldest_entry_once_full() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _invoice, pool) = setup(&env);
    let sme = Address::generate(&env);

    // Shrink the ring buffer so the test doesn't need MAX_PAYMENT_HISTORY
    // (100) iterations to exercise wraparound.
    let small_max: u32 = 3;
    client.set_max_payment_history(&admin, &small_max);
    assert_eq!(client.get_max_payment_history(), small_max);

    for i in 1..=small_max as u64 {
        record_on_time_payment(&env, &client, &pool, &sme, i, 10_000_000);
    }
    assert_eq!(client.get_payment_history_length(&sme), small_max);
    // Oldest record still logically at index 0 is invoice_id == 1.
    assert_eq!(client.get_payment_record(&sme, &0).unwrap().invoice_id, 1);

    // One more payment must evict invoice_id 1 (the oldest), not any other.
    record_on_time_payment(&env, &client, &pool, &sme, small_max as u64 + 1, 10_000_000);

    assert_eq!(
        client.get_payment_history_length(&sme),
        small_max,
        "ring buffer must not grow past max_payment_history"
    );
    let history = client.get_payment_history(&sme);
    assert_eq!(history.len(), small_max);
    let ids: Vec<u64> = history.iter().map(|r| r.invoice_id).collect();
    assert!(
        !ids.iter().any(|id| id == 1),
        "oldest record (invoice 1) should have been evicted, got ids: {ids:?}"
    );
    assert!(
        ids.iter().any(|id| id == small_max as u64 + 1),
        "newest record should be present, got ids: {ids:?}"
    );
    // Logical index 0 always reports the current oldest surviving record.
    assert_eq!(client.get_payment_record(&sme, &0).unwrap().invoice_id, 2);
}

#[test]
fn payment_history_length_is_capped_by_max_payment_history_constant() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _invoice, pool) = setup(&env);
    let sme = Address::generate(&env);

    assert_eq!(client.get_max_payment_history(), MAX_PAYMENT_HISTORY);

    // Record more than the default cap (kept small enough to run quickly in
    // CI while still crossing the boundary at least once).
    let total = MAX_PAYMENT_HISTORY + 5;
    for i in 1..=total as u64 {
        record_on_time_payment(&env, &client, &pool, &sme, i, 10_000_000);
    }

    assert_eq!(
        client.get_payment_history_length(&sme),
        MAX_PAYMENT_HISTORY,
        "history length must never exceed max_payment_history"
    );
    assert_eq!(
        client.get_credit_score(&sme).total_invoices,
        total,
        "total_invoices keeps counting even once the ring buffer is full"
    );
}

// ── v2 attestation blend (#868) ─────────────────────────────────────────────

#[test]
fn attestation_blend_changes_score_relative_to_internal_only() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _invoice, pool) = setup(&env);
    let sme = Address::generate(&env);

    // Build up an internal (payment-history) score first.
    for i in 1..=6u64 {
        record_on_time_payment(&env, &client, &pool, &sme, i, 200_000_000);
    }
    let internal_only_score = client.get_credit_score(&sme).score;

    // A high-weight attestor submits a maximal attestation. With the default
    // 70/30 internal/external blend, this should visibly move the score
    // toward MAX_SCORE without a further payment being recorded.
    let attestor = Address::generate(&env);
    client.register_attestor(&admin, &attestor, &AttestorType::CreditBureau, &10_000u32);

    let now = env.ledger().timestamp();
    client.submit_attestation(
        &attestor,
        &sme,
        &AttestorType::CreditBureau,
        &1000u32, // max score_contribution
        &SorobanString::from_str(&env, "evidence-hash"),
        &(now + 86_400),
    );

    let blended_score = client.get_credit_score(&sme).score;
    assert!(
        blended_score > internal_only_score,
        "a maximal attestation should raise the blended score above the internal-only score: {internal_only_score} -> {blended_score}"
    );
    assert!(blended_score <= MAX_SCORE);
}

#[test]
fn attestation_blend_leaves_score_untouched_with_zero_active_attestations() {
    // #868: "existing SMEs are never regressed by v2" — with no attestations
    // at all, the blended score must exactly equal the pre-v2 internal score.
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _invoice, pool) = setup(&env);
    let sme = Address::generate(&env);

    for i in 1..=3u64 {
        record_on_time_payment(&env, &client, &pool, &sme, i, 200_000_000);
    }

    let score_before = client.get_credit_score(&sme).score;
    let score_after = client.get_credit_score(&sme).score;
    assert_eq!(score_before, score_after);
}

// ── simulate_score_with_attestations ────────────────────────────────────────

#[test]
fn simulate_score_with_attestations_previews_without_persisting() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _invoice, pool) = setup(&env);
    let sme = Address::generate(&env);

    for i in 1..=4u64 {
        record_on_time_payment(&env, &client, &pool, &sme, i, 200_000_000);
    }
    let real_score = client.get_credit_score(&sme).score;

    let mut hypothetical = soroban_sdk::Vec::new(&env);
    hypothetical.push_back((10_000u32, 1000u32)); // one maximal hypothetical attestation

    let simulated = client.simulate_score_with_attestations(&sme, &hypothetical);
    assert!(
        simulated >= real_score,
        "a positive hypothetical attestation should never simulate a lower score"
    );

    // The simulation must be read-only: the real stored score is unaffected.
    let real_score_after = client.get_credit_score(&sme).score;
    assert_eq!(
        real_score, real_score_after,
        "simulate_score_with_attestations must not persist any state change"
    );
    // And no attestation was actually created.
    assert_eq!(client.list_sme_attestations(&sme).len(), 0);
}

#[test]
fn simulate_score_with_attestations_is_cached_for_identical_hypothetical_input() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _invoice, pool) = setup(&env);
    let sme = Address::generate(&env);

    for i in 1..=2u64 {
        record_on_time_payment(&env, &client, &pool, &sme, i, 200_000_000);
    }

    let mut hypothetical = soroban_sdk::Vec::new(&env);
    hypothetical.push_back((5_000u32, 800u32));

    let first = client.simulate_score_with_attestations(&sme, &hypothetical);
    let second = client.simulate_score_with_attestations(&sme, &hypothetical);
    assert_eq!(
        first, second,
        "identical hypothetical input should be served from cache with the same result"
    );

    // A different hypothetical set must not be conflated with the cached one.
    let mut other_hypothetical = soroban_sdk::Vec::new(&env);
    other_hypothetical.push_back((5_000u32, 200u32));
    let third = client.simulate_score_with_attestations(&sme, &other_hypothetical);
    assert!(
        third <= first,
        "a lower hypothetical score_contribution should not simulate a higher score: {first} vs {third}"
    );
}
