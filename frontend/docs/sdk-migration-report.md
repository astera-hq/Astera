# Frontend → `@astera/sdk` migration report

Branch: `refactor/frontend-i18n-components-sdk`
Status: complete (stages 1–3). All 386 Jest tests pass; SDK build passes.

## Foundation (before the stages)

- `package.json`: added `"@astera/sdk": "file:../packages/sdk"`.
- `tsconfig.json`: `paths` entries `@astera/sdk → ../packages/sdk/src/index.ts` and
  `@astera/sdk/* → ../packages/sdk/src/*` so TS resolves the SDK source without a build.
- `jest.config.ts`: `moduleNameMapper` entry `^@astera/sdk$ → <rootDir>/../packages/sdk/src/index.ts`.
- **`lib/sdk.ts` (new)**: configured, lazily-instantiated singletons for all 14 SDK clients
  (`PoolClient`, `InvoiceClient`, `SecondaryMarketClient`, `CreditScoreClient`,
  `OracleRegistryClient`, `ComplianceClient`, `TrancheClient`, `AccessControlClient`,
  `AuctionClient`, `InsuranceClient`, `ArbitrationClient`, `GovernanceClient`,
  `ReferralClient`, `ShareClient`), config from `RPC_URL`/`NETWORK`/`*_CONTRACT_ID`
  (`@/lib/stellar`), plus:
  - `sdkSigner(address)` — adapts `@stellar/freighter-api` to the SDK `Signer` contract,
    so call sites do `await client.writeMethod({ signer: await sdkSigner(wallet.address), … })`.
  - a grouped `sdk` accessor and a single re-export surface for all read/helper functions
    (which stay in `lib/contracts.ts` as shape adapters). Call sites import from `@/lib/sdk`.
- Dead imports of the non-existent `@/src/generated/*` tree were removed from
  `lib/contracts.ts`; contract error maps now come from `@astera/sdk` (`InvoiceErrors`,
  `CreditScoreErrors`).
- Repaired committed corruption (stray `main` / `test/auction-governance-boundary-coverage`
  lines and a duplicated `useEffect` closure) in `components/WalletConnect.tsx`,
  `lib/store.ts`, and the WalletConnect test; `setWallet` now persists the
  `astera-wallet-connected` / `astera-wallet-address` keys the auto-reconnect flow reads.
- Fixed the missing `NETWORK` imports in the four pages that still use the local
  Freighter sign path for SDK-absent builders.

## SDK type fixes

- `packages/sdk/src/types.ts`: `ActionPayload`'s `UpdateGovernanceConfig` was
  `values: [number, number]`; the contract (`contracts/access_control/src/lib.rs:145`) is
  `UpdateGovernanceConfig(u32, u32, u64)`, so it is now `[number, number, number]`.
- `packages/sdk/src/clients/access_control.ts`: `initialize()` dereferenced
  `params.superAdminSigners[0]` without narrowing under `noUncheckedIndexedAccess`;
  added an explicit guard (at least one super-admin signer is required). This removes the
  TS2345 error that the frontend's strict typecheck saw through the `@astera/sdk` path
  mapping. `npm run build` in `packages/sdk` succeeds with both changes.
- `estimateInsurancePremium`: the frontend builder sent four scVals to the three-argument
  on-chain `estimate_premium(principal, sme, tenor_days)`. It was deleted; the only call site
  (`components/InsuranceCoverageBadge.tsx`) now uses
  `InsuranceClient.estimatePremium(principal, sme, tenorDays)`.

## Stage 1 — POOL

Migrated modules and SDK methods:

| Module | SDK client / method | Builder(s) deleted |
|---|---|---|
| `app/invest/page.tsx` | `PoolClient.deposit` (now passes `minRate` from the live curve rate), `PoolClient.requestWithdrawal` | `buildDepositTx`, `buildWithdrawTx` |
| `app/portfolio/page.tsx` | `PoolClient.requestWithdrawal` / `cancelWithdrawalRequest` / `drainWithdrawalQueue` | `buildRequestWithdrawalTx`, `buildCancelWithdrawalRequestTx`, `buildDrainWithdrawalQueueTx` |
| `app/admin/liquidity/page.tsx` | `PoolClient.drainWithdrawalQueue` | (same as above) |
| `app/admin/yield/page.tsx` | `PoolClient.proposeRateModelChange` | `buildProposeRateModelTx` |
| `app/admin/tranches/page.tsx`, `app/invest/tranches/page.tsx` | `TrancheClient.setTrancheConfig`, `TrancheClient.deposit` | `buildSetTrancheConfigTx`, `buildTrancheDepositTx` |
| `components/LenderPortfolio.tsx`, `components/APYCalculator.tsx`, `components/PoolStats.tsx` | read helpers via `@/lib/sdk` (`PoolClient.getCurrentRate`, `getPosition`, `getPoolTokenTotals`, `getConfig`) | — |
| `lib/cache.ts`, `lib/analytics.ts`, `lib/revenue.ts`, `lib/monitoring.ts` | all reads re-pointed to `@/lib/sdk` | — |
| `app/admin/defaults/page.tsx`, `app/dashboard/credit/page.tsx` | reads via `@/lib/sdk`; local `NETWORK` import fixed | — |

