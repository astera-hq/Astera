# Event Reference

All contract events follow a consistent schema:

- **Topics**: `[Symbol("CONTRACT_NAME"), Symbol("action_name")]`
- **Data**: `[field1, field2, ..., ledger_timestamp, actor_address?]`

---

## Invoice Contract (`INVOICE`)

| Event | Topics | Data Fields | When |
|-------|--------|-------------|------|
| `created` | `["INVOICE", "created"]` | `(id: u64, owner: Address, amount: i128, metadata_uri: Option<String>, timestamp: u64)` | SME mints a new invoice |
| `funded` | `["INVOICE", "funded"]` | `(id: u64, pool: Address, timestamp: u64)` | Pool marks invoice as funded |
| `paid` | `["INVOICE", "paid"]` | `(id: u64, timestamp: u64)` | Invoice fully repaid |
| `defaulted` | `["INVOICE", "defaulted"]` | `(id: u64, timestamp: u64)` | Invoice marked defaulted |
| `verified` | `["INVOICE", "verified"]` | `(id: u64, oracle_hash: String, timestamp: u64)` | Oracle approves invoice |
| `disputed` | `["INVOICE", "disputed"]` | `(id: u64, timestamp: u64)` | Oracle rejects / dispute raised |
| `paused` | `["INVOICE", "paused"]` | `(admin: Address, timestamp: u64)` | Admin pauses contract |
| `unpaused` | `["INVOICE", "unpaused"]` | `(admin: Address, timestamp: u64)` | Admin unpauses contract |
| `oracle_updated` | `["INVOICE", "oracle_updated"]` | `(admin: Address, old_oracle: Option<Address>, new_oracle: Address)` | Admin changes the oracle address (#347) |
| `daily_limit_updated` | `["INVOICE", "daily_limit_updated"]` | `(admin: Address, old_limit: u32, new_limit: u32)` | Admin changes the daily invoice limit (#347) |
| `max_amount_updated` | `["INVOICE", "max_amount_updated"]` | `(admin: Address, old_max: i128, new_max: i128)` | Admin changes the max invoice amount (#347) |
| `expiration_updated` | `["INVOICE", "expiration_updated"]` | `(admin: Address, old_secs: u64, new_secs: u64)` | Admin changes the invoice expiration duration (#347) |
| `grace_period_updated` | `["INVOICE", "grace_period_updated"]` | `(admin: Address, old_days: u32, new_days: u32)` | Admin changes the global grace period (#347) |
| `timelock_updated` | `["INVOICE", "timelock_updated"]` | `(admin: Address, old_secs: u64, new_secs: u64)` | Admin changes the upgrade timelock (#338) |
| `upg_prop` | `["INVOICE", "upg_prop"]` | `(admin: Address, earliest_execute_at: u64)` | Admin proposes a WASM upgrade (#338/#340) |
| `upgraded` | `["INVOICE", "upgraded"]` | `(admin: Address, timestamp: u64)` | WASM upgrade executed |

---

## Pool Contract (`POOL`)

| Event | Topics | Data Fields | When |
|-------|--------|-------------|------|
| `deposit` | `["POOL", "deposit"]` | `(investor: Address, amount: i128, shares: i128, timestamp: u64)` | Investor deposits stablecoin |
| `withdraw` | `["POOL", "withdraw"]` | `(investor: Address, amount: i128, shares: i128, timestamp: u64)` | Investor withdraws |
| `funded` | `["POOL", "funded"]` | `(invoice_id: u64, sme: Address, principal: i128, token: Address, timestamp: u64)` | Invoice funded from pool |
| `repaid` | `["POOL", "repaid"]` | `(invoice_id: u64, principal: i128, interest: i128, timestamp: u64)` | Invoice fully repaid |
| `part_pay` | `["POOL", "part_pay"]` | `(invoice_id: u64, amount: i128, total_repaid: i128, timestamp: u64)` | Partial repayment received |
| `high_util` | `["POOL", "high_util"]` | `(token: Address, utilization_bps: u32, timestamp: u64)` | Utilization exceeds warning threshold (#275) |
| `paused` | `["POOL", "paused"]` | `(admin: Address, timestamp: u64)` | Admin pauses pool |
| `unpaused` | `["POOL", "unpaused"]` | `(admin: Address, timestamp: u64)` | Admin unpauses pool |
| `add_token` | `["POOL", "add_token"]` | `(admin: Address, token: Address, timestamp: u64)` | New stablecoin whitelisted |
| `rm_token` | `["POOL", "rm_token"]` | `(admin: Address, token: Address, timestamp: u64)` | Stablecoin removed |
| `yield_prop` | `["POOL", "yield_prop"]` | `(admin: Address, current_bps: u32, proposed_bps: u32, effective_at: u64, timestamp: u64)` | Yield change proposed |
| `yield_chg` | `["POOL", "yield_chg"]` | `(old_bps: u32, new_bps: u32, timestamp: u64)` | Yield rate updated |
| `col_dep` | `["POOL", "col_dep"]` | `(invoice_id: u64, depositor: Address, token: Address, amount: i128, timestamp: u64)` | Collateral deposited |
| `col_ret` | `["POOL", "col_ret"]` | `(invoice_id: u64, depositor: Address, amount: i128, timestamp: u64)` | Collateral returned on repayment |
| `col_seiz` | `["POOL", "col_seiz"]` | `(invoice_id: u64, depositor: Address, amount: i128, timestamp: u64)` | Collateral seized on default |
| `set_util` | `["POOL", "set_util"]` | `(admin: Address, bps: u32, timestamp: u64)` | Max utilization threshold updated (#275) |
| `set_uwarn` | `["POOL", "set_uwarn"]` | `(admin: Address, bps: u32, timestamp: u64)` | Utilization warning threshold updated (#275) |
| `kyc_appr` | `["POOL", "kyc_appr"]` | `(admin: Address, investor: Address)` | Investor KYC explicitly approved (#337) |
| `kyc_rej` | `["POOL", "kyc_rej"]` | `(admin: Address, investor: Address)` | Investor KYC explicitly rejected (#337) |
| `timelock_updated` | `["POOL", "timelock_updated"]` | `(admin: Address, old_secs: u64, new_secs: u64)` | Upgrade timelock changed (#338) |
| `upg_prop` | `["POOL", "upg_prop"]` | `(admin: Address, earliest_execute_at: u64)` | Admin proposes a WASM upgrade (#338/#340) |
| `upgraded` | `["POOL", "upgraded"]` | `(admin: Address, timestamp: u64)` | WASM upgrade executed |

---

## Credit Score Contract (`CREDIT`)

| Event | Topics | Data Fields | When |
|-------|--------|-------------|------|
| `payment` | `["CREDIT", "payment"]` | `(sme: Address, invoice_id: u64, status: PaymentStatus, score: u32, timestamp: u64)` | Payment recorded and score updated |
| `default` | `["CREDIT", "default"]` | `(sme: Address, invoice_id: u64, score: u32, timestamp: u64)` | Default recorded and score updated |
| `paused` | `["CREDIT", "paused"]` | `(admin: Address, timestamp: u64)` | Admin pauses contract |
| `unpaused` | `["CREDIT", "unpaused"]` | `(admin: Address, timestamp: u64)` | Admin unpauses contract |
| `timelock_updated` | `["CREDIT", "timelock_updated"]` | `(admin: Address, old_secs: u64, new_secs: u64)` | Upgrade timelock changed (#338) |
| `upg_prop` | `["CREDIT", "upg_prop"]` | `(admin: Address, earliest_execute_at: u64)` | Admin proposes a WASM upgrade (#338/#340) |
| `upgraded` | `["CREDIT", "upgraded"]` | `(admin: Address, timestamp: u64)` | WASM upgrade executed |

---

## Parsing Events (Frontend)

Use `monitoring.ts` to parse events. The consistent topic structure means:

```ts
const [contractName, actionName] = event.topic; // e.g. ["POOL", "deposit"]
const data = event.value;                        // array of data fields
```

All events include `timestamp` as the last data field for correlation with ledger time.
