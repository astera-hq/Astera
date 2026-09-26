# Testnet Deployment Guide

This guide explains how to deploy the full Astera protocol to the Stellar
**testnet** — either manually from your machine, or automatically via the
tag-triggered GitHub workflow.

The protocol consists of **14 contracts**:

`access_control`, `arbitration`, `auction`, `compliance`, `credit_score`,
`governance`, `insurance`, `invoice`, `oracle_registry`, `pool`, `referral`,
`secondary_market`, `share`, and `tranche`.

Deployment happens in two phases:

1. **Deploy** — upload each contract's wasm and capture its contract ID
   (`scripts/deploy-testnet.sh`).
2. **Initialize** — call `initialize` on each contract and wire
   `access_control` into every contract that supports it
   (`scripts/init-contracts.sh`).

## Manual deployment

### Prerequisites

- Rust stable with the `wasm32-unknown-unknown` target
- The [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools/stellar-cli)
- A funded testnet account

```bash
rustup target add wasm32-unknown-unknown
stellar keys generate --global deployer --network testnet
stellar keys fund deployer --network testnet   # uses Friendbot
```

### Step 1 — Build the wasm

```bash
cargo build --locked --target wasm32-unknown-unknown --release \
  -p access_control -p arbitration -p auction -p compliance \
  -p credit_score -p governance -p insurance -p invoice \
  -p oracle_registry -p pool -p referral -p secondary_market \
  -p share -p tranche
```

### Step 2 — Deploy

```bash
export DEPLOYER_KEY=deployer
export NETWORK=testnet
# optional: export FUND_WITH_FRIENDBOT=true   # re-fund if balance is low
# optional: export RPC_URL=https://soroban-testnet.stellar.org

sh scripts/deploy-testnet.sh
```

This writes every contract ID to `deployed-testnet.env`. **Source that file
before initializing.**

### Step 3 — Initialize

```bash
source deployed-testnet.env
export DEPLOYER_KEY=deployer
export ADMIN_ADDRESS="$(stellar keys address deployer)"
export USDC_TOKEN_ID="<testnet USDC SAC or mock token address>"

sh scripts/init-contracts.sh
```

`init-contracts.sh` initializes all 14 contracts in dependency order and then
wires `access_control` into `invoice`, `pool`, `credit_score`, `governance`,
`compliance`, `oracle_registry`, and `referral`.

#### Optional tuning

| Env var                        | Default              | Meaning                                              |
| ------------------------------ | -------------------- | ---------------------------------------------------- |
| `ARBITRATION_MIN_STAKE`        | `1000000000`         | Min stake (in `USDC_TOKEN_ID`) to become an arbitrator |
| `ORACLE_MIN_STAKE`             | `1000000000`         | Min stake to register an oracle                      |
| `TRANCHE_SENIOR_SHARE_TOKEN_ID` | `SHARE_CONTRACT_ID`  | Senior tranche share token (override in production)  |
| `TRANCHE_JUNIOR_SHARE_TOKEN_ID` | `SHARE_CONTRACT_ID`  | Junior tranche share token (override in production)  |
| `TRANCHE_CONFIG`              | see script           | `TrancheConfig` struct for the tranche pool          |
| `SUPER_ADMIN_SIGNERS`         | `["$ADMIN_ADDRESS"]` | access_control SuperAdmin signer set (JSON array)   |
| `SUPER_ADMIN_THRESHOLD`       | `1`                  | access_control SuperAdmin threshold                 |

### Step 4 — Smoke test

```bash
source deployed-testnet.env
bash scripts/smoke-test-testnet.sh
```

## Automated deployment (GitHub workflow)

Pushing a tag matching `v*` runs `.github/workflows/testnet-deploy.yml`, which:

1. Builds all 14 wasm files (including `secondary_market` and `access_control`,
   which the earlier 5-package build skipped).
2. Deploys via `scripts/deploy-testnet.sh`.
3. Initializes via `scripts/init-contracts.sh`.
4. Smoke-tests via `scripts/smoke-test-testnet.sh`.
5. Uploads `deployed-testnet.env` as a workflow artifact.

The workflow needs the following **repository secrets**:

- `TESTNET_DEPLOYER_KEY` — the deployer key alias/secret.
- `TESTNET_ADMIN_ADDRESS` — the admin `ADMIN_ADDRESS`.
- `TESTNET_USDC_TOKEN_ID` — the testnet USDC token contract ID.

> If a contract is re-deployed, `initialize` is idempotent-guarded (it panics if
> already initialized). Re-running init against an already-initialized contract
> will report the error and stop; deploy a fresh instance or skip that contract.
