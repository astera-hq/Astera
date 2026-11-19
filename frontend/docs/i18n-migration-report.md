# i18n migration report — six page chrome surfaces

Scope: `app/page.tsx`, `app/history/page.tsx`, `app/governance/page.tsx`,
`app/analytics/page.tsx`, `app/settings/notifications/page.tsx`,
`app/glossary/page.tsx`.

New top-level namespaces (added to `locales/{en,fr,es}/common.json` with the
same flattened key set): **`Landing`, `History`, `Governance`, `Analytics`,
`NotificationSettings`, `Glossary`**.

- Server components (`app/page.tsx`, `app/glossary/page.tsx`) use
  `await getTranslations(<Namespace>)` inside the component and inside
  `generateMetadata()`. Static `metadata` exports became `generateMetadata()`
  and keep `alternates: { canonical }`. Module-level `stats`/`features`/`steps`
  arrays became functions of `t` built inside the component (icons and numeric
  values unchanged).
- Client components use `useTranslations('<Namespace>')`. State, effects,
  handlers and data flow are unchanged.
- ICU-safe copy: typographic apostrophes are used so single quotes are not
  consumed by the ICU parser. Placeholders (`{id}`, `{token}`, …) are passed via
  `t(key, values)`.

## 1. `app/page.tsx` — namespace `Landing`

**(a) Hardcoded strings migrated**

| UI string (before) | Key |
|---|---|
| `Astera — Real World Assets on Stellar` (metadata title) | `Landing.metadata.title` |
| `Tokenize invoices. Fund real businesses. …` (metadata description) | `Landing.metadata.description` |
| `Built on Stellar Soroban · Testnet Live` | `Landing.hero.badge` |
| `Real invoices.` / `Real yield.` / `Real impact.` | `Landing.hero.heading1` / `heading2` / `heading3` |
| Hero paragraph | `Landing.hero.description` |
| `Tokenize an Invoice` / `Invest in the Pool` | `Landing.hero.ctaTokenize` / `ctaInvest` |
| 4 stat labels + sub-captions | `Landing.stats.{gap,settlement,apy,fee}{Label,Sub}` |
| `The infrastructure layer for` / `emerging market finance` / intro paragraph | `Landing.features.headingPrefix` / `headingAccent` / `intro` |
| 6 feature titles + descriptions | `Landing.features.{tokenize,liquidity,escrow,credit,usdc,diaspora}{Title,Desc}` |
| `How Astera works` | `Landing.steps.heading` |
| 3 step roles, titles, descriptions | `Landing.steps.{sme,pool,investor}{Role,Title,Desc}` |
| CTA heading / paragraph / button | `Landing.cta.title` / `description` / `button` |
| `Built on Stellar · Powered by` / `· USDC Settlement` | `Landing.footer.builtOnPrefix` / `builtOnSuffix` |
| Footer links `Dashboard` / `Invest` / `Glossary` | `Landing.footer.{dashboard,invest,glossary}` |

**(b) Strings from child components — left alone**

- `components/GlossaryTerm.tsx` (`<GlossaryTerm id="soroban" />` in the
  footer): supplies the visible term label ("Soroban") and the tooltip
  definition. It owns that copy (read from `lib/glossary.ts`) and is a shared
  child; re-translating it from the parent would duplicate its responsibility.
  Note: this child currently renders English glossary data directly rather than
  calling next-intl — it is outside this migration's ownership, so a follow-up
  should migrate it if its copy needs localisation.

**(c) Deliberately left as data / non-UI**

- `Astera` (brand/proper noun) in the footer.
- Stat values `$1.7T`, `3-5s`, `8%`, `<$0.01`, step numbers `01`/`02`/`03`,
  and feature glyph icons `◈ ◉ ◎ ◐ ◑ ◒` (numeric/icon values, not copy).

## 2. `app/history/page.tsx` — namespace `History`

**(a) Hardcoded strings migrated**

