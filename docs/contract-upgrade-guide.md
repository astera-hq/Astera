# Contract Upgrade Guide

Astera contracts are upgradeable. This guide explains the safe, repeatable
upgrade workflow: simulate the upgrade first (see
[Upgrade Dry-Run Guide](upgrade-dry-run.md)), then apply it on the target
network.

## When to use this guide

Use the upgrade path whenever you ship a new wasm for an already-deployed
contract — a bug fix, a new feature, or a storage-layout change. **Never** deploy
a fresh contract instance over an existing one expecting state to carry over;
the upgrade mechanism preserves instance storage, while a redeploy starts blank.

## Prerequisites

- The currently deployed contract's ID (from `deployed-*.env`).
- A newly built wasm for the contract you want to upgrade.
- The Stellar CLI and a funded upgrade authority (typically the `access_control`
  SuperAdmin or the contract admin, depending on the contract's upgrade gate).

## Step 1 — Dry-run (required for mainnet)

Always run the dry-run against a standalone or testnet copy first:

```bash
bash scripts/upgrade-dry-run.sh <contract> --network standalone
```

Replace `<contract>` with `invoice`, `pool`, `credit`, or `all`. The tool
snapshots current state, deploys the new wasm in a throwaway environment, runs
smoke tests, and reports any breaking changes that would require a migration.
Resolve all reported breakages before proceeding.

## Step 2 — Apply the upgrade

Use the Stellar CLI to write the new wasm hash and bump the contract:

```bash
# 1. Upload the new wasm and get its hash
WASM_HASH=$(stellar contract upload \
  --wasm target/wasm32-unknown-unknown/release/<contract>.wasm \
  --source <upgrader> --network <network> | tail -1)

# 2. Apply the upgrade to the existing contract instance
stellar contract update \
  --id <CONTRACT_ID> \
  --wasm-hash "$WASM_HASH" \
  --source <upgrader> --network <network>
```

For contracts gated by `access_control`, the upgrade must be authorized through
the `access_control` multisig proposal flow rather than a single admin key.

## Step 3 — Verify

```bash
source deployed-<network>.env
bash scripts/smoke-test-testnet.sh   # point STELLAR_RPC_URL at the target network
```

Confirm `version()` reflects the new build and that no storage/behavior
regressions appeared.

## Migration notes

If the dry-run reports a storage-layout change (renamed/removed `DataKey`s or a
changed struct), write a one-off migration that reads the old layout and writes
the new one before the new wasm takes over. Coordinate the migration with the
upgrade in the same proposal/transaction where possible.
