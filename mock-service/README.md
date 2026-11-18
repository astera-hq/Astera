# Astera Mock Service

A static mock REST API for frontend-only development, served by
[json-server](https://github.com/typicode/json-server) from `db.json`. With
it, the frontend can render invoices without a running Stellar node or
deployed contracts (#229).

## Running

With Docker Compose (from the repository root):

```bash
docker compose up mock-service
```

Or directly with Node:

```bash
cd mock-service
npx -y json-server@0.17.4 --watch db.json --port 4000
```

Both serve the API on <http://localhost:4000>. `--watch` reloads `db.json`
whenever it changes.

## Pointing the frontend at it

In `frontend/.env.local`:

```bash
NEXT_PUBLIC_USE_MOCK=true
# Optional — this is the default:
NEXT_PUBLIC_MOCK_API_URL=http://localhost:4000
```

With `NEXT_PUBLIC_USE_MOCK=true`, `frontend/lib/contracts.ts` reads invoices
from `GET /invoices/:id` on the mock API instead of simulating the invoice
contract over Soroban RPC. Other contract reads and every write
(transactions) still go to the configured network.

## Data and routes

json-server exposes one REST resource for each top-level key in `db.json`:

| Route                    | Contents                                                                       |
| ------------------------ | ------------------------------------------------------------------------------ |
| `GET /invoices`          | Sample invoices covering each status (`Pending`, `Verified`, `Funded`, ...)    |
| `GET /invoices/:id`      | One invoice by id (what the frontend reads in mock mode)                       |
| `GET /pool`              | Pool configuration and totals                                                  |
| `GET /pool_token_totals` | Share-token supply and share price                                             |
| `GET /investors`         | Investor positions and their invoice commitments (`/investors/:id` by address) |
| `GET /credit_scores`     | SME credit scores (filter with `?sme=G...`)                                    |

Standard json-server features (filtering, sorting, pagination, and
`POST`/`PUT`/`PATCH`/`DELETE` on the resources above) also work, and writes
are persisted back to `db.json` — revert it with `git checkout db.json` if
you do not want to commit the changes.

All amounts are raw integers in the token's smallest unit (7 decimals, so
`10000000000` = 1,000 USDC), and timestamps are Unix seconds, matching the
contracts.

## Limitations

- The data is static: nothing here mirrors contract logic, so funding,
  repayment, and so on will not change the mock state.
- There is no faucet route. The testnet faucet component's "mint test USDC"
  button calls `POST /faucet/usdc`, which this service does not implement, so
  it shows its "faucet unavailable" message; mint test USDC via the CLI
  instead.
