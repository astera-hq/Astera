# Contract event topics

Every Astera contract event now has a two-segment topic:

| Contract         | Namespace  | Action format          | Examples                                                           |
| ---------------- | ---------- | ---------------------- | ------------------------------------------------------------------ |
| Invoice          | `invoice`  | lowercase `snake_case` | `created`, `funded`, `default`, `due_ext`, `meta_img`              |
| Pool             | `pool`     | lowercase `snake_case` | `deposit`, `funded`, `part_pay`, `repaid`, `yld_claim`, `wd_queue` |
| Secondary market | `market`   | lowercase `snake_case` | `lst_open`, `lst_buy`, `ord_open`, `ord_fill`, `ord_cncl`          |
| Credit score     | `credit`   | lowercase `snake_case` | `score_cfg`, `payment`, `dispute`, `resolved`, `lt_upd`            |
| Tranche          | `TRANCHE`  | lowercase `snake_case` | `deposit`, `withdraw`, `fund`, `repay`, `default`, `config`        |
| Auction          | `auction`  | lowercase `snake_case` | `col_risk`, `col_safe`, `auc_liq`, `sale_open`, `sale_take`        |
| Compliance       | `COMPLY`   | lowercase `snake_case` | `screened`, `review`, `scr_prop`, `tl_set`, `paused`               |
| Oracle registry  | `ORACLE`   | lowercase `snake_case` | `registrd`, `rnd_open`, `voted`, `consensus`, `cfg_upd`            |
 test/auction-governance-boundary-coverage

| Governance       | `gov`      | lowercase `snake_case` | `vote`, `execute`, `set_gov`, `cat_q`                              |
| Referral         | `REFERRAL` | lowercase `snake_case` | `activatd`, `paused`, `ac_rot`                                     |
| Insurance        | `INSURNCE` | lowercase `snake_case` | `covered`, `cfg_set`, `mcr_set`, `min_rsv`                         |
| Access control   | `accctl`   | lowercase `snake_case` | `proposed`                                                         |
| Share (token)    | `share`    | lowercase `snake_case` | `paused`, `incrallow`, `decrallow`, `xfer_from`                    |
| Arbitration      | `ARBTRTN`  | lowercase `snake_case` | `jurorsel`, `resolved`, `slashed`, `noquorum`                      |

## Invoice (`invoice` namespace)

The `invoice` contract's core lifecycle events:

| Action      | Payload                                                              |
| ----------- | --------------------------------------------------------------------- |
| `created`   | `(id, owner, amount, metadata_uri, timestamp, debtor)`                |
| `funded`    | `(id, pool, owner, timestamp)`                                        |
| `paid`      | `(id, pool, owner, timestamp)`                                        |
| `default`   | `(id, owner, timestamp)`                                              |
| `disputed`  | `(id, timestamp)`                                                     |
| `cancelled` | `(id, owner, timestamp)` or `(id, caller)` depending on the call site |
| `expired`   | `id`                                                                   |
| `paused`    | `(admin, timestamp)`                                                  |
| `unpaused`  | `(admin, timestamp)`                                                  |

Beyond these, `invoice` also emits roughly thirty admin/governance parameter-update
events (`due_window_updated`, `grace_period_updated`, `oracle_updated`,
`gov_min_due`, `gov_max_amt`, `gov_disp_thr`, `gov_orc_reg`, and similar) each
scoped to a single config field. These follow the same
`(EVT, symbol_short!("<action>"))` two-segment topic shape as everything else,
but are not individually tabulated here since they're low-frequency and named
directly after the config field they change — grep `contracts/invoice/src/lib.rs`
for `events().publish` if a specific one is needed.

## Pool (`pool` namespace)

The `pool` contract's core, highest-volume events — the ones the indexer and
frontend depend on most:

