# Roles-admin component split report

## Goal

Split the monolithic `frontend/app/admin/roles/page.tsx` (1,164 lines) into a thin
orchestration component plus focused components under
`frontend/components/admin/roles/`, and add pure-function unit tests for the
proposal payload encoder — **without changing any behavior or user-facing text**
(this page is not part of the i18n migration).

## Line-count reduction

| File | Before | After |
|------|-------:|------:|
| `app/admin/roles/page.tsx` | 1,164 | **194** |
| Net reduction in the page | — | **970 lines (−83%)** |

The page now owns only: state, loading/`load`, `signAndSubmit`, `isSigner`, the
propose/signer/execute submit helpers, and composition of the three main
sections. All markup, dynamic fields and summaries moved out.

## New files and responsibilities

| File | Lines | Responsibility |
|------|-------:|----------------|
| `components/admin/roles/proposalPayload.ts` | 339 | **Pure** module. Exports `buildActionPayload(action, fields): ActionPayload \| null`, `summarizeAction(action)`, `TargetKey`, `ACTIONS_BY_TARGET`, `ACTION_LABELS`. Imports only `@/lib/types` (types + `parseStellarAddress` + `ROLE_LABELS`) — no React, no `@/lib/stellar`, no `@/lib/contracts`, so it is directly unit-testable. |
| `components/admin/roles/constants.ts` | 30 | `TARGET_CONTRACTS` (the one piece of config needing live contract IDs from `@/lib/stellar`). Re-exports `TargetKey`, `ACTIONS_BY_TARGET`, `ACTION_LABELS` for component consumers. |
| `components/admin/roles/ActionFields.tsx` | 451 | The dynamic per-action field inputs (moved verbatim from the old `ActionFields` function at page line ~717), covering all ~50 action variants. |
| `components/admin/roles/ProposalBuilder.tsx` | 100 | "Propose an Action" form: role/target/action selectors, `ActionFields`, submit button. Owns the target-change reset logic (first action + cleared fields). Submission callback is wired to the page's `handlePropose`. |
| `components/admin/roles/ProposalList.tsx` | 110 | "Proposals" queue: skeleton/empty states, per-proposal summary, status badge, approval count, proposer/expiry line, and `ApprovalActions`. Computes the `canApprove`/`canReject`/`canRevoke`/`canExecute` flags exactly as before. |
| `components/admin/roles/ApprovalActions.tsx` | 69 | Approve / Execute / Revoke Approval / Reject buttons with the original labels, ordering, classes and disabled state. Receives callbacks from the page; no contract imports of its own. |
| `components/admin/roles/SignerManagement.tsx` | 69 | "Roles" section: role-config cards, threshold/signer display, "You are a signer" badge, skeleton. Add/remove-signer and threshold changes remain multisig proposal actions (`AddSigner` / `RemoveSigner` / `SetThreshold`) exposed by `ProposalBuilder`, as in the original page. |
| `__tests__/admin-roles-proposal-builder.test.ts` | 309 | Pure-function tests for the encoder and `summarizeAction` (see below). |

### Preserved behavior / constraints

- All existing `@/lib/contracts` call sites remain in `page.tsx` (no SDK
  migration performed here); `@/lib/types.ts`, `lib/contracts.ts`,
  `lib/stellar.ts`, locales and other pages were not touched.
- All admin strings, button labels, Tailwind classes, handler logic and toast
  messages are unchanged.
- The encoder is a pure function of `(action, fields)`; the page passes
  `proposeAction` + `fields` into it. Missing-field defaults (`0`, `0n`,
  `false`, whitespace-trimmed addresses) match the original implementation.
- `UpdateGovernanceConfig` is encoded as **three** values
  `[quorumBps, passBps, executionDelaySecs]`, matching the Rust ABI
  `UpdateGovernanceConfig(u32, u32, u64)` at
  `contracts/access_control/src/lib.rs:145` and `lib/types.ts`'s
  `values: [number, number, number]`. The SDK type (currently two values) was
  deliberately not used or edited.

## Test coverage summary

`admin-roles-proposal-builder.test.ts` contains **29 test blocks / 49 executed
cases** (parameterized `it.each` cases counted individually), covering:

- **Boolean variants** — `SetPaused`, `SetKycRequired`,
  `SetOracleRegistryPaused` (+ `SetCompliancePaused`, `SetReferralPaused`),
  true/false and missing-field default.
- **Numeric BPS variants** — `SetYield`, `SetMaxUtilization`,
  `SetBorrowRewardBps`, `SetDepositRewardBps`, plus empty-field default.
- **Address variants** — `SetTreasury`, `AddKeeper`,
  `SetInvoiceAccessControl` (+ all other address/rotation actions), including
  whitespace trimming and `parseStellarAddress` validation.
- **Bigint variants** — `WithdrawRevenue` (exact > 2^53 amount),
  `SetLateThreshold`, `SetRescreeningInterval`/`SetScreenerTimelock`, and
  `0n` default.
- **Compound / struct variants** — `RegisterDebtor`, `SetScoreThresholds`,
  `RegisterAttestor`, `SetOracleRegistryConfig`, `SlashOracle`,
  `AdminResolveRound` (approved/rejected), `SetInvestorKyc`.
- **Role variants** — `AddSigner`, `RemoveSigner`, `SetThreshold`.
- **Optional address** — `SetOracleRegistryTreasury` with empty string and
  whitespace → `[undefined]`; with a valid address → `[address]`.
- **ABI-truth assertions** — `UpdateGovernanceConfig` emits **exactly three**
  values; `SetCategoryQuorum` emits exactly two.
- **`summarizeAction`** — boolean, bigint-bearing, role-based, three-value
  governance config, and `(none)` optional-treasury summaries.

Tests import only the pure `proposalPayload.ts` module — no React rendering and
no Jest mocks for `next-intl`, `@/lib/store` or `@/lib/contracts` are required.

## Validation status

- **Syntax**: all nine new/modified TypeScript files transpile cleanly with the
  TypeScript 5.9.3 compiler API (zero syntax diagnostics).
- **Runtime check of the encoder**: compiled `proposalPayload.ts` + `lib/types.ts`
  to CommonJS in a temp dir and ran **42 independent assertions** mirroring the
  test suite — all passed, including the three-value `UpdateGovernanceConfig`
  and `[undefined]` optional-treasury cases.
- **`npx tsc --noEmit` / `npx jest`**: not executed — `frontend/node_modules`
  (and root `node_modules`) are absent in this environment, so dependencies
  cannot resolve. No installs were attempted, per the task instruction not to
  block on installs.