| UI string (before) | Key |
|---|---|
| `Transaction History` / `On-chain activity for your wallet` | `History.title` / `History.subtitle` |
| `Export CSV` / `Export PDF` | `History.exportCsv` / `History.exportPdf` |
| Module-level `KIND_LABELS` (all 8 event kinds) | `History.kinds.{invoiceCreated,invoiceFunded,invoicePaid,invoiceDefaulted,poolDeposit,poolFunded,poolRepaid,poolWithdraw}` |
| `All` filter chip | `History.filterAll` |
| `↓ Newest` / `↑ Oldest` | `History.sortNewest` / `History.sortOldest` |
| `Connect your wallet` + description | `History.connect.title` / `History.connect.description` |
| Two empty-state messages | `History.empty.filtered` / `History.empty.none` |
| `Invoice #{id}` | `History.invoice` (`{id}`) |
| `+{amount} {symbol} interest` | `History.interest` (`{amount}`, `{symbol}`) |
| `View tx ↗` | `History.viewTx` |
| `Loading…` / `Load more` | `History.loading` / `History.loadMore` |
| Two error toasts | `History.error.notConfigured` / `History.error.loadFailed` |
| CSV header row (`Date`, `Type`, `Invoice ID`, `Amount (…)`, `Interest (…)`, `Address`, `Tx Hash`) | `History.csv.{date,type,invoiceId,amount,interest,address,txHash}` |

`exportCSV(events, t)` now receives the translator so CSV headers and event
kinds are localised too.

**(b) Strings from child components — left alone**

- `components/Skeleton.tsx` → `HistoryEventSkeleton`: shimmer placeholders
  only; contains no text, so there is nothing to translate. The 6 skeletons
  and their count are unchanged.

**(c) Deliberately left as data / non-UI**

- `formatTs()` uses `en-US` `Intl` date formatting and the `—` placeholder —
  date formatting/symbol, not translatable UI copy; kept to preserve behaviour.
- Explorer base URL, CSV filename `astera-history-<date>.csv`, contract IDs,
  env var names (kept literal even inside translated error copy), and
  `stablecoinLabel()` output are identifiers/data.
- `console.warn('[History] …')` is a developer log, not UI.

## 3. `app/governance/page.tsx` — namespace `Governance`

**(a) Hardcoded strings migrated**

| UI string (before) | Key |
|---|---|
| `Governance` eyebrow, page title, intro paragraph | `Governance.eyebrow` / `title` / `intro` |
| `Voting Power:` / `(min N required)` | `Governance.votingPower` / `minRequired` (`{min}`) |
| `Refresh` / `Refreshing...` | `Governance.refresh` / `refreshing` |
| `Hide Form` / `New Proposal` | `Governance.hideForm` / `newProposal` |
| Contract-not-configured notice | `Governance.notConfigured` |
| Form heading, intro, `Share-weighted` | `Governance.form.title` / `intro` / `shareWeighted` |
| `Target contract` / `Function name` / `Description` / `Calldata` | `Governance.form.{targetContract,functionName,description,calldata}` |
| Description placeholder (`Raise the pool yield by 50 bps`) | `Governance.form.descriptionPlaceholder` |
| `Submitting...` / `Insufficient Balance` / `Create Proposal` | `Governance.form.{submitting,insufficient,create}` |
| Insufficient-power `title` tooltip | `Governance.form.insufficientTitle` (`{min}`) |
| `Proposal History`, loading/count lines, `Selected #N` | `Governance.history.{title,loading,total,filtered,selected}` |
| Six status filter chips | `Governance.filters.{all,active,passed,rejected,executed,expired}` |
| Both empty states | `Governance.empty.{none,filtered}` (`{status}`) |
| `Proposer …` / `Ends …` / `For …` / `Against …` | `Governance.proposal.{proposer,ends,for,against}` |
| `Load more (N remaining)` | `Governance.loadMore` (`{remaining}`) |
| `Vote`, `Voting power:`, `Target: …`, `Function: …`, `Vote For`, `Vote Against`, `Select a proposal to vote.` | `Governance.vote.{title,power,target,function,for,against,select}` |
| `Execute`, its help text, `Execute Selected` | `Governance.execute.{title,intro,button}` |
| Four toasts | `Governance.toast.{loadFailed,connectFirst,submitted,failed}` |

**(b) Strings from child components — left alone**

- None. This page renders no shared translated child components; all visible
  copy is produced by the page itself.

**(c) Deliberately left as data / non-UI**

- `STATUS_FILTERS` values (`'All'`, `'Active'`, …) and `proposal.status`
  comparisons are code identifiers; only their display labels are translated.
