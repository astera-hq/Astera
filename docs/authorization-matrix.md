# Authorization Matrix — Cross-Contract Call Matrix

This document summarizes which contracts (or external actors) are authorized to call specific entry points in other contracts. It is intended for auditors, contributors, and operators to verify access control policies quickly.

## Cross-Contract Call Table

| Caller | Target Contract | Function | Authorization Check |
|--------|-----------------|----------|--------------------|
| Pool | Invoice | `mark_funded(id)` | `require_auth()` — `pool_contract` address stored in Invoice config |
| Pool | Invoice | `mark_paid(id)` | `pool_contract` address check (invoice.owner or pool) |
| Pool | Invoice | `mark_defaulted(id)` | `pool_contract` address check |
| Pool | CreditScore | `record_payment(id, sme, status)` | `pool_contract` address stored in CreditScore config |
| Oracle | Invoice | `mark_verified(id, hash)` | `oracle` address stored in Invoice config |
| Oracle | Invoice | `mark_disputed(id)` | `oracle` address check |
| Admin | All Contracts | `pause()`, `unpause()` | Admin address stored in each contract config; `require_auth(admin)` |
| Admin | Pool | `set_yield()`, `add_token()`, etc. | Admin address check |
| Anyone | Invoice | `get_invoice(id)` | Read-only — no auth required |
| Anyone | Invoice | `check_expiration(id)` | Public keeper — no auth required |


## Verification Checklist (for auditors)

For each row in the table above, verify the following:

- [ ] The authorization check (e.g., `require_auth()` or explicit stored address comparison) is present in the on-chain code for the target function.
- [ ] The authorized address is set and validated during `initialize()` and cannot be changed without admin authorization.
- [ ] There is a unit test demonstrating that an unauthorized caller is rejected for this function.
- [ ] If the call crosses contracts (contract A calling contract B), ensure that the caller's contract address is stored in the callee's configuration and used for the authorization check.


## Code Annotation Guidance

Add a short comment block at the top of each contract file describing its authorized callers. Example:

```rust
// === AUTHORIZED CALLERS ===
// - Admin: pause(), unpause(), admin-only setters
// - Pool contract: mark_funded(), mark_paid(), mark_defaulted()
// - Oracle: mark_verified(), mark_disputed()
// - Anyone: read-only view functions
```

Include this block in:
- `contracts/invoice/src/lib.rs`
- `contracts/pool/src/lib.rs`
- `contracts/credit_score/src/lib.rs`


## Notes and Rationale

- Storing the caller contract address in the callee's initialization (e.g., `invoice.initialize(admin, pool_contract, ...)`) makes authorization checks explicit and auditable.
- Use `require_auth()` for externally-signed admin calls; use `env::invoker()`/`contract` client checks or compare `caller == stored_contract_address` for contract-to-contract calls.
- For sensitive state-changing operations, prefer *both* `require_auth()` and explicit stored-address checks where applicable to protect against accidental misconfiguration.


## Change Log

- 2026-04-29 — Initial draft (issue #273)

