## Summary

Fixes wallet session persistence and three Soroban contract error-reporting issues.

### Freighter wallet persistence

- Persist the connected state and wallet address in `localStorage`.
- Silently reconnect returning users when Freighter still grants permission.
- Clear persisted state when permission is revoked or the user disconnects.
- Detect a Freighter account switch and require reconnection.
- Add unit coverage for persisted reconnect behavior.

### Access control signer removal

- Add `ThresholdMustBeLoweredBeforeSignerRemoval` for removals that would leave
  the signer set below its current threshold.
- Update the lifecycle test to verify the actionable error.

### Tranche accessors

- Return `Option<InvestorPosition>` from `get_position` so unknown investors are
  distinct from zeroed positions.
- Return `NotInitialized` from `get_admin` before initialization.
- Propagate the typed admin error through `set_tranche_config` and
  `open_tranche_for_token`.
- Add regression tests for both behaviors.

## Test plan

- [x] `cargo test -p access_control --test lifecycle_tests test_remove_signer_rejects_dropping_below_threshold`
- [x] `cargo test -p tranche --test accessor_tests`
- [x] Rust compilation and source diagnostics pass for changed contracts.
- [ ] Frontend Jest and TypeScript checks: not run because frontend dependencies
      are not installed in the workspace (`jest` and `tsc` unavailable).