- On-chain data rendered verbatim: `proposal.description`, addresses,
  `proposal.functionName`, `proposal.calldata`, formatted dates, vote counts,
  `votingPower`/`minShareBalance` numbers, the `—` placeholder.
- Input placeholder examples that are code/identifier literals: `C...`,
  `set_yield_rate`, `{"yield_bps":850}` (the natural-language description
  placeholder *was* translated).
- `console.error('[Governance] …')` is a developer log.

## 4. `app/analytics/page.tsx` — namespace `Analytics`

**(a) Hardcoded strings migrated**

| UI string (before) | Key |
|---|---|
| `Pool Analytics` / intro paragraph | `Analytics.title` / `Analytics.intro` |
| `Updated <time>` | `Analytics.updated` (`{time}`) |
| `Loading…` / `Refresh` | `Analytics.loading` / `Analytics.refresh` |
| Not-deployed notice (around the `<code>` env name) | `Analytics.notConfiguredPrefix` / `notConfiguredSuffix` |
| `Failed to load analytics data.` fallback | `Analytics.error.loadFailed` |
| 4 stat card labels + sub-captions | `Analytics.stats.{totalPoolValue,deployedCapital,availableLiquidity,targetApy}` + `…Sub` |
| `Capital Allocation by Token`, `Utilization`, `Pool NAV`, `Deployed`, `Available` | `Analytics.allocation.{title,utilization,nav,deployed,available}` |
| `Rate Curve — <token>` (parent-supplied chart title) | `Analytics.rateCurveTitle` (`{token}`) |
| Historical-trends footnote | `Analytics.footer` |
| `Invest Now` / `View Portfolio` | `Analytics.links.{invest,portfolio}` |

`UtilizationBar` now takes a translated `label` prop instead of the hardcoded
`Utilization` text; `StatCard` already took labels as props, so both now
receive translated values.

**(b) Strings from child components — left alone**

- `components/analytics/PoolUtilizationChart.tsx`, `YieldPerformanceChart.tsx`,
  `InvoiceFunnelChart.tsx`, `RecentEventsFeed.tsx`, `RateCurveChart.tsx`: these
  components own their headings, axis labels and feed copy (e.g.
  `Recent On-Chain Events`, `No recent events detected.`,
  `now: x% util → y% APY`). The page only supplies data plus the `title` prop,
  which is translated above; the children's own copy is not re-translated from
  the parent and their files are outside this migration's ownership.
  Heads-up: inspection showed these children currently use hardcoded English
  (not `useTranslations`), so migrating them is a worthwhile follow-up.
- `components/Skeleton.tsx` → `StatCardSkeleton` / `ChartSkeleton`: placeholder
  shimmer only, no text.

**(c) Deliberately left as data / non-UI**

- `NEXT_PUBLIC_POOL_CONTRACT_ID` inside `<code>` (env constant identifier),
  stablecoin symbols from `stablecoinLabel()`, numeric/percentage values and
  chart data, `formatUSDC()` output, and `new Date().toLocaleTimeString()`
  output (runtime-formatted time).

## 5. `app/settings/notifications/page.tsx` — namespace `NotificationSettings`

**(a) Hardcoded strings migrated**

| UI string (before) | Key |
|---|---|
| `Notification settings` + intro | `NotificationSettings.title` / `intro` |
| `In-app notifications` + local-storage note | `NotificationSettings.inApp.title` / `description` |
| `Reset` | `NotificationSettings.reset` |
| `Email notifications` + server-side note | `NotificationSettings.email.title` / `description` |
| `Subscribe to email alerts` + `Requires a valid email address.` | `NotificationSettings.email.subscribe` / `subscribeDesc` |
| `Email address` / `Enter a valid email address.` / `Events to send via email` | `NotificationSettings.email.{address,invalid,events}` |
| `Webhook forwarding` + its help text | `NotificationSettings.webhook.title` / `description` |
| `Enable webhook forwarding` + `Requires a valid URL.` | `NotificationSettings.webhook.{enable,enableDesc}` |
| `Webhook URL` / `Enter a valid http/https URL.` / `Test webhook` / `Testing…` / `Events to forward` | `NotificationSettings.webhook.{url,invalid,test,testing,events}` |
| `Save email & webhook` / `Saving…` | `NotificationSettings.save` / `saving` |
| `SME` / `Investor` / `Both` pills | `NotificationSettings.audience.{sme,investor,both}` |
| 11 toast messages (unsubscribe, validation errors, saved/failed, test, reset) | `NotificationSettings.toast.*` |

