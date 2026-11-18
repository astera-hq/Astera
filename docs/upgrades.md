# Contract Upgrades & State Migrations

This document describes how the Astera Soroban contracts (`invoice`, `pool`,
`credit_score`) are upgraded in place and how their persistent storage layout is
migrated after an upgrade. For the operational deploy runbook see
[`contract-upgrade-guide.md`](./contract-upgrade-guide.md); this document focuses
on the on-chain mechanism and the version/migration hooks added in #397.

The newer `governance` and `insurance` contracts do **not** currently
implement this flow at all — see [Per-contract status](#per-contract-status)
below.

## Overview

Each contract supports an in-place WASM upgrade behind a 24-hour timelock, plus a
manually-invoked migration hook that transforms persistent storage when the
schema changes between versions. Two independent version numbers are tracked:

| Function | Returns | Meaning |
| --- | --- | --- |
| `version()` | semantic version struct | The compiled package version (`CARGO_PKG_VERSION`), recorded at `initialize()`. |
| `migration_version()` | `u32` | The applied storage-schema migration level. Starts at `0`, advanced by `run_migration`. |

All upgrade and migration entry points are **admin-only** — the caller must be the
admin recorded at `initialize()` and must authorize the call.

## Upgrade flow (timelocked)

The WASM hash is proposed first and can only be executed once the timelock has
elapsed, giving stakeholders a window to react to a malicious or mistaken
proposal.

```text
admin ──propose_upgrade(wasm_hash)──▶ ProposedWasmHash + UpgradeScheduledAt stored
                                       (emits "upg_prop" with the unlock timestamp)
        ... 24h timelock (UPGRADE_TIMELOCK_SECS) ...
admin ──execute_upgrade()───────────▶ env.deployer().update_current_contract_wasm(hash)
                                       (emits "upgraded")
```

- `propose_upgrade(admin, wasm_hash)` — records the candidate WASM hash and the
  current ledger timestamp.
- `execute_upgrade(admin)` — reverts unless `now >= scheduled_at + 24h`, then
  swaps the contract's WASM via `env.deployer().update_current_contract_wasm`.

Re-proposing simply overwrites the pending hash and resets the timelock.

## Migration flow

After `execute_upgrade` installs new code, persistent storage written by the old
code may need transforming. `run_migration` performs that transformation:

```rust
pub fn run_migration(env: Env, admin: Address) /* -> Result<(), PoolError> in pool */ {
    admin.require_auth();
    // admin check
    let current = migration_version(); // defaults to 0
    if current >= CURRENT_MIGRATION_VERSION {
        return; // idempotent no-op once fully migrated
    }
    // migration arms (current -> current + 1) transform storage here
    set(MigrationVersion, CURRENT_MIGRATION_VERSION);
}
```

Properties:

- **Manual** — run once by the admin immediately after `execute_upgrade`. It is
  not triggered automatically by the upgrade.
- **Idempotent** — once `migration_version() == CURRENT_MIGRATION_VERSION`,
  further calls are a no-op, so it is safe to retry.
- **Forward-only** — each release bumps `CURRENT_MIGRATION_VERSION` by one and
  adds a migration arm transforming the previous layout to the new one.

### Adding a migration

When a release changes the persistent storage layout:

1. Bump `CURRENT_MIGRATION_VERSION` by one in the contract.
2. Add the transformation for the step `current -> current + 1` in
   `run_migration` (read old entries, write the new shape, remove stale keys).
3. Add a test that seeds old-shape state, runs the migration, and asserts the new
   shape and that unrelated state is preserved.

## Upgrade + migration runbook

```bash
# 1. Build and upload the new WASM, capture its hash
stellar contract install --wasm target/wasm32-unknown-unknown/release/<contract>.wasm

# 2. Propose the upgrade (starts the 24h timelock)
stellar contract invoke --id <CONTRACT_ID> --source admin -- \
  propose_upgrade --admin <ADMIN> --wasm_hash <WASM_HASH>

# 3. After 24h, execute the upgrade
stellar contract invoke --id <CONTRACT_ID> --source admin -- execute_upgrade --admin <ADMIN>

# 4. Run the storage migration
stellar contract invoke --id <CONTRACT_ID> --source admin -- run_migration --admin <ADMIN>

# 5. Verify
stellar contract invoke --id <CONTRACT_ID> -- version
stellar contract invoke --id <CONTRACT_ID> -- migration_version
```

## Per-contract status

| Contract | `version()` | `propose_upgrade` / `execute_upgrade` | `run_migration` / `migration_version` |
| --- | --- | --- | --- |
| `invoice` | ✅ | ✅ | ✅ |
| `pool` | ✅ | ✅ | ✅ |
| `credit_score` | ✅ | ✅ | ✅ |
| `governance` | ❌ | ❌ | ❌ |
| `insurance` | ❌ | ❌ | ❌ |

`CURRENT_MIGRATION_VERSION` is currently `1` in each contract that has one;
freshly initialized contracts start at `migration_version() == 0` until
`run_migration` is invoked.

### `governance` and `insurance` — no upgrade mechanism yet (#988)

Neither contract implements any part of the flow above — there is no
`propose_upgrade`/`execute_upgrade`/`run_migration`/`version`/
`migration_version` in `contracts/governance/src/lib.rs` or
`contracts/insurance/src/lib.rs` (confirmed by grepping both files; none of
those identifiers appear at all). A deployed `governance` or `insurance`
contract today has **no in-place upgrade path** — replacing its logic
requires deploying a new contract instance and migrating state manually.

Adding the timelocked upgrade + migration mechanism to these two contracts —
following the same `propose_upgrade`/`execute_upgrade`/`run_migration`
pattern as `invoice`/`pool`/`credit_score` — is real contract work (new
`DataKey` variants, new admin-gated entry points, tests), not a documentation
change, and is out of scope here. This section exists so the gap is explicit
rather than silently assumed away.
