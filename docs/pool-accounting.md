# Pool accounting model and invariants

This document describes the accounting conventions implemented by `contracts/pool/src/lib.rs`. Amounts are tracked per accepted token, but the pool normalizes value-bearing amounts into the protocol's USDC-equivalent accounting unit before applying share and concentration math. It is not safe to compare raw token amounts across different decimal precisions or exchange rates.

## Units and state

| Field | Unit / meaning | Important distinction |
|---|---|---|
| `PoolTokenTotals.pool_value` | USDC-equivalent smallest accounting units (`i128`) | Investor-backed net asset value represented by the pool's share supply. Includes deployed receivables while they remain recognized as assets, not just the token balance held by the contract. |
| `PoolTokenTotals.total_deployed` | USDC-equivalent principal outstanding | Subset of `pool_value` committed to ordinary pool-funded invoices. It is not a second asset to add to `pool_value`. |
| `PoolTokenTotals.reward_per_share` | Accumulator scaled by `REWARD_PRECISION = 10^12` | Cumulative interest entitlement per share; it is not a token balance or a USDC amount by itself. |
| `PoolTokenTotals.protocol_revenue` | USDC-equivalent protocol fee reserve | Separate from investor NAV and unavailable to investor principal withdrawals. Used for treasury withdrawals and capped loyalty bonuses. |
| `InvestorPosition.deposited` | Cumulative USDC-equivalent value deposited | Used by the investor concentration guard; it is not the current share balance and is not reduced on each withdrawal. |
| `InvestorPosition.available` | Pool-share units (`i128`) | Shares available to fund an invoice or withdraw. Queued withdrawals reserve shares by reducing this value. |
| `InvestorPosition.deployed` | Pool-share units (`i128`) | Share-denominated investor exposure deployed into ordinary invoice positions; secondary-market settlement transfers this exposure between positions. |
| `InvestorPosition.earned` | Accumulated reward amount as stored by the position implementation | Do not use it as a substitute for the reward-per-share accumulator when computing a claim. |

The token amount transferred on deposit/withdrawal is converted using the token's configured decimals and exchange rate. The default exchange rate is 10,000 basis points (1:1). Share minting uses the USDC-equivalent deposit value and current share supply: when supply or NAV is zero, shares are initialized 1:1; otherwise shares minted are `deposit_value * total_shares / pool_value`. Withdrawal performs the inverse valuation and converts the USDC-equivalent amount back to the token's units.

## How the main totals move

- **Deposit** (`deposit`): increases `pool_value` by normalized deposit value, mints shares, increases `InvestorPosition.deposited` by normalized value and `available` by minted shares. The token transfer is the final interaction.
- **Immediate withdrawal / queued withdrawal settlement** (`withdraw`, `process_immediate_withdrawal`, `process_withdrawal_queue`): burns shares, reduces `available`, and decreases `pool_value` by the USDC-equivalent value paid. A queued request moves shares out of `available`; it does not reduce NAV until settlement.
- **Ordinary invoice funding** (`fund_invoice` / internal funding path): moves investor shares from available to deployed and increases `total_deployed` by the funded principal. Funding sends the underlying token to the SME, while the pool continues to recognize the receivable in `pool_value`.
- **Ordinary invoice repayment** (`repay_invoice` / `repay_invoice_request`): partial payments update the funded-invoice record and transfer incoming tokens to the pool. On full repayment, principal is removed from `total_deployed`, interest is added to `pool_value`, factoring fees are added to both `total_fee_revenue` and `protocol_revenue`, and the interest reward is added to `reward_per_share` using the share supply and 10^12 precision.
- **Yield claim** (`claim_yield`): computes the investor's unclaimed reward from the difference between current `reward_per_share` and their saved snapshot, multiplied by their current share-token balance and divided by 10^12. Any loyalty bonus is capped by `protocol_revenue`; a successful transfer advances the snapshot and subtracts the bonus reserve.
- **Default / collateral seizure** (`liquidate_invoice` and `risk_liquidate_collateral`): writes off defaulted principal from both `pool_value` and `total_deployed` in the loan token's accounting, then credits recovered collateral into `pool_value` for the collateral token.
- **Insurance recovery** (`receive_insurance_payout`): increases the relevant token's `pool_value` by the positive payout amount. It does not change `total_deployed`.
- **Treasury withdrawal / referral rewards**: decrease `protocol_revenue` when reserve funds leave the pool; they must not be represented as investor NAV.

