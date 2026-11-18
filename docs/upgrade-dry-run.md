# Upgrade Dry-Run Guide

`scripts/upgrade-dry-run.sh` simulates a contract upgrade **before** you apply it
to testnet or mainnet, so you can catch breaking changes and storage migrations
ahead of time. It is referenced by the
[Contract Upgrade Guide](contract-upgrade-guide.md).

## What it does

For each target contract the tool:

1. **Snapshots** the current contract state.
2. **Deploys the new wasm** in a throwaway/standalone test environment.
3. **Runs smoke tests** against the new binary.
4. **Compares** pre/post behavior and storage layout.
5. **Reports** breaking changes that would require a migration or rollback plan.

## Usage

```bash
./scripts/upgrade-dry-run.sh [contract] [options]
```

- `contract`: `invoice` | `pool` | `credit` | `all` (default: `all`)
- `--wasm <path>`: path to the new wasm (optional; builds if omitted)
- `--snapshot`: restore from snapshot after the test (useful on testnet)
- `--network <n>`: `standalone` (default) | `testnet` | `mainnet`
- `--verbose`: detailed logging

## Exit codes

| Code | Meaning                                            |
| ---- | -------------------------------------------------- |
| `0`  | Upgrade safe — no breaking changes detected        |
| `1`  | Build or setup error                               |
| `2`  | Upgrade has breaking changes (needs migration/plan)|

## Example: dry-run a pool change on standalone

```bash
./scripts/upgrade-dry-run.sh pool --network standalone --verbose
```

## Example: dry-run everything with a prebuilt wasm on testnet

```bash
./scripts/upgrade-dry-run.sh all --wasm target/wasm32-unknown-unknown/release/pool.wasm --network testnet --snapshot
```

## Interpreting results

- **Exit 0:** safe to proceed with the normal upgrade flow.
- **Exit 2:** read the printed diff. If it's a storage-layout change, write a
  migration (see [Contract Upgrade Guide](contract-upgrade-guide.md)) and
  re-run the dry-run until it passes.

> Running the dry-run against `standalone` is the cheapest and safest first pass.
> Promote to `testnet` (with `--snapshot` so you don't pollute state) only after
> standalone is clean.
