# Governance Parameter Inventory and Classification

## Overview
This document inventories all admin-settable parameters across pool, invoice, oracle_registry, and compliance contracts, and classifies each as either:
- **Safe (fast admin-only)**: Truly operational parameters that can be changed instantly without governance
- **Governance-gated**: Parameters that require a governance proposal + timelock for changes

---

## Pool Contract Parameters

### Governance-Gated Parameters
These parameters affect protocol economics, risk management, or core configuration and should require governance:

1. **set_yield** - Changes the base APY (affects protocol economics)
2. **set_yield_change_policy** - Changes yield cooldown periods (affects protocol economics)
3. **set_factoring_fee** - Changes factoring fee percentage (affects protocol economics)
4. **set_fee_tier** - Changes fee tier structure (affects protocol economics)
5. **set_treasury** - Changes treasury address (fund movement risk)
6. **set_max_utilization** - Changes max utilization cap (risk parameter)
7. **set_min_deposit** - Changes minimum deposit amount (access control)
8. **set_max_investor_concentration** - Changes concentration limits (risk parameter)
9. **set_loyalty_tiers** - Changes loyalty bonus structure (protocol economics)
10. **set_withdrawal_limits** - Changes withdrawal rate limits (risk parameter)
11. **set_max_withdrawal_queue_age** - Changes withdrawal queue age limit (risk parameter)
12. **set_max_withdrawal_queue_depth** - Changes withdrawal queue depth (risk parameter)
13. **set_oracle_contract** - Changes oracle address (critical infrastructure)
14. **set_oracle_stale_threshold** - Changes oracle staleness tolerance (risk parameter)
15. **set_fallback_price** - Changes fallback price (risk parameter)
16. **set_rate_bounds** - Changes exchange rate bounds (risk parameter)
17. **set_exchange_rate** - Changes exchange rate (risk parameter)
18. **set_compliance_registry** - Changes compliance registry (critical infrastructure)
19. **set_require_compliance_check** - Toggles compliance requirement (access control)
20. **set_referral_registry** - Changes referral registry (protocol economics)
21. **set_kyc_required** - Toggles KYC requirement (access control)
22. **set_credit_score_contract** - Changes credit score contract (critical infrastructure)
23. **set_insurance_contract** - Changes insurance contract (critical infrastructure)
24. **set_compound_interest** - Toggles compound interest (protocol economics)
25. **set_secondary_market_contract** - Changes secondary market contract (critical infrastructure)
26. **set_risk_contract** - Changes risk/liquidation contract (critical infrastructure)
27. **set_collateral_config** - Changes collateral configuration (risk parameter)
28. **set_upgrade_timelock** - Changes upgrade timelock duration (security parameter)
29. **set_operation_delay** - Changes operation delay (security parameter)

### Safe (Fast Admin-Only) Parameters
These are operational parameters that can be changed instantly for emergency response:

1. **pause/unpause** - Emergency pause (MUST remain fast for "stop the bleeding")
2. **set_investor_kyc** - Per-investor KYC approval (operational)
3. **set_paused_via_ac** - Emergency pause via access control (operational)

---

## Invoice Contract Parameters

### Governance-Gated Parameters

1. **set_grace_period** - Changes global grace period (affects all invoices, risk parameter)
2. **set_min_due_date_window** - Changes minimum due date window (risk parameter)
3. **set_max_invoice_amount** - Changes maximum invoice amount (risk parameter)
4. **set_max_sme_outstanding** - Changes SME exposure limit (risk parameter)
5. **set_expiration_duration** - Changes invoice expiration duration (risk parameter)
6. **set_completed_invoice_ttl** - Changes completed invoice TTL (data retention)
7. **set_daily_invoice_limit** - Changes daily invoice creation limit (risk parameter)
8. **set_dispute_window** - Changes dispute resolution window (risk parameter)
9. **set_oracle** - Changes primary oracle address (critical infrastructure)
10. **set_secondary_oracle** - Changes secondary oracle address (critical infrastructure)
11. **set_oracle_registry** - Changes oracle registry address (critical infrastructure)
12. **set_consensus_required** - Toggles consensus verification (security parameter)
13. **set_compliance_registry** - Changes compliance registry (critical infrastructure)
14. **set_require_compliance_check** - Toggles compliance requirement (access control)
15. **set_require_registered_debtor** - Toggles registered debtor requirement (access control)
16. **set_oracle_verified_funding_only** - Toggles oracle verification requirement (security parameter)
17. **set_arbitration_contract** - Changes arbitration contract (critical infrastructure)
18. **set_dispute_value_threshold** - Changes dispute value threshold (risk parameter)
19. **set_metadata_image_uri** - Changes default metadata image (operational but affects UX)

### Safe (Fast Admin-Only) Parameters