## Co-funding accounting ("bypass")

Multi-investor co-funding is intentionally not the ordinary pool-funded path. `commit_to_invoice` burns each participant's pool shares and reduces `pool_value` by the normalized committed amount while recording each participant's committed amount and ownership basis points. The committed token balance remains in the pool contract earmarked for that round. When `finalize_co_funding` fills the round, the contract creates the funded-invoice record and transfers the committed principal to the SME.

Because the commitment already left `pool_value` at commit time and was never added to `total_deployed`, finalization deliberately does not change either total. Adding principal to `total_deployed` at finalization would double-count a deployment; adding it back to `pool_value` would create phantom NAV. Repayments for a co-funded invoice use the pari-passu distribution path rather than the ordinary pool-wide reward-per-share path. Expired/cancelled rounds refund participants and restore the relevant NAV/shares through the refund helper.

## Exchange-rate normalization

The pool stores a per-token exchange rate in basis points, defaulting to 10,000. Deposit converts the received token amount into USDC-equivalent value before increasing NAV and minting shares. Withdrawal first values shares in USDC-equivalent units, then divides by the configured exchange rate to obtain the token transfer amount. Co-funding similarly converts the committed token amount to USDC-equivalent value before calculating shares to burn and reducing NAV.

Consequently:
- `pool_value`, `total_deployed`, and `deposited` are comparable normalized values;
- `available` and `deployed` are share counts, not raw token amounts;
- token transfers and invoice principal fields remain denominated in the invoice's selected token;
- rounding is integer truncation, so invariants should allow bounded dust rather than assume perfect reversibility.

## Solvency and regression invariants

Tests should assert these after every successful state transition (and assert no state change after a rejected transition):

1. **Non-negative accounting:** `pool_value >= 0`, `total_deployed >= 0`, and `protocol_revenue >= 0`; also `total_deployed <= pool_value` for ordinary pool-funded exposures, subject to explicitly modeled co-funding states and loss transitions.
2. **Liquidity bound:** immediately withdrawable USDC-equivalent liquidity is `pool_value - total_deployed`; a withdrawal must not burn shares or reduce NAV if that liquidity is insufficient.
3. **Share conservation:** total investor share balances equal share-token total supply; deposit mints exactly the calculated shares, withdrawal burns exactly the shares valued, and queued shares remain reserved until cancelled or settled.
4. **Position consistency:** `available` and `deployed` are share-denominated; funding moves the relevant share exposure from available to deployed, repayment/settlement releases it, and secondary-market settlement transfers it between seller and buyer without changing global share supply.
5. **NAV reconciliation:** for each token, reconcile NAV changes to normalized deposits, withdrawals, interest, insurance recoveries, collateral recoveries, and realized write-offs. Never count deployed principal twice.
6. **Revenue isolation:** protocol fees increase `protocol_revenue` independently of investor NAV; treasury/referral/loyalty-bonus outflows reduce the reserve and cannot make it negative.
7. **Reward snapshot safety:** a failed reward transfer must not advance the investor snapshot or consume the protocol reserve; successful claims cannot exceed accrued rewards plus a funded bonus.
8. **Co-funding conservation:** commitments reduce NAV once; successful finalization transfers the escrowed principal without increasing NAV/deployed totals again; expiry/cancellation refunds exactly the recorded committed amounts, within integer-rounding rules.
9. **Cross-token correctness:** conversion uses the configured rate and decimal normalization consistently in deposits, withdrawals, and co-funding. Tests should cover non-default decimals/rates and round-trip truncation.
10. **Atomic failure behavior:** failed token/share/cross-contract interactions must leave the ledger transaction rolled back; specifically test that insufficient pool token balance cannot result in burned shares with no payout.

These are accounting expectations for tests and reviewers, not a substitute for auditing every mutation. When adding an entrypoint, update this document and add a test that exercises both its successful accounting delta and its rejection/rollback path.
