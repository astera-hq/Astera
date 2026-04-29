# Astera Security Checklist

Pre-mainnet checklist mapping security properties to contract functions.
Check each item before deploying to mainnet.

## Invoice Contract (`contracts/invoice/src/lib.rs`)

- [x] `initialize` — can only be called once (`Initialized` flag)
- [x] `create_invoice` — requires owner auth; validates amount > 0; validates due_date in future; rate-limited 10/day/address; reads pool from `DataKey::Pool` (not admin)
- [x] `verify_invoice` — only stored oracle address accepted
- [x] `resolve_dispute` — only stored admin address accepted
- [x] `mark_funded` — only stored pool address accepted; status must be Pending or Verified
- [x] `mark_paid` — only owner, pool, or admin; invoice must be Funded
- [x] `mark_defaulted` — only stored pool address; invoice must be Funded
- [x] `cleanup_invoice` — only admin; invoice must be Paid or Defaulted
- [x] `propose_upgrade` — admin only; 24h timelock set
- [x] `execute_upgrade` — admin only; timelock must have expired
- [x] Circuit breaker (`pause`/`unpause`) — admin only; all state-changing functions guarded

## Pool Contract (`contracts/pool/src/lib.rs`)

- [x] `initialize` — one-shot; sets config, accepted tokens, share token mapping
- [x] `deposit` — investor auth; amount > 0; token must be whitelisted; mints shares proportionally
- [x] `withdraw` — investor auth; shares > 0; checks share balance; checks available liquidity before transfer
- [x] `fund_invoice` — admin only; principal > 0; no duplicate funding; liquidity sufficiency check; factoring fee locked at funding time
- [x] `repay_invoice` — payer auth; idempotency guard (`repaid` flag); tracks `total_fee_revenue` and `total_paid_out`
- [x] `add_token` — admin only; no duplicate tokens
- [x] `remove_token` — admin only; requires zero pool balance
- [x] `set_yield` — admin only; capped at 50%
- [x] `set_factoring_fee` — admin only; capped at 100%
- [x] `propose_upgrade` / `execute_upgrade` — admin + 24h timelock
- [x] Circuit breaker — admin only

## Credit Score Contract (`contracts/credit_score/src/lib.rs`)

- [x] `initialize` — one-shot
- [x] `record_payment` — pool auth required if caller ≠ pool; idempotency guard per invoice_id; status derived from timestamps
- [x] `record_default` — same guards as `record_payment`
- [x] Score bounded to [200, 850] — enforced in `calculate_score`
- [x] `propose_upgrade` / `execute_upgrade` — admin + 24h timelock
- [x] Circuit breaker — admin only; read-only views succeed while paused

## Build / Deployment

- [ ] `cargo audit` passes with no unresolved vulnerabilities
- [ ] `cargo clippy -- -D warnings` passes
- [ ] All 3 contracts compile to WASM (`--target wasm32-unknown-unknown --release`)
- [ ] All unit tests pass (`cargo test`)
- [ ] Admin keys stored in hardware wallet or multisig
- [ ] Testnet deployment verified before mainnet
- [ ] `docs/mainnet-checklist.md` completed
- [ ] `docs/security-audit/audit-tracking.md` completed
- [ ] No `Critical`/`High` findings remain open in the audit tracker
