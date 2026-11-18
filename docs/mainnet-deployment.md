# Mainnet Deployment Guide

> **⚠️ Important:** Mainnet deployment involves real assets. Complete all
> security audits and testing on testnet (see [Testnet Deployment Guide](deployment.md))
> before deploying to production.

This guide collects the operational checklist for a production Astera
deployment. It reuses the same scripts as testnet
(`scripts/deploy-testnet.sh` and `scripts/init-contracts.sh`) with a real
`NETWORK=mainnet` and production-grade keys.

## Pre-deployment security checklist

- [ ] All contracts audited; audit findings resolved.
- [ ] Full testnet run completed: deploy → init → smoke test all green.
- [ ] `upgrade-dry-run.sh` run against the candidate wasm for every contract
      you plan to upgrade (see [Upgrade Dry-Run Guide](upgrade-dry-run.md)).
- [ ] `access_control` SuperAdmin is a real M-of-N signer set, **not** a single
      `ADMIN_ADDRESS` (set `SUPER_ADMIN_SIGNERS` and `SUPER_ADMIN_THRESHOLD`).
- [ ] `ADMIN_ADDRESS` and `DEPLOYER_KEY` are separate, cold-stored accounts.
- [ ] `USDC_TOKEN_ID` points to the **mainnet** native USDC SAC.
- [ ] `TRANCHE_SENIOR_SHARE_TOKEN_ID` / `TRANCHE_JUNIOR_SHARE_TOKEN_ID` are the
      real, distinct tranche token contracts (override the share-token default).
- [ ] Off-chain services (oracle, compliance, indexer) configured with
      production secrets and monitors.

## Deployment procedure

1. **Build** all 14 contracts for `wasm32-unknown-unknown` (release).
2. **Deploy** with `NETWORK=mainnet` and a funded production deployer:
   ```bash
   export DEPLOYER_KEY=<mainnet_deployer>
   export NETWORK=mainnet
   sh scripts/deploy-testnet.sh
   ```
3. **Initialize** with production admin + tranche/stake overrides:
   ```bash
   source deployed-mainnet.env
   export DEPLOYER_KEY=<mainnet_deployer>
   export ADMIN_ADDRESS=<prod_admin>
   export USDC_TOKEN_ID=<mainnet_usdc>
   export SUPER_ADMIN_SIGNERS='["G...","G..."]'
   export SUPER_ADMIN_THRESHOLD=2
   sh scripts/init-contracts.sh
   ```
4. **Smoke test**: `bash scripts/smoke-test-testnet.sh` (point `STELLAR_RPC_URL`
   at mainnet RPC).
5. **Verify** each contract's `version()` and that `access_control` is wired in.

## Contract verification

Publish the wasm / verify source on Stellar Expert or your explorer of choice so
users can confirm the on-chain bytecode matches the audited source.

## Monitoring and alerting

- Watch deployer and admin account balances.
- Alert on failed oracle rounds, paused contracts, and compliance screen failures.
- See `docs/keeper-monitor.md` for keeper/monitor setup.

## Rollback and emergency procedures

- `access_control` provides a multisig emergency path; rehearse the proposal
  flow **before** go-live.
- Keep the previously deployed wasm artifacts so you can revert via the upgrade
  path described in [Contract Upgrade Guide](contract-upgrade-guide.md).

## Post-deployment verification

- [ ] All 14 contract IDs present in the generated env file.
- [ ] `initialize` succeeded on all 14 (no "already initialized" on first run).
- [ ] `access_control` wired into invoice/pool/credit_score/governance/
      compliance/oracle_registry/referral.
- [ ] Frontend points at the new contract IDs and connects on mainnet.
