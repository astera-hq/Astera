# Invoice detail page split report

## Goal

Split the 1,727-line client component `app/invoice/[id]/page.tsx` into small
feature components plus data-loading hooks, keeping
`app/invoice/[id]/page.test.tsx` green without weakening its assertions.

## Outcome

| Metric | Before | After |
| --- | --- | --- |
| `app/invoice/[id]/page.tsx` | 1,727 lines | **247 lines** |
| Reduction in page | — | **1,480 lines (−85.7%)** |
| New hooks | 0 | 2 |
| New feature components | 0 | 13 |

The page is now a thin composition/orchestration layer: it calls two hooks,
holds the three early-return states (loading / error / private guard), and
renders the feature components in the original order.

## New hooks

### `hooks/useInvoiceDetail.ts` (540 lines)

Encapsulates **all read-only data loading and derived state**:

- Route id (`useParams`) and wallet (`useStore`), exposed as typed data.
- `invoice`, `metadata`, `poolConfig`, `fundedInvoice`.
- Best-effort loads: collateral config/deposit, live oracle ratio, risk
  config, `atRiskSince`, accepted tokens, private flag, credit score,
  co-funding round, dispute record, arbitration case + evidence.
- Transaction-history loader (`loadHistory`) and the pure
  `parseInvoiceHistory` helper (moved verbatim, exported for isolated tests).
- Cross-asset oracle price effect and `requiredCollateralAmount` memo.
- Derived values: `days`, `isOwner`, `isAdmin`, `canViewInvoice`,
  `statusSteps`, `projectedInterest`, `accruedInterest`, `interestProgress`,
  `remainingDue`, `fullyRepaid`.
- `reload()` for post-transaction refreshes and `setIsPrivate` /
  `setCollateralToken` setters for the actions that mutate them.

Exports typed `InvoiceEvent`, `InvoiceEventKind`, `TransactionStep` and
`InvoiceDetailData` interfaces.

### `hooks/useInvoiceActions.ts` (484 lines)

Encapsulates **all async transaction handlers and their local UI state**:

- `simulateRepay` + `useTransactionSimulation` fee estimate.
- `handleRepay`, `handleDepositCollateral`, `handleDispute`,
  `handleRaiseDefaultDispute`, `handleSubmitArbitrationEvidence`,
  `exportInvoicePDF`, `handleShare`, `handleTogglePrivate`,
  `handleCommitToInvoice`.
- Associated state: action loading, repayment amount, both dispute modals and
  their inputs, arbitration evidence input, collateral amount/loading/modal,
  PDF/privacy loading, co-funding amount/loading.

Exports a typed `InvoiceActions` interface. All contract call sites remain on
`@/lib/contracts` — SDK migration is intentionally left to the other worker.

## New feature components (`components/invoice/`)

| Component | Lines | Responsibility |
| --- | --- | --- |
| `InvoiceHeader.tsx` | 77 | Back-to-dashboard link is kept in the page; this renders the main detail card (image, title/status, amount, due date, time remaining, owner, `BorrowerCreditBadge`, description), including the owner-only full-address comment. |
| `InvoicePdfExport.tsx` | 32 | Export PDF + Share toolbar buttons. |
| `InvoicePrivacyControl.tsx` | 24 | Owner-only Make Private / Make Public toggle. |
| `CreditScorePanel.tsx` | 51 | Borrower credit-score card + insurance coverage badge. |
| `Timeline.tsx` | 45 | Status-step timeline. |
| `PoolDetails.tsx` | 54 | Pool config + funded-principal details. |
| `InterestAccrual.tsx` | 68 | Accrued/projected interest and remaining-due summary. |
| `CollateralPanel.tsx` | 312 | Collateral config, deposit/top-up form, cross-asset selector, live oracle ratio, at-risk/liquidated/settled states. |
| `CoFundingPanel.tsx` | 104 | Co-funding progress bar and commit form / wallet connect. |
| `InvoiceHistory.tsx` | 79 | Parsed event timeline with loading skeleton and error text. |
| `RepaymentPanel.tsx` | 71 | Repayment input, estimated fee, submit button, "fully repaid" notice. |
| `ArbitrationPanel.tsx` | 66 | Arbitration case details + evidence submission. |
| `DisputePanel.tsx` | 224 | Dispute review banner, default-dispute modal, raise-dispute modal; composes `ArbitrationPanel`. |

Every component receives explicit typed props (existing types from
`@/lib/types` plus the hooks' exported interfaces). No component reaches into
global state: `useStore` is called only once, inside `useInvoiceDetail`.

`ExtensionPanel`: **N/A** — the original page contains no extension-related
strings or logic (grep confirmed), so none was invented.

## Behavior deliberately left in the page

1. **Private-access guard** (`if (!canViewInvoice) … "Invoice not found."`).
   Kept in the page so it runs before any invoice content — including any new
   child component that could render the title — is composed. This is exactly
   what the test asserts.
2. **Loading skeleton and error / missing-invoice states.** Kept in the page as
   the top-level state router; they are page-level concerns, not feature UI.
3. **Layout wrappers** (`min-h-screen …`, `max-w-2xl mx-auto`) and the
   `space-y-3` action-panel container remain in the page.
4. **Small conditional snippets** — the admin "Open funding queue" link and
   the "invoice is pending review" notice — remain inline because they are
   two-line, data-independent fragments; extracting them would add indirection
   without improving clarity.
5. **Conditional rendering of panels.** Each panel component self-hides by
   returning `null` for absent data instead of the page wrapping every panel in
   a conditional. The rendered output is equivalent to the original.

## Verification

- **Test**: `app/invoice/[id]/page.test.tsx` → **1 passed, 1 total**. Run via a
  temporary Jest config (the repo's `jest.config.ts` fails to parse under the
  installed ts-node/TS combination — a pre-existing environment issue). The
  test's mocks were not changed and no new mocks were needed: all new modules
  import from the same mocked paths (`@/lib/contracts`, `@/lib/stellar`,
  `@/lib/simulateFee`, `@/hooks/useTransactionSimulation`,
  `@/components/*`, `react-hot-toast`).
- **Typecheck**: an isolated `tsc --noEmit` over only the new page, hooks and
  components reports **zero errors in the new files**. (The project-wide
  typecheck is blocked by pre-existing, out-of-scope errors in
  `components/WalletConnect.tsx`, `lib/store.ts`, `lib/contracts.ts` and
  `lib/simulateFee.ts`.)
