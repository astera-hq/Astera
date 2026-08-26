## Summary

Fixes four governance issues: #1118, #1119, #1121, #1122.

- **#1118 — `cancel_proposal` let a single proposer veto a `Passed` proposal.**
  `cancel_proposal` now only lets the original proposer cancel while a
  proposal is still `Active`. Once a proposal has passed quorum, only the
  admin may cancel it — a single voter can no longer unilaterally block an
  approved change during the timelock window. Finalization is also applied
  lazily before the authorization check, so a proposer can't dodge the new
  rule by cancelling before anyone else has touched a proposal whose voting
  period already ended (which would otherwise still read `Active` in
  storage).

- **#1119 — `execute_proposal` never invoked the target contract.**
  `execute_governance_action`'s dispatch match had six action variants
  (`SetPoolFeeTier`, `SetPoolLoyaltyTiers`, `SetPoolFallbackPrice`,
  `SetPoolRateBounds`, `SetPoolExchangeRate`, `SetPoolCollateralConfig`)
  shadowed by earlier `TODO: Implement` / `return Ok(())` arms for the same
  patterns, so those actions silently no-opped instead of calling the target
  contract. The crate also didn't compile: `GovernanceAction` had grown to
  54 variants against soroban's 50-case cap on a single `#[contracttype]`
  union, two variant names exceeded the 32-character symbol limit, three
  `PoolAction`-style fields had drifted from the real target signatures
  (`u32`/`i128` mismatches, an `i128` standing in for `CollateralConfig`),
  and every cross-contract call in the dispatcher passed arguments by value
  where the generated `#[contractclient]` methods expect references.
  Fixed by: splitting `GovernanceAction` into `PoolAction` / `InvoiceAction`
  / `OracleRegistryAction` / `ComplianceAction` sub-enums (each under the
  50-case cap) wrapped by a 4-variant `GovernanceAction`; removing the dead
  TODO arms so the real implementations execute; correcting the drifted
  field types; and passing every action field by reference to match the
  generated client signatures. `execute_proposal` now performs a genuine
  cross-contract call for every action variant instead of only emitting an
  event for an off-chain relayer.

- **#1121 — no way to update `min_share_balance` after `initialize`.**
  Added `update_min_share_balance` (admin-gated) and
  `update_min_share_balance_via_ac` (multisig-gated, matching the existing
  `update_config` / `update_config_via_ac` pattern), so the proposal-creation
  stake threshold can be raised or lowered post-deploy without redeploying
  governance. Both reject non-positive values, preserving the #931 spam
  guard.

- **#1122 — no property-based coverage for `finalize_proposal`'s bps math.**
  Added `tests/finalize_proposal_proptest.rs`, in the style of
  `share/tests/fuzz_tests.rs`, covering the quorum/pass-threshold arithmetic
  end-to-end through the public contract API (`create_proposal` → `vote` →
  `execute_proposal`) against an independent oracle of the same formula, plus
  two targeted invariants: under-quorum proposals never execute, and adding
  more YES votes never turns an `Executed` outcome into `Rejected`.

## Other changes bundled in

The governance crate did not compile on `main` (54-variant union over the
50-case cap, over-length symbols, drifted field types, missing `&` on every
cross-contract call argument — all pre-existing, unrelated to any single
issue above but blocking all of them) and its three test files were
completely stale against the current contract API (`initialize`,
`create_proposal`, `vote`, and `execute_proposal` had all changed shape).
Both are fixed here since none of the four issues could otherwise be
verified. New tests use a small in-crate mock target contract
(`tests/common/mod.rs`) instead of the real pool/invoice/oracle_registry/
compliance contracts, because those are independently broken on `main` for
unrelated reasons (missing storage keys, moved-value bugs, and — for
compliance's `*_via_governance` methods specifically — function names that
exceed Soroban's 32-character limit, meaning no valid contract can currently
implement them; this is a pre-existing defect worth a follow-up but is out
of scope here).

`Cargo.lock` changes collapse a duplicate `ed25519-dalek` resolution (2.2.0
and 3.0.0 both present) down to a single version — the 3.0.0 edge doesn't
compile against the workspace's `rand`/`rand_chacha` versions, which
previously made `cargo test` fail to even build for any crate pulling in
`soroban-sdk`'s `testutils` (confirmed this also blocked `share`'s existing
test suite, unrelated to governance).

## Test plan

- [x] `cargo build -p governance`
- [x] `cargo test -p governance` — 46 tests pass across `lib`,
      `access_control_tests`, `governance_flow_tests`, and
      `finalize_proposal_proptest`
- [x] `cargo clippy -p governance --all-targets` — no errors, only
      pre-existing/cosmetic warnings