| Action      | Payload                                                  |
| ----------- | ----------------------------------------------------------- |
| `deposit`   | `(investor, token, amount_received, shares_minted, timestamp)` |
| `withdraw`  | `(investor, token, amount, shares, timestamp)`               |
| `wd_queue`  | `(investor, token, shares, request_id)` — queued when liquidity is insufficient for an immediate withdrawal |
| `funded`    | `(invoice_id, sme, principal, token, ...)` — emitted from both direct funding and co-funding finalization |
| `repaid`    | `(invoice_id, payer, principal, total_interest, timestamp)` — full repayment |
| `part_pay`  | `(invoice_id, actual_payment, repaid_amount, timestamp)` — partial repayment |
| `yld_claim` | `(investor, token, claimable, bonus)`                        |
| `mkt_stl`   | `(invoice_id, seller, buyer, price)` — one per fill from `market_settle_listing`, mirroring `secondary_market`'s `ord_fill`/`lst_buy` |

Co-funding round lifecycle: `cf_open`, `cf_commit`, `cf_cncl`, `cf_exp`, `cf_fin`,
`cf_wthdw`. Collateral/liquidation: `col_dep`, `col_liq`, `col_ret`, `col_topup`,
`col_cfg`. KYC gating: `kyc_req`, `kyc_appr`, `kyc_rej`, `kyc_set`.

`pool` is by far the largest event surface in the workspace — upwards of ninety
distinct actions once every governance parameter setter (`gov_rate`, `gov_fee`,
`gov_util`, `gov_treas`, `gov_yield`, `gov_max_conc`, and dozens more, one per
tunable) and timelocked-operation event (`op_prop`, `op_exec`, `op_cncl`,
`op_delay`) is counted. These aren't tabulated exhaustively here; grep
`contracts/pool/src/lib.rs` for `events().publish` for the full list, or see
`indexer/src/parser.ts` for which of them the indexer currently classifies.

## Credit score (`credit` namespace)

| Action      | Payload                                                          |
| ----------- | ------------------------------------------------------------------- |
| `payment`   | `(caller, sme, invoice_id, status, score, timestamp)`                |
| `funded`    | `(sme, invoice_id, amount, timestamp)` — #534 funding-as-signal event |
| `default`   | `(caller, sme, invoice_id, score, ...)`                              |
| `dispute`   | see `att_disp` below (older alias; both refer to attestation disputes) |
| `resolved`  | see `att_res` below                                                  |
| `lt_upd`    | `days` — late-payment threshold changed                              |
| `hist_upd`  | `max_history` — `set_max_payment_history` changed the ring-buffer cap |
| `score_cfg` | `(old_score_version, new_score_version)`                             |
| `risk_sig`  | `(sme, debtor_concentration_bps, invoice_size_risk_bps)`             |

