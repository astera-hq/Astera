# Mock Service — Offline Frontend Development

The mock service is a lightweight [json-server](https://github.com/typicode/json-server) instance that serves realistic fixture data for all invoice states, pool states, and user scenarios. It allows frontend contributors to develop and test without a running Stellar node.

## Prerequisites

```bash
npm install -g json-server   # or: pnpm add -g json-server
```

## Starting the Mock Service

```bash
# From the repo root:
cd mock-service
json-server --watch db.json --port 4000
```

The API will be available at `http://localhost:4000`.

## Switching the Frontend to Mock Mode

Set the environment variable before starting the frontend dev server:

```bash
# .env.local (frontend/)
NEXT_PUBLIC_USE_MOCK=true
NEXT_PUBLIC_MOCK_API_URL=http://localhost:4000
```

When `NEXT_PUBLIC_USE_MOCK=true`, the frontend reads data from the mock service instead of making Soroban RPC calls. This lets you work on UI and component logic with no blockchain dependency.

## Available Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /invoices` | All invoices (one per status) |
| `GET /invoices/:id` | Single invoice by ID |
| `GET /pool` | Pool configuration and totals |
| `GET /pool_token_totals` | Share price and USDC totals |
| `GET /investors` | All investor positions |
| `GET /investors/:id` | Single investor |
| `GET /credit_scores` | SME credit scores |

### Filtering (json-server built-in)

```bash
# Invoices by status:
GET /invoices?status=Funded

# Invoices by owner:
GET /invoices?owner=GDEV1…
```

## Mock Data Coverage

`db.json` includes one invoice in each status:

| ID | Status | Debtor |
|----|--------|--------|
| 1 | Pending | Acme Corp |
| 2 | AwaitingVerification | Beta Industries |
| 3 | Verified | Gamma Ltd |
| 4 | Funded (14-day grace override) | Delta Manufacturing |
| 5 | Paid | Epsilon Retail |
| 6 | Defaulted | Zeta Logistics |
| 7 | Disputed | Eta Services |
| 8 | Expired | Theta Wholesale |

Pool state shows partially deployed capital (42% deployed). Two investors are included — one with three commitments (2 active, 1 completed) and one with no commitments. Three SME credit-score profiles cover Excellent (750), Fair (520), and No History (500).

## Adding New Mock Data

Edit `db.json` directly. json-server watches the file and reloads automatically. Follow the shape of existing records. Amount fields use stroops (1 USDC = 10 000 000 stroops).