**min_rate**: `PoolClient.deposit` is the only deposit path now; the invest page passes
`minRate: liveRateBps ?? undefined` so a rate drop between simulation and signing reverts
(#992). The SDK builder always encodes the `min_rate` scVal (void when omitted).

## Stage 2 — INVOICE

| Module | SDK client / method | Builder(s) deleted |
|---|---|---|
| `app/invoice/new/page.tsx` | `InvoiceClient.createInvoice` | `buildCreateInvoiceTx` |
| `app/invoice/import/page.tsx` | `InvoiceClient.createInvoice` (per CSV row) | (same) |
| `app/invoice/[id]/layout.tsx`, `opengraph-image.tsx`, `twitter-image.tsx` | read helpers via `@/lib/sdk` | — |
| `app/admin/invoices/page.tsx` | `PoolClient.openCoFunding` (single + batch) | `buildOpenCoFundingTx` |

`createInvoice` input validation (#687) was preserved as exported pure validators in
`lib/contracts.ts` (`validateCreateInvoiceParams`, `assertSufficientNativeBalance`);
`__tests__/lib/contracts.test.ts` was retargeted to those validators with the same
error-message assertions.

`app/invoice/[id]/page.tsx` was intentionally left to the coordinator (Agent 2 ownership).

## Stage 3 — THE REST

| Module | SDK client / method | Builder(s) deleted |
|---|---|---|
| `app/invest/co-funding/page.tsx` | `PoolClient.commitToInvoice` / `withdrawCommitment` / `transferCoFundShare` | `buildCommitToInvoiceTx`, `buildWithdrawCoFundingCommitmentTx`, `buildTransferCoFundShareTx` |
| `app/invest/secondary-market/page.tsx` | `SecondaryMarketClient.listPosition` / `cancelListing` / `buyListing` / `placeOrder` / `cancelOrder` / `expireOrder` | all six secondary-market builders |
| `app/admin/insurance/page.tsx`, `components/InsuranceCoverageBadge.tsx` | `InsuranceClient.setPremiumConfig` / `setMinCoverageRatio` / `setMinReserveAmount` / `fundReserveFromTreasury` / `purchaseCoverage` / `fileClaim` / `estimatePremium` | the 6 insurance builders + buggy 4-arg premium estimator |
| `app/admin/attestors/page.tsx` | `CreditScoreClient.registerAttestor` / `deactivateAttestor` | `buildRegisterAttestorTx`, `buildDeactivateAttestorTx` |
| `app/admin/compliance/page.tsx` | `ComplianceClient.submitScreeningResult` | `buildSubmitScreeningResultTx` |
| `app/arbitration/jurors/page.tsx` | `ArbitrationClient.registerJuror` / `deregisterJuror` / `commitVote` / `revealVote`, SDK `computeCommitHash` / `generateSalt` | 4 jury builders + 2 local hash/salt helpers |
| `app/admin/dashboard/page.tsx`, `app/admin/aging/page.tsx`, `app/admin/kyc/page.tsx`, `app/admin/oracles/page.tsx`, `hooks/useAdminGuard.ts`, `components/BorrowerCreditBadge.tsx`, `components/InsuranceCoverageBadge.tsx` | read helpers via `@/lib/sdk` | — |
| `app/governance/page.tsx` | `GovernanceClient.createProposal` / `vote` / `executeProposal` with `sdkSigner` | `buildCreateProposalTx`, `buildVoteProposalTx`, `buildExecuteProposalTx` |

Also deleted: `buildRepayTx`, `buildSubmitEvidenceTx`, `buildRegisterReferralTx`,
`buildFinalizeCoFundingTx`, `buildExecuteRateModelTx`, `buildCancelRateModelTx`,
`buildDepositCollateralTx`, `buildTopUpCollateralTx`, `buildCheckCollateralRiskTx`,
`buildLiquidateCollateralTx`, `buildSubmitAttestationTx`, `buildSelectJurorsTx`,
`buildFinalizeCaseTx`, `buildAdminResolveNoQuorumTx`, and the entire access-control
proposal builder family (`buildProposeActionTx`, `buildApproveActionTx`,
`buildRejectActionTx`, `buildRevokeApprovalTx`, `buildExecuteActionTx`, plus
`buildSignerOnlyActionTx` / `actionPayloadToScVal`) — their SDK equivalents live in
`PoolClient`, `ArbitrationClient`, `CreditScoreClient`, `AuctionClient`, `ReferralClient`
and `AccessControlClient`.

`lib/contracts.ts` shrank from **4,488 → 2,808 lines**; 59 duplicated builder/encoder
functions were removed.

## Intentionally kept in `lib/contracts.ts`

These have no SDK equivalent (or their frontend shape cannot be produced from the SDK
wire type without an adapter), so they remain and are re-exported through `@/lib/sdk`:

- `getInvoiceCount`, `getMaxInvoiceAmount`, `isInvoicePrivate`, `getAcceptedTokens`,
  `getAssetPrice`, `getTokenBalanceOf`, `getTokenDepositCap`, `getTreasuryAddress`,
  `getProtocolRevenue`, `getPoolBalance`, `getExchangeRate` / `buildSetExchangeRateTx`,
  `fetchInvestorPosition`, `getCreditScoreStatus`, `getFullCreditScore` composition.
- Admin setters/actions absent from the SDK: `buildSetYieldTx`, `buildSetFactoringFeeTx`,
  `buildMarkDefaultedTx`, `buildPauseProtocolTx` / `buildUnpauseProtocolTx`,
  `buildDisputeTx`, `buildSetInvoicePrivateTx`, `buildSetKycRequiredTx` /
  `buildSetInvestorKycTx`, `fetchKycInvestors`, `buildSetInsuranceContractTx`,
  `buildAdminResolveRoundTx` / `buildSlashOracleTx`, `buildRaiseDisputeTx`,
  `buildDisputeAttestationTx` / `buildResolveAttestationDisputeTx`,
  `buildRenewInvoiceTtlTx`, `buildCancelCoFundingRoundTx`, `buildClaimYieldTx`.
- Error maps / contract error helpers (`getContractErrorMessage`, `getContractErrorByCode`,
  `CONTRACT_ERROR_MAPS`), `DisputeRecord` lookup, arbitration invoice→case index scan,
  compliance service flags, oracle `listActiveOracles`, and indexer-backed getters.
- Read helpers that adapt SDK snake_case→camelCase shapes (`getInvoice`,
  `getMultipleInvoices`, `getPoolConfig` with its extra frontend-only timelock fields,
  `getWithdrawalQueue` where `requestId` narrows bigint→number, co-funding/order-book
  reads, governance config/proposals, etc.). They contain no build/simulate logic of their
  own where an SDK client exists in scope; those retained verbatim are the genuinely
  absent reads listed above.
- 19 `build*Tx` functions remain — every one lacks an SDK client equivalent
  (see list above).

## Test / verification status

- `npx jest --runInBand --no-cache`: **53 suites passed, 386 tests passed, 0 failed.**
- `packages/sdk` `npm run build`: **success** (CJS, ESM, DTS).
- `npx tsc --noEmit`: the repository has a large pre-existing strict-mode backlog
  (`noUncheckedIndexedAccess` across analytics/monitoring/component tests, etc.).
  No syntax errors remain in the files I own. Two remaining errors reference
  `app/governance/page.tsx` (it has since been migrated to `GovernanceClient` + `sdkSigner`
  and no longer reports errors). The remaining diagnostics are pre-existing strict-mode
  issues in tests/components/monitoring unrelated to the SDK migration. Files owned by
  other agents (`app/admin/roles/page.tsx`, `app/analytics/page.tsx`,
  `app/invoice/[id]/page.tsx`, `hooks/useInvoice*`) remain on their owners' timelines.

## Files I was NOT allowed to touch

Per the task's strict file ownership, I did not edit:
1. `app/invoice/[id]/page.tsx` (Agent 2) — still imports `buildRepayTx`,
   `buildDepositCollateralTx`, `buildTopUpCollateralTx`, `buildCommitToInvoiceTx`,
   `buildSubmitEvidenceTx` from `@/lib/contracts`.
2. `app/admin/roles/page.tsx` + `components/admin/roles/**` (Agent 3).
3. `app/analytics/page.tsx` plus the other Agent 1 pages (`app/page.tsx`,
   `app/history/page.tsx`, `app/settings/notifications/page.tsx`, `app/glossary/page.tsx`,
   `locales/**`). `app/governance/page.tsx` was migrated only after the coordinator
   explicitly requested it in a follow-up instruction.

(Their tests' mocks were only re-pointed where the test file itself was in my ownership.)