Attestation events (#868, external credit signals blended into the v2 score):

| Action     | Payload                                        |
| ---------- | ------------------------------------------------- |
| `att_reg`  | `(attestor_address, weight_bps)`                   |
| `att_deact`| `attestor_address`                                 |
| `att_sub`  | `(attestation_id, sme, attestor, score_contribution)` |
| `att_disp` | `(attestation_id, caller)`                         |
| `att_res`  | `(attestation_id, upheld)`                         |

## Governance (`gov` namespace)

| Action    | Payload                                        |
| --------- | --------------------------------------------------- |
| `vote`    | `(proposal_id, voter, in_favor, weight)`             |
| `execute` | `(proposal_id, target_contract)`                     |
| `set_gov` | `(caller, governance_address)`                       |
| `cat_q`   | `(caller, category, quorum_bps)` — per-category quorum override |
| `ac_rot`  | `(access_control, new_access_control)`               |
| `ac_cfg`  | `(access_control, quorum_bps, pass_bps)`             |
| `ac_minbal` | `(access_control, min_share_balance)`              |
| `ac_cat_q`| `(access_control, category, quorum_bps)`             |

## Referral (`REFERRAL` namespace)

| Action     | Payload                              |
| ---------- | ----------------------------------------- |
| `activatd` | `(referee, referrer)`                      |
| `paused`   | `admin`                                    |
| `ac_rot`   | `(access_control, new_access_control)`     |

## Insurance (`INSURNCE` namespace)

| Action    | Payload                                       |
| --------- | -------------------------------------------------- |
| `init`    | `admin`                                             |
| `cfg_set` | `admin`                                             |
| `mcr_set` | `(admin, token, min_ratio_bps)`                     |
| `min_rsv` | `(admin, token, min_amount)`                        |
| `covered` | `(invoice_id, payer, premium, coverage_bps)`        |
| `paused`  | `admin`                                             |

## Access control (`accctl` namespace)

| Action     | Payload                          |
| ---------- | ------------------------------------- |
| `proposed` | `(proposal_id, proposer, target)`      |

This is the satellite contract's own governance-proposal surface; the
individual config-change events it fires *on behalf of* other contracts
(`ac_pause`, `ac_orcl`, `ac_treas`, etc.) are documented under each of those
contracts instead, since that's the namespace they're published under.

## Share (`share` namespace)

The fungible share-token contract backing pool deposits.

| Action      | Payload                          |
| ----------- | ------------------------------------- |
| `paused`    | `admin`                                |
| `unpause`   | `admin`                                |
| `incrallow` | `(owner, spender, new_allowance)`      |
| `decrallow` | `(owner, spender, new_allowance)`      |
| `xfer_from` | `(spender, from, to, amount)`          |

Mint/transfer/burn use the standard SEP-41 token events, not this contract's own
`EVT` namespace — only the extensions above (pause and allowance changes) are
Astera-specific.

## Arbitration (`ARBTRTN` namespace)

| Action     | Payload                                              |
| ---------- | --------------------------------------------------------- |
| `cfg_upd`  | `admin`                                                    |
| `paused`   | `admin`                                                    |
| `jurorsel` | `(case_id, committee_size, retry_count)`                   |
| `noquorum` | `(case_id, retry_count)` — juror selection failed to reach quorum |
| `resolved` | `(case_id, invoice_id, outcome_favor_debtor, synced)`       |
| `slashed`  | `(juror, bps, amount, case_id)`                            |
 main

## Secondary market (`market` namespace)

The `secondary_market` satellite contract (see `contracts/secondary_market`) emits
under its own `market` topic — the indexer classifies these into the `pool` API
category (see `indexer/src/parser.ts`'s `classifyContract`) since it's a satellite
of pool, not a distinct product area.

Fixed-price listings (#1025 — `list_position`/`cancel_listing`/`buy_listing`):

| Action     | Payload                                             |
| ---------- | ---------------------------------------------------- |
| `lst_open` | `(listing_id, invoice_id, seller, amount_or_bps, price)` |
| `lst_cncl` | `(listing_id, invoice_id, seller)`                   |
| `lst_buy`  | `(listing_id, invoice_id, seller, buyer, price)`     |

Limit order book (#1035 — `place_order`/`cancel_order`/`expire_order`), which sits
alongside the fixed-price flow rather than replacing it:

| Action     | Payload                                                                 |
| ---------- | ------------------------------------------------------------------------ |
| `ord_open` | `(order_id, invoice_id, owner, side, amount_or_bps, price)`             |
| `ord_fill` | `(taker_order_id, maker_order_id, invoice_id, buyer, seller, fill_qty, price)` |
| `ord_cncl` | `(order_id, invoice_id, owner)`                                         |
| `ord_exp`  | `(order_id, invoice_id, owner)`                                         |

`ord_fill`'s `price` is the fill's total price for `fill_qty` units (always at the
resting/maker order's per-unit price), not the per-unit `price` carried on
`ord_open`. `pool`'s own `mkt_stl` event (under the `pool` topic, emitted once per
fill from the trusted `market_settle_listing` entrypoint) carries the same trade
as `(invoice_id, seller, buyer, price)`.

## Tranche (`TRANCHE` namespace)

The `tranche` contract (#862) implements invoice tranching (senior/junior) with
waterfall repayment and loss allocation. Events are emitted under the uppercase
`TRANCHE` topic.

| Action     | Payload                                                            |
| ---------- | ------------------------------------------------------------------ |
| `deposit`  | `(investor, token, amount, tranche_class)`                        |
| `withdraw` | `(investor, token, amount, tranche_class)`                        |
| `fund`     | `(invoice_id, token, senior_deployed, junior_deployed)`            |
| `repay`    | `(invoice_id, token, senior_payout, junior_payout)`                |
| `default`  | `(invoice_id, token, junior_loss, senior_loss)`                    |
| `config`   | `(admin, senior_bps, junior_bps, ...)`                             |

## Auction (`auction` namespace)

The `auction` satellite contract (#1036) handles collateral-liquidation Dutch
auctions and oracle-priced risk-response monitoring. It is classified as `pool`
by the indexer (satellite of pool). Events are emitted under the lowercase
`auction` topic.

| Action     | Payload                                                              |
| ---------- | -------------------------------------------------------------------- |
| `col_risk` | `(invoice_id, ratio_bps)`                                            |
| `col_safe` | `(invoice_id, ratio_bps)`                                            |
| `auc_liq`  | `(invoice_id, depositor, token, amount, ...)`                        |
| `risk_cfg` | `(admin, risk_contract, ...)`                                        |
| `sale_open`| `(listing_id, invoice_id, depositor, token, amount, start_price, ...)` |
| `sale_take`| `(listing_id, invoice_id, depositor, buyer, price, ...)`             |
| `sale_exp` | `(listing_id, invoice_id, depositor)`                                |

## Compliance (`COMPLY` namespace)

The `compliance` contract (#867) provides on-chain sanctions screening and
compliance registry. Events are emitted under the uppercase `COMPLY` topic.

| Action     | Payload                                                              |
| ---------- | -------------------------------------------------------------------- |
| `screened` | `(screener, subject, result, ...)`                                   |
| `review`   | `(screener, subject, status, ...)`                                   |
| `scr_prop` | `(proposer, subject, ...)`                                           |
| `scr_reg`  | `(admin, subject, ...)`                                              |
| `scr_del`  | `(admin, subject, ...)`                                              |
| `scr_can`  | `(admin, subject, ...)`                                              |
| `int_set`  | `(admin, interval, ...)`                                             |
| `tl_set`   | `(admin, threshold, ...)`                                            |
| `paused`   | `(admin)`                                                            |
| `unpaused` | `(admin)`                                                            |

## Oracle registry (`ORACLE` namespace)

The `oracle_registry` contract (#861) implements the N-of-M staked oracle
consensus network. Events are emitted under the uppercase `ORACLE` topic.

| Action       | Payload                                                            |
| ------------ | ------------------------------------------------------------------ |
| `registrd`   | `(oracle, ...)`                                                    |
| `dreg_req`   | `(requester, oracle, ...)`                                         |
| `dreg_done`  | `(oracle, ...)`                                                    |
| `slashed`    | `(oracle, amount, ...)`                                            |
| `rnd_open`   | `(round_id, ...)`                                                  |
| `voted`      | `(oracle, round_id, vote, ...)`                                    |
| `consensus`  | `(round_id, result, ...)`                                          |
| `rnd_exp`    | `(round_id, ...)`                                                  |
| `fallback`   | `(round_id, ...)`                                                  |
| `inv_set`    | `(admin, interval, ...)`                                           |
| `cfg_upd`    | `(admin, ...)`                                                     |
| `paused`     | `(admin)`                                                          |
| `unpaused`   | `(admin)`                                                          |

## Indexer migration

Deployed consumers previously received uppercase namespaces (`INVOICE`, `POOL`,
and `CREDIT`). Update all filters and parsers to use the lowercase namespaces
above. During a contract rollout, indexers that must process historical ledgers
should accept both the old and new namespace values; events emitted by a
redeployed contract use only the new form.

The TypeScript event consumers in `frontend/app/history`, invoice detail,
monitoring, and the recent-events feed use the new namespace values.
