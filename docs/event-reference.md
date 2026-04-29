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

---

## Credit Score Contract (`CREDIT`)

| Event | Topics | Data Fields | When |
|-------|--------|-------------|------|
| `payment` | `["CREDIT", "payment"]` | `(sme: Address, invoice_id: u64, status: PaymentStatus, score: u32, timestamp: u64)` | Payment recorded and score updated |
| `default` | `["CREDIT", "default"]` | `(sme: Address, invoice_id: u64, score: u32, timestamp: u64)` | Default recorded and score updated |
| `paused` | `["CREDIT", "paused"]` | `(admin: Address, timestamp: u64)` | Admin pauses contract |
| `unpaused` | `["CREDIT", "unpaused"]` | `(admin: Address, timestamp: u64)` | Admin unpauses contract |

---

## Parsing Events (Frontend)

Use `monitoring.ts` to parse events. The consistent topic structure means:

```ts
const [contractName, actionName] = event.topic; // e.g. ["POOL", "deposit"]
const data = event.value;                        // array of data fields
```

All events include `timestamp` as the last data field for correlation with ledger time.
