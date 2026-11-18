# Off-Chain Services Architecture & Trust Model

Astera relies on two off-chain services — `oracle-service` and
`compliance-service` — to bring information onto the chain that the
contracts themselves have no way to observe: whether an invoice is
genuine, and whether an address is subject to sanctions or exhibits
suspicious activity. This document describes what each service is
trusted to do on-chain, how that trust is bounded, and what happens if
a service is compromised or goes offline.

Related docs: [keeper-monitor.md](keeper-monitor.md) covers the third
off-chain actor (the default keeper), which follows the same
allowlist-plus-narrow-privilege pattern described below.

## Why these services exist

Smart contracts can't fetch a document off IPFS, call a sanctions API, or
watch for behavioral patterns across many transactions — they can only
react to calls made to them. `oracle-service` and `compliance-service`
are the bridges that do that off-chain work and then submit narrow,
specific, signed transactions back to the relevant contract.

Both follow the same shape: an off-chain process holds a Stellar keypair
that is registered on-chain (directly or via a registry contract) with a
role that lets it call a small number of gated functions — never a
general admin key.

## Oracle service: invoice verification

**On-chain role:** either the invoice contract's `oracle` /
`oracle_secondary` address (legacy mode), or one registered voter in the
`oracle_registry` contract (consensus mode, #861).

### What it's allowed to do on-chain

| Mode | Function | Effect |
| --- | --- | --- |
| Legacy (1-of-2) | `invoice.verify_invoice(oracle, id, approved, reason, oracle_hash)` | Unilaterally marks a single invoice verified or rejected. Requires `oracle_hash` to match the invoice's stored `verification_hash`. |
| Consensus (#861) | `oracle_registry.submit_vote(operator, invoice_id, approve)` | Casts one stake-weighted vote in a `VerificationRound`. The registry — not any one oracle — calls back into `invoice.consensus_verify` once votes cross the configured quorum (default two-thirds, `DEFAULT_QUORUM_BPS`). |

In consensus mode a single oracle **cannot** verify or reject an invoice
by itself; it can only move a round's vote tally. Oracles that vote
against the eventual consensus outcome, or that are provably wrong, are
subject to `slash_oracle` (admin- or governance-triggered stake slashing)
and `deregister_oracle`.

Neither mode grants the oracle key any access to pool funds, KYC
records, compliance state, or admin functions — `verify_invoice` and
`submit_vote` are the entire on-chain surface.

### What happens if it's compromised

- **Legacy mode**: a compromised primary oracle key can falsely approve
  or reject individual invoices, since it is a single point of trust by
  design. The admin can call `set_oracle` / `set_secondary_oracle` to
  rotate the compromised key immediately. Damage is limited to invoice
  verification status — the key cannot move funds directly (funding
  still requires the pool contract's own checks) or touch other
  contracts.
- **Consensus mode**: a single compromised oracle key can only cast one
  vote among N; it cannot unilaterally flip a round's outcome unless it
  also controls enough stake-weight to reach quorum alone, which the
  quorum threshold and per-oracle stake caps are intended to make
  impractical. A compromised node's operator should `deregister_oracle`
  (returning its stake) or have it `slash_oracle`'d by governance.

### What happens if it's offline

- **Legacy mode**: invoices simply stay unverified until the oracle (or
  its configured secondary) comes back, or the admin rotates to a new
  oracle address. If `oracle_verified_funding_only` is enabled, affected
  invoices cannot be funded until verification resumes.
- **Consensus mode**: an open `VerificationRound` that never reaches
  quorum expires via `expire_round`; the admin can also force a
  resolution with `admin_resolve_round` as a circuit breaker. One node
  being offline does not block the network as long as enough of the
  remaining oracles are online to reach quorum.

See [`oracle-service/README.md`](../oracle-service/README.md) for
configuration, running multiple nodes, and the registration flow.

## Compliance service: sanctions screening & monitoring

**On-chain role:** a registered "screener" on the `compliance` contract
(`register_screener` / `confirm_screener_registration`, gated by an
admin-configurable timelock — see `set_screener_timelock`).

### What it's allowed to do on-chain

| Function | Caller | Effect |
| --- | --- | --- |
| `submit_screening_result(screener, address, status, reason_code, risk_tier, ...)` | registered screener or admin | Records a `Cleared` / `Flagged` / `Blocked` decision for an address, gating `pool.deposit` (#867) when compliance screening is enabled. |
| `request_review(caller, address, reason)` | registered screener or admin | Off-chain `Monitor` calls this when it observes a suspicious pattern (structuring: many near-threshold deposits in a window; rapid deposit-then-withdraw cycling) — flips the address to pending review without asserting a final verdict. |

A screener key **cannot** clear or block an address without going
through `submit_screening_result`, cannot move pool funds, and cannot
bypass `pool`'s own KYC or concentration checks — it only ever writes to
the `compliance` contract's own state (`ComplianceRecord`,
`ScreeningHistoryEntry`). `is_cleared` / `get_effective_status` are the
read-only functions other contracts (like `pool.deposit`) consult.

The service's `screener.ts` (`MockScreener`) is a placeholder decision
engine for local/dev use — a real deployment would replace it with a
call to an actual sanctions-list provider, without changing the on-chain
trust boundary described here.

### What happens if it's compromised

A compromised screener key can submit false `Flagged`/`Blocked`
decisions (a denial-of-service against legitimate investors) or falsely
`Cleared` a sanctioned address. Because `register_screener` requires
admin approval and (optionally) a timelock before a screener becomes
active, and `deregister_screener` is available immediately, the blast
radius is bounded to compliance decisions — never funds, KYC, or other
contracts' state. The admin should `deregister_screener` the compromised
key and, if any addresses were wrongly cleared or flagged, resubmit
correct `submit_screening_result` calls once a trusted screener is back
online.

### What happens if it's offline

- New addresses are not screened, so `pool.deposit`'s compliance gate
  (`ComplianceNotCleared`) rejects them by default whenever screening is
  enabled — deposits fail closed, not open. Existing `Cleared` addresses
  are unaffected until their next `rescreening_interval` elapses.
- The structuring/rapid-cycle `Monitor` simply stops flagging new
  patterns; nothing on-chain assumes the monitor is continuously
  running, so there is no availability impact on the contracts
  themselves, only a detection gap.

## Common trust-model properties

Both services share the same design intent, which should hold for any
future off-chain integration added to this repo:

1. **Narrow on-chain privilege.** Each service's key can call only the
   specific functions listed above — never a general-purpose admin
   function on any contract.
2. **Admin-reversible.** Every role is revocable by the contract admin
   (`set_oracle`, `deregister_oracle`, `deregister_screener`) without
   requiring the compromised key's cooperation.
3. **Fail closed where it matters.** Funding gates (`oracle_verified_funding_only`,
   the compliance screening gate) default to rejecting when the
   off-chain signal is absent, rather than defaulting to "assume it's
   fine."
4. **No single off-chain party is fully trusted for high-value
   decisions.** The consensus oracle network requires stake-weighted
   quorum instead of one signer; compliance decisions are logged with
   full history (`get_screening_history`) rather than being a single
   mutable flag, so a bad decision is auditable and reversible.