`EventPill` now receives a translated `label` prop; the audience union
(`'SME' | 'Investor' | 'Both'`) stays a code value and is mapped to the
translated label.

**(b) Strings from child components — left alone**

- None. `Toggle` and `EventPill` are local components in this same file and
  were updated in place; no shared translated child is rendered here.

**(c) Deliberately left as data / non-UI**

- `e.label` and `e.description` for every event row come from `NOTIFICATION_EVENTS`
  in `lib/notification-preferences.ts`. That is content data outside this
  migration's ownership, so it is intentionally left untouched (same category
  as `lib/glossary.ts` data).
- Example placeholders `you@example.com` and `https://example.com/webhook`.
- Server-provided error text from the API (`body.error`, `HTTP <status>`) and
  email addresses from query params.

## 6. `app/glossary/page.tsx` — namespace `Glossary`

**(a) Hardcoded strings migrated**

| UI string (before) | Key |
|---|---|
| `Glossary — Astera` (metadata title) | `Glossary.metadata.title` |
| `Plain-language explanations of …` (metadata description) | `Glossary.metadata.description` |
| `Glossary` heading | `Glossary.title` |
| Intro paragraph | `Glossary.intro` |

**(b) Strings from child components — left alone**

- None — the page renders the glossary data directly; no shared child
  components are used.

**(c) Deliberately left as data / non-UI**

- `glossary` entries from `lib/glossary.ts` (`entry.term` and
  `entry.definition`), explicitly identified as content data to leave untouched.
- The `#anchor` ids (`entry.id`) are data identifiers.

## Test / tooling notes

- `npm run check:locales` passes (fr/es match en exactly).
- All 609 new locale messages parse cleanly with `intl-messageformat` (no ICU
  errors).
- `npx jest`: `__tests__/GlossaryPage.test.tsx` passes after being updated to
  `render(await GlossaryPage())` (server components are now async). Mock-sensitive
  suites `__tests__/components/Navbar.test.tsx` and
  `app/invest/__tests__/page.test.tsx` still pass — the `useTranslations` mock
  was deliberately left returning the key. `__mocks__/next-intl.js` is **byte-for-byte
  unchanged** (`git diff` is empty), so no test that asserts on literal keys or
  literal English strings was affected.
- `__tests__/components/NetworkMismatchBanner.test.tsx` fails, but this is a
  pre-existing, unrelated failure: both the test file and
  `components/NetworkMismatchBanner.tsx` are unmodified from `HEAD`, the banner
  uses `useTranslations` (the unchanged client mock, not the server mock I
  touched), and the test asserts on stale keys (`messageStart`, `helpLink`)
  that the component never renders (it renders `description`, `switchNetworks`).
  It cannot be satisfied through the mock without breaking other tests, and the
  test file is outside this migration's ownership — so it was left untouched.
  The other failing suites (`WalletConnect`, `lib/analytics`, `lib/contracts`,
  `lib/sse-events`, `transaction-pages`) are likewise unrelated concurrent SDK
  work; none import the six migrated pages.
- `__mocks__/next-intl-server.js` now exports `getTranslations(namespace?)`, an
  async function returning a translator that resolves `namespace.key` against
  the real English JSON (simple `{param}` interpolation, key fallback).
  `getMessages`/`getLocale`/`getRequestConfig` behaviour is unchanged, so no
  other mock consumer changed behaviour.
- `npx tsc --noEmit`: the only diagnostics in owned files are three
  `TS2305` errors on the pre-existing `buildCreateProposalTx` /
  `buildVoteProposalTx` / `buildExecuteProposalTx` imports in `app/governance/page.tsx`
  (those exact imports exist in `HEAD`; another agent's SDK refactor removed the
  exports from `lib/contracts`). A pre-existing `noUncheckedIndexedAccess`
  error in `app/history/page.tsx` (`events[0].token`) was fixed with the
  behaviour-identical `events[0]?.token`. All other remaining `tsc` errors are in
  files outside this migration's ownership. A TypeScript syntax-only check of
  every owned file passed.
