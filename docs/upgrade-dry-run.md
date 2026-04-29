# Upgrade Dry-Run Guide

This guide explains how to use the **upgrade dry-run simulation tool** to safely validate Soroban contract upgrades before deploying them to mainnet.

## Why Use Upgrade Dry-Run?

Contract upgrades in Soroban are **irreversible** once executed. Mistakes can freeze investor deposits or invalidate existing invoices. The dry-run tool:

- ✅ **Tests new WASM against live state** (via snapshot)
- ✅ **Detects API breaking changes** before deployment
- ✅ **Validates storage schema compatibility** (no lost data)
- ✅ **Runs full integration test suite** with new binary
- ✅ **Generates audit trail** for compliance teams
- ✅ **Provides rollback confidence** with detailed diff reports

## Quick Start

### 1. Build and test locally

Test the new WASM without any network calls:

```bash
./scripts/upgrade-dry-run.sh pool
```

This will:

- Build the latest WASM (if not already built)
- Run all integration tests
- Generate a report with no breaking changes

### 2. Run reports

View detailed upgrade report:

```bash
cat .upgrade-dry-run/upgrade-report-standalone-*.md | less
```

**Key sections to review:**

- ✅ **API Changes**: Are all entry points backward compatible?
- ✅ **Storage Schema**: Are persistent storage keys unchanged?
- ✅ **Test Results**: Do all integration tests pass?

## Usage

```bash
./scripts/upgrade-dry-run.sh <contract> [options]
```

### Arguments

| Arg        | Required | Options                                                   | Default |
| ---------- | -------- | --------------------------------------------------------- | ------- |
| `contract` | No       | `invoice`, `pool`, `credit`, `share`, `governance`, `all` | `all`   |

### Options

| Flag            | Value                              | Purpose                                             |
| --------------- | ---------------------------------- | --------------------------------------------------- |
| `--wasm <path>` | File path                          | Test a pre-built WASM instead of rebuilding         |
| `--network`     | `standalone`, `testnet`, `mainnet` | Which network to target (affects report)            |
| `--snapshot`    | (flag)                             | For testnet: restore state from snapshot after test |
| `--verbose`     | (flag)                             | Print full state snapshots and test logs            |

### Examples

**Test all contracts locally:**

```bash
./scripts/upgrade-dry-run.sh all
```

**Test pool contract with specific WASM:**

```bash
./scripts/upgrade-dry-run.sh pool --wasm ./my-pool-build.wasm --verbose
```

**Test for testnet deployment:**

```bash
./scripts/upgrade-dry-run.sh pool --network testnet
```

**Test a feature branch before opening PR:**

```bash
git stash           # save local changes
git checkout feat/my-upgrade
./scripts/upgrade-dry-run.sh all
git checkout -     # back to original branch
```

## Report Structure

The tool generates reports in `.upgrade-dry-run/` directory:

```
.upgrade-dry-run/
├── upgrade-report-standalone-20250429_1430.md    # Main upgrade report
├── test-output-20250429_1430.log                 # Full test output
└── snapshots/
    ├── state-pre-20250429_1430.json              # Pre-upgrade state
    └── state-post-20250429_1430.json             # Post-upgrade state
```

### Report Sections

1. **Summary**: Exit status, tests passed, breaking changes detected
2. **State Comparison**: What changed in contract storage and config
3. **API Compatibility**: Entry points with same/changed signatures
4. **Storage Migration**: Indexed keys, new fields, removed fields
5. **Recommendations**: Next steps for testnet or mainnet
6. **Test Artifacts**: Links to detailed logs and snapshots

## Breaking Changes Explained

### API Breaking Changes (❌ FAIL)

Occurs when an existing entry point changes its signature:

```rust
// BEFORE: pool.rs
pub fn deposit(e: Env, investor: Address, usdc_address: Address, amount: i128) { ... }

// AFTER: pool.rs
pub fn deposit(e: Env, investor: Address, usdc_address: Address, amount: i128, memo: String) { ... }
//                                                                                      ^^^^^^^^
//                                                                                 NEW PARAMETER
```

**Impact**: Existing frontend code will break; invoices or deposits in flight may fail.

**Detection**: Integration tests fail with "function signature mismatch" errors.

### Storage Schema Breaking Changes (❌ FAIL)

Occurs when persistent storage keys change incompatibly:

```rust
// BEFORE: pool.rs
persistent!(StorageKeys { Pool(Symbol) = true });

// AFTER: pool.rs
persistent!(StorageKeys { Pools(Symbol) = true });  // RENAMED KEY
//                       ^     ^
//                    Different field name
```

**Impact**: New binary cannot read old investor positions; deposits become inaccessible.

**Detection**:

- "Key not found" errors in post-upgrade state snapshots
- Storage analysis reports "REMOVED" keys without migration

### Safe Changes (✅ PASS)

Examples of changes the tool confirms as safe:

✅ **Internal function refactoring** — helper functions renamed/reorganized  
✅ **New validations** — stricter input checks that don't affect API  
✅ **Algorithm improvements** — yield calculation optimized but same results  
✅ **New entry points** — additional functions added; old ones unchanged  
✅ **Lazy storage migration** — old keys readable; transparent to callers

## Dry-Run Workflow for Production Upgrades

### Phase 1: Development (your machine)

```bash
# Feature branch work
git checkout -b feat/pool-migrate-v2

# Make changes and test locally (many times)
./scripts/upgrade-dry-run.sh pool --verbose
./scripts/upgrade-dry-run.sh invoice

# Once satisfied, commit and push
git push origin feat/pool-migrate-v2
```

