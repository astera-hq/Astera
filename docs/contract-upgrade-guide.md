# Contract Upgrade Guide

This guide explains how to safely upgrade Astera Soroban contracts using the pool timelock flow.

## 1) Timelock Mechanism Overview

The pool contract uses a two-step upgrade process:

1. `propose_upgrade(admin, wasm_hash)` stores the target Wasm hash and starts a 24-hour timelock.
2. `execute_upgrade(admin)` can be called only after the timelock expires.

Only the configured admin can call these functions. The delay gives operators and users time to review and react before a new binary becomes active.

## 2) Pre-Upgrade Checklist

- Verify the new Wasm hash from the exact artifact reviewed in CI.
- Run the full contract test suite plus at least one upgrade rehearsal on testnet.
- Announce the maintenance window to users at least 48 hours in advance.
- Consider pausing the pool before execution to reduce in-flight state changes.
- Confirm there are no operational blockers (critical incidents, unfinished repayment workflows, or unreviewed migrations).

## 3) Step-by-Step Upgrade Procedure

```bash
# 1. Build new Wasm
cargo build --target wasm32-unknown-unknown --release

# 2. Install new Wasm and capture hash
stellar contract install \
  --wasm target/wasm32-unknown-unknown/release/pool.wasm \
  --network testnet \
  --source admin

# 3. Propose upgrade (starts 24h timelock)
stellar contract invoke \
  --id $POOL_CONTRACT_ID \
  --network testnet \
  --source admin \
  -- propose_upgrade \
  --admin $ADMIN_ADDRESS \
  --wasm_hash <WASM_HASH>

# 4. After 24 hours, execute
stellar contract invoke \
  --id $POOL_CONTRACT_ID \
  --network testnet \
  --source admin \
  -- execute_upgrade \
  --admin $ADMIN_ADDRESS
```

## 4) State Migration

When storage schema changes:

- Keep old fields readable in the new binary.
- Add migration logic that lazily upgrades records on access, or provide explicit admin migration entry points.
- Validate migration on testnet snapshots before mainnet.
- Never remove support for legacy keys until migration completion is confirmed.

## 5) Post-Upgrade Verification

- Run smoke checks (`get_config`, `accepted_tokens`, `get_token_totals`) immediately.
- Execute one low-value deposit/withdraw scenario on testnet or staging first, then production if approved.
- Unpause if paused for maintenance.
- Monitor logs and user-facing errors for at least 24 hours.

## 6) Rollback Considerations

Soroban upgrades are not a direct "undo". If rollback is needed:

- Build and deploy a corrective Wasm version.
- Repeat the same timelock process (`propose_upgrade` -> wait -> `execute_upgrade`).
- Communicate status updates clearly during the full rollback window.

