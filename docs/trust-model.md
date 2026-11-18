# Cross-contract trust model and threat model

This document records the pool's contract-to-contract trust boundaries visible in the current implementation. It is intended as an audit map: a configured address is an authority boundary, not proof that the target's implementation or administrator is safe.

## Trust-boundary inventory

| Edge / configured dependency | Pool entrypoints and direction | What the pool verifies | Assumption and compromise impact |
|---|---|---|---|
| Invoice contract (pool calls invoice) | `fund_invoice` checks `get_authorized_pool`; `liquidate_invoice` checks `is_invoice_defaulted`; other invoice reads support lifecycle decisions | The configured invoice address is used; funding rejects unless its authorized pool equals this pool. Default checks fail closed when the call errors. | The pool relies on invoice status and authorization answers. A malicious/compromised invoice implementation can misreport authorization/status and affect funding or collateral seizure. |
| Share token (pool calls share) | Deposit mint; withdrawal burn; share balance and supply reads; reward calculations | Address must be configured for the accepted token; calls execute as contract invocations. | Share balances/supply are foundational to NAV ownership and reward allocation. A malicious share token can mint/burn incorrectly or lie about balances/supply, corrupting ownership and enabling loss or denial of withdrawals. |
| Secondary market (market calls pool) | `market_settle_listing` | Requires caller auth and exact match with `SECONDARY_MARKET_CONTRACT`; checks buyer/seller compliance and KYC, self-trade, buyer available funds, seller position/listing balance, and transfer limits. | The pool trusts the configured market to authorize the settlement intent and provide the settlement payload. Compromise can trigger unauthorized position/share transfers or manipulate market settlement, bounded by the pool's own balance, compliance, KYC, and position checks. |
| Auction / risk-response contract (risk contract calls pool) | `risk_liquidate_collateral` | Requires caller auth and exact match with configured `RISK_CONTRACT`; rejects missing/settled collateral and applies pause/reentrancy checks. | The pool explicitly trusts the risk contract's liquidation decision; it does not independently re-run the risk engine's eligibility decision here. Compromise can seize eligible unsettled collateral prematurely and route recovered collateral through the pool's accounting/transfer path. |
| Insurance contract (insurance calls pool) | `receive_insurance_payout` | Requires caller auth, exact match with `INSURANCE_CFG`, and a positive amount. | The pool treats the configured insurance contract as the source of valid payout claims. A compromised insurance contract can credit fictitious recoveries to NAV (without itself proving an external token transfer in this entrypoint), creating accounting insolvency or false investor value. |
| Oracle registry / price source (pool reads oracle data) | Collateral valuation and risk-related price reads | Pool-side price retrieval applies the configured feed and staleness/availability checks; cross-asset collateral valuation rejects when a usable price is unavailable. | Correctness depends on feed configuration, oracle quorum/administration, and price freshness. A compromised feed/admin can distort collateral valuations, causing under-collateralized funding or incorrect risk decisions. |
| Governance contract (governance calls pool) | `*_via_governance` configuration entrypoints | Governance-gated setters require the configured governance address and typed/validated parameter values. | The configured governance contract is a privileged principal. A malicious governance implementation or captured voting/admin process can change risk limits, fees, rates, collateral parameters, and other supported settings; blast radius is protocol-wide. |
| Access-control contract (pool reads/uses roles) | `*_via_ac` admin/configuration entrypoints | Access-control-gated paths delegate role membership/authorization to the configured access-control contract and reject when it is absent or invalid. | A compromised access-control contract or its role administrators can grant privileged roles or deny legitimate operators, affecting all operations protected by those roles. |

## Administrative trust roots

- The pool's bootstrap/admin identity can configure trusted contract addresses, accepted tokens, share-token mappings, oracle settings, and protocol parameters. A compromised admin can replace a trust anchor with an attacker-controlled contract.
- Several high-impact configuration changes are routed through proposal/timelock or governance entrypoints; the exact path varies by setting. A timelock reduces immediate key-compromise risk but does not make a malicious approved change safe.
- The risk-contract setter is explicitly a direct admin operation and is described in code as a trust tier similar to the oracle setter. Insurance and secondary-market addresses are also trust anchors.
- Contract upgrade authority is a separate trust root: a valid upgrade can replace code while retaining the contract address and its downstream trust relationships. Review upgrade proposals, wasm hashes, timelocks, and authorization independently.
- Operators, keepers, relayers, and frontends are not inherently trusted merely because they submit transactions. Contract authorization and state validation must remain authoritative on-chain.

## Threat scenarios and required controls

1. **Trusted-address substitution:** compromise an admin/governance/access-control authority, replace a trusted contract address, then invoke the pool through the new contract. Mitigate with multi-party control, delayed changes, emitted configuration events, address allowlists where appropriate, and monitoring of every trust-anchor update.
2. **False settlement or liquidation:** compromise the market or risk satellite and submit a syntactically valid but economically malicious action. Keep independent pool-side balance, ownership, status, compliance, KYC, pause, and replay checks; add adversarial integration tests for each boundary.
3. **False recovery:** compromise insurance and report a payout that is not backed by an actual token transfer. Reconcile payout events against token transfers and require the insurance workflow to transfer funds atomically or provide verifiable evidence before NAV is credited.
4. **Oracle manipulation / stale data:** compromise an oracle reporter, registry administrator, or quorum. Enforce feed authorization, quorum, staleness, sane price bounds, and conservative behavior when the feed is unavailable.
5. **Governance capture / role escalation:** compromise governance voters, proposer controls, or access-control administrators. Require bounded parameter validation, timelocks, quorum, monitoring, and independent review for changes affecting solvency or privileged callers.
6. **Dependency reentrancy or malicious return values:** a configured dependency may re-enter or return inconsistent data. Preserve the pool's non-reentrancy guard around value-moving paths, check return values, and test rollback behavior with malicious mock contracts.

## Audit checklist

- Enumerate every configured address and identify who can set or rotate it.
- For every cross-contract call, record caller authorization, target authorization, argument validation, return-value handling, and whether errors fail open or closed.
- Confirm each privileged setter's direct-admin, access-control, governance, or timelock path from source rather than assuming all setters share one policy.
- Test compromised dependencies with mocks: unauthorized callers, false return values, stale prices, duplicate/replayed settlement, reentrancy, and failed token transfers.
- Reconcile emitted events and token transfers with all corresponding accounting mutations.
- Revisit this document whenever a new contract client, trusted address, or `*_via_governance` / `*_via_ac` entrypoint is added.

This is a source-oriented threat-model baseline, not a formal verification result or an independent security audit.