### Phase 2: Code Review (GitHub)

```
- Reviewer runs their own dry-run in the PR branch
- ./scripts/upgrade-dry-run.sh all --network testnet
- Reviews the generated report
- Approves changes or requests follow-ups
```

### Phase 3: Staging / Testnet Deployment

```bash
# Deploy to testnet first
stellar contract install --wasm target/wasm32-unknown-unknown/release/pool.wasm \
  --network testnet --source deployer

# Get the wasm hash and extract it to a file for dry-run
# (Or use --wasm flag if you saved it separately)

./scripts/upgrade-dry-run.sh pool --network testnet

# Review report
cat .upgrade-dry-run/upgrade-report-testnet-*.md

# Run smoke tests manually against testnet deployed contracts
# (See: Contract Upgrade Guide section 5)
```

### Phase 4: Approval to Mainnet

```bash
# When ready (after testnet validation + team review):
# Follow the timelock procedure from Contract Upgrade Guide:

stellar contract invoke \
  --id $POOL_CONTRACT_ID \
  --network mainnet \
  --source admin \
  -- propose_upgrade \
  --admin $ADMIN_ADDRESS \
  --wasm_hash <hash-from-install-output>

# Wait 24 hours

stellar contract invoke \
  --id $POOL_CONTRACT_ID \
  --network mainnet \
  --source admin \
  -- execute_upgrade \
  --admin $ADMIN_ADDRESS
```

## CI/CD Integration

The upgrade dry-run is **automatically part of PR checks** via GitHub Actions:

```yaml
# .github/workflows/contracts.yml
- name: Upgrade Dry-Run (all contracts)
  run: ./scripts/upgrade-dry-run.sh all

- name: Check for Breaking Changes
  run: |
    if grep -q "UPGRADE BLOCKED" .upgrade-dry-run/upgrade-report-*.md; then
      echo "❌ Breaking changes detected"
      cat .upgrade-dry-run/upgrade-report-*.md
      exit 1
    fi
```

**PR behavior**:

- ✅ GREEN: All tests pass, no breaking changes → auto-merge eligible
- ❌ RED: Tests fail or breaking changes detected → requires fixes

## Troubleshooting

### ❌ "WASM build failed"

**Cause**: Soroban contract code has compilation errors.

**Solutions**:

1. **Check for unmerged upstream changes**:

   ```bash
   git fetch upstream
   git rebase upstream/main
   cargo build --target wasm32-unknown-unknown --release
   ```

2. **Fix the actual compilation errors**:
   - Review the error messages printed to console
   - Check `build-*.log` in `.upgrade-dry-run/` for full details
   - Fix Rust source code issues
   - Re-run the dry-run

3. **Test with a known-good WASM if needed**:
   ```bash
   ./scripts/upgrade-dry-run.sh pool --wasm ./previous-release.wasm
   ```

This helps identify if the build issue is in your changes or pre-existing.

1. **Syntax error in Rust** → Fix the compilation issue
2. **Breaking API change** → Test was expecting old function signature
3. **Storage migration missing** → New keys can't read old data

**Solution**:

```bash
./scripts/upgrade-dry-run.sh all --verbose  # See full error details
cargo test -r --test integration_tests      # Run tests directly
# Inspect error messages and fix code
```

### ❌ "WASM size exceeds limit"

**Cause**: Compiled binary is > 200 KB (Soroban size limit)

**Solutions**:

1. **Strip debug symbols**:

   ```bash
   cargo build --target wasm32-unknown-unknown --release
   wasm-opt target/wasm32-unknown-unknown/release/pool.wasm -o pool-opt.wasm -O4
   ```

2. **Remove dead code**:
   - Delete unused helper functions
   - Remove debug logging macro calls
   - Use conditional compilation to exclude features

3. **Inline critical functions** to reduce symbol table

See [Gas Optimizations Guide](./gas-optimizations.md) for more.

### ✅ "Upgrade ready" but storage snapshots are empty

**Cause**: Tool uses templates for snapshot (safe for dry-run, not complete state)

**Solution**:

- On testnet with actual contracts, extracting real state is possible
- For now, verify manually with:
  ```bash
  stellar contract inspect --wasm target/wasm32-unknown-unknown/release/pool.wasm
  ```

## Acceptance Criteria

Use this checklist before approving upgrades to mainnet:

- [ ] Dry-run executes with exit code 0 (safe to upgrade)
- [ ] Integration tests pass (all 4+ tests)
- [ ] No API breaking changes in report
- [ ] No storage schema removals in report
- [ ] WASM size <= 200 KB
- [ ] Testnet run also shows "Upgrade Ready"
- [ ] At least 2 reviewers approve on GitHub
- [ ] 48-hour notice given to users (maintenance window)
- [ ] Rollback plan documented and tested

## Next Steps

1. **Setup**: Copy/link script to your CI/CD pipeline
2. **Use**: Run `./scripts/upgrade-dry-run.sh all` in feature branches
3. **Review**: Check reports in `.upgrade-dry-run/` after each run
4. **Deploy**: Follow the [Contract Upgrade Guide](./contract-upgrade-guide.md) when ready

## Related Documentation

- [Contract Upgrade Guide](./contract-upgrade-guide.md) — Manual upgrade procedure
- [ARCHITECTURE.md](./ARCHITECTURE.md) — Storage design & migration patterns
- [Disaster Recovery](./disaster-recovery.md) — Handling failed upgrades
- [Gas Optimizations](./gas-optimizations.md) — Size constraints

---

**Questions?** Contact the Astera team or open a GitHub issue.

**Last updated**: 2025-04-28