1. **pause/unpause** - Emergency pause (MUST remain fast)
2. **set_invoice_grace_period** - Per-invoice grace period override (operational)
3. **set_paused_via_ac** - Emergency pause via access control (operational)
4. **set_oracle_via_ac** - Oracle change via access control (operational)
5. **set_access_control** - Bootstrap access control (one-time setup)
6. **set_access_control_via_ac** - Rotate access control (operational)
7. **set_invoice_private** - Per-invoice privacy setting (owner-controlled, not admin)

---

## Oracle Registry Parameters

### Governance-Gated Parameters

1. **set_invoice_contract** - Changes invoice contract address (critical infrastructure)
2. **set_treasury** - Changes treasury address (fund movement risk)
3. **set_registry_config** - Changes registry configuration (min_stake, required_votes, quorum_bps, round_duration_secs, deregister_cooldown_secs) - (risk parameters)
4. **set_quorum_tiers** - Changes value-based quorum schedule (risk parameter)

### Safe (Fast Admin-Only) Parameters

1. **pause/unpause** - Emergency pause (MUST remain fast)
2. **slash_oracle** - Oracle slashing (operational/enforcement)
3. **admin_resolve_round** - Admin fallback for expired rounds (operational)
4. **set_paused_via_ac** - Emergency pause via access control (operational)
5. **set_access_control** - Bootstrap access control (one-time setup)
6. **set_access_control_via_ac** - Rotate access control (operational)
7. **set_invoice_contract_via_ac** - Invoice contract change via access control (operational)
8. **set_treasury_via_ac** - Treasury change via access control (operational)
9. **set_registry_config_via_ac** - Registry config change via access control (operational)

---

## Compliance Contract Parameters

### Governance-Gated Parameters

1. **set_rescreening_interval** - Changes rescreening interval (risk parameter)
2. **set_screener_timelock** - Changes screener registration timelock (security parameter)

### Safe (Fast Admin-Only) Parameters

1. **pause/unpause** - Emergency pause (MUST remain fast)
2. **register_screener** - Register new screener (operational)
3. **confirm_screener_registration** - Confirm screener after timelock (operational)
4. **deregister_screener** - Remove screener (operational)
5. **set_paused_via_ac** - Emergency pause via access control (operational)
6. **set_access_control** - Bootstrap access control (one-time setup)
7. **set_access_control_via_ac** - Rotate access control (operational)
8. **set_rescreening_interval_via_ac** - Rescreening interval via access control (operational)
9. **set_screener_timelock_via_ac** - Screener timelock via access control (operational)

---

## Emergency Pause Path Design

### Requirements
- Must remain fast (no full governance latency for "stop the bleeding" scenarios)
- Should have its own short timelock OR immediate effect with mandatory public justification
- Must be independently testable

### Proposed Design

**Option A: Immediate pause with mandatory justification event**
- `pause()` and `unpause()` remain admin-only (fast path)
- Add mandatory `reason` parameter to pause events
- Add `EmergencyPause` event with: `caller`, `timestamp`, `reason`
- Consider adding a short cooldown (e.g., 1 hour) before unpause to prevent pause/unpause abuse

**Option B: Short timelock with fast-track governance**
- Keep current pause/unpause as admin-only for emergencies
- Add a separate `propose_emergency_pause()` that requires only a lower quorum threshold
- Emergency proposals execute after a short timelock (e.g., 1 hour instead of 48 hours)
- Requires justification in the proposal description

**Recommended: Option A**
- Simpler to implement
- Faster response time for genuine emergencies
- Event-based transparency provides accountability
- Can be combined with access_control multisig for distributed emergency response

---

## Implementation Plan

### Phase 1: Governance Contract Updates
1. Add new `ActionPayload` variants for governance-gated parameter changes
2. Update `execute_proposal` to handle cross-contract parameter change calls
3. Add governance-specific entrypoints to each contract that verify proposal execution

### Phase 2: Contract Updates
1. Add `*_via_governance` entrypoints for all governance-gated parameters
2. These entrypoints verify the caller is the governance contract and that a proposal was executed
3. Keep existing admin setters for backward compatibility but mark them as deprecated
4. Add deprecation warnings or errors if called directly (optional)

### Phase 3: Emergency Pause Path
1. Ensure pause/unpause remains fast (admin-only or access_control multisig)
2. Add mandatory justification events for pause actions
3. Add integration tests for emergency pause scenarios

### Phase 4: SDK and Frontend Updates
1. Update SDK to route parameter changes through governance proposals
2. Update admin frontend to show governance proposal UI for parameter changes
3. Keep emergency pause as direct action in UI

### Phase 5: Test Updates
1. Update existing tests to use governance flow for parameter changes
2. Add test-only fast paths clearly marked as such
3. Add integration tests for full proposal → vote → timelock → execute cycle
