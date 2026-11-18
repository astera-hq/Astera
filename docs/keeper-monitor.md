# Invoice Default Keeper Monitor (#801)

Invoice due-date monitoring and default triggering used to require an admin to
manually call `mark_defaulted()` once an invoice's grace period expired. This
document describes the `keeper` role that lets a restricted, automated
off-chain monitor perform that call safely, and how to run it.

## On-chain keeper role

The invoice contract now recognizes a `keeper` allowlist in addition to the
admin and the pool contract:

| Function | Caller | Effect |
| --- | --- | --- |
| `add_keeper(admin, keeper)` | admin only | Whitelists `keeper` to call `mark_defaulted()`. |
| `remove_keeper(admin, keeper)` | admin only | Revokes a keeper. |
| `list_keepers()` | anyone (read-only) | Returns the current keeper allowlist. |
| `mark_defaulted(id, caller)` | pool **or** a whitelisted keeper | Unchanged default logic; the grace-period check is not affected by who calls it. |

A keeper address has **no other privileges** — it cannot call any other
admin- or pool-gated function. Non-keeper, non-pool callers are rejected with
`InvoiceError::Unauthorized`.

Whitelist a keeper once, from the admin account:

```bash
soroban contract invoke \
  --id "$INVOICE_CONTRACT_ID" \
  --source admin \
  --network testnet \
  -- add_keeper --admin "$ADMIN_ADDRESS" --keeper "$KEEPER_ADDRESS"
```

## Off-chain monitor script

`scripts/monitor_invoices.ts` polls the invoice contract for `Funded`
invoices whose grace period (`due_date + grace_period_days`) has elapsed and
submits `mark_defaulted(id, keeper)` for each, signed by the keeper key.

### Setup

```bash
cd scripts
npm install
```

### Configuration

| Env var | Required | Default | Purpose |
| --- | --- | --- | --- |
| `INVOICE_CONTRACT_ID` | yes | — | Deployed invoice contract id. |
| `KEEPER_SECRET_KEY` | yes | — | Secret key of the whitelisted keeper account. |
| `RPC_URL` | no | `https://soroban-testnet.stellar.org` | Soroban RPC endpoint. |
| `NETWORK_PASSPHRASE` | no | `Test SDF Network ; September 2015` | Network passphrase. |
| `POLL_INTERVAL_MS` | no | `60000` | Delay between sweeps when run continuously. |
| `RUN_ONCE` | no | `false` | Set to `true` to perform a single sweep and exit. |

### Running as a cron job

Run one sweep per invocation and let cron handle scheduling:

```bash
# crontab -e
*/15 * * * * cd /path/to/Astera/scripts && \
  INVOICE_CONTRACT_ID=... KEEPER_SECRET_KEY=... RUN_ONCE=true \
  npx tsx monitor_invoices.ts >> /var/log/astera-keeper.log 2>&1
```

### Running continuously (long-lived process)

Omit `RUN_ONCE` and let the script poll on its own interval — suitable for a
systemd service, Docker container, or a Stellar Turret task runner:

```bash
INVOICE_CONTRACT_ID=... KEEPER_SECRET_KEY=... npm run monitor:invoices
```

### Deploying to a Stellar Turret

The script has no dependency on any specific runner — a Turret task simply
needs to invoke `npx tsx monitor_invoices.ts` with `RUN_ONCE=true` on its own
schedule, using the same environment variables as the cron setup above. The
Turret's signing key must be whitelisted via `add_keeper()` beforehand.

## Security notes

- The keeper key should be a dedicated account funded only for transaction
  fees — it cannot move funds or call any other contract function.
- If a keeper key is compromised, call `remove_keeper()` from the admin
  account immediately; this does not affect the pool contract's ability to
  call `mark_defaulted()`.
