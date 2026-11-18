# Astera Compliance Service

Off-chain compliance / sanctions screening service (#867). It screens
addresses against a watchlist provider, submits the result to the on-chain
`compliance` contract via `submit_screening_result`, and watches pool
activity on Horizon for suspicious patterns (structuring, rapid
deposit-then-withdraw cycles), calling `request_review` on-chain when one
fires.

## Components

- **Screener** (`src/screener.ts`) — decides a `ScreenResult` for an address.
  - `MockScreener` (default): a small bundled OFAC-SDN-style fixture list plus
    simple jurisdiction / volume / debtor-concentration heuristics. For local
    development and tests only.
  - `HttpScreenerProvider` (#981): used instead when `SCREENING_PROVIDER_URL`
    is set. POSTs `{ address, name?, jurisdiction?, ... }` to
    `${SCREENING_PROVIDER_URL}/screen` and expects a `ScreenResult` JSON body
    back. Calls are aborted after `SCREENING_PROVIDER_TIMEOUT_MS`; upstream
    errors surface as errors rather than being treated as "Cleared".
- **Monitor** (`src/monitor.ts`) — polls Horizon `/operations` every 5s for
  `invoke_host_function` calls against the pool contract and applies the
  structuring / rapid-cycle heuristics. It also re-screens subjects that were
  screened on-chain through this service once their clearance expires
  (#982). The monitor only starts when both `COMPLIANCE_SCREENER_SECRET_KEY`
  and `COMPLIANCE_CONTRACT_ID` are set; otherwise the REST API still runs and
  screening works offline, but nothing is submitted on-chain.

## Running

```bash
cd compliance-service
npm install
# export the env vars below (or put them in a local .env — dotenv is loaded)
npm run dev            # tsx watch
# or
npm start
```

The service is also part of the root `docker-compose.yml` (`compliance-service`,
port `8081`).

Tests:

```bash
npm test
```

## Configuration

| Env var                          | Required        | Default                               | Description                                                                                                    |
| -------------------------------- | --------------- | ------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `RPC_URL`                        | no              | `https://soroban-testnet.stellar.org` | Soroban RPC endpoint                                                                                           |
| `HORIZON_URL`                    | no              | `https://horizon-testnet.stellar.org` | Horizon endpoint polled by the monitor                                                                         |
| `NETWORK_PASSPHRASE`             | no              | `Test SDF Network ; September 2015`   | Network passphrase                                                                                             |
| `COMPLIANCE_SCREENER_SECRET_KEY` | for on-chain    | falls back to `ORACLE_SECRET_KEY`     | Secret key (`S...`) of an account registered as a screener on the compliance contract. Validated at startup.   |
| `COMPLIANCE_CONTRACT_ID`         | for on-chain    | —                                     | Compliance contract address. Required for `submitOnChain`, the monitor and re-screening.                       |
| `POOL_CONTRACT_ID`               | no              | —                                     | Pool contract whose deposits / withdrawals the monitor watches                                                 |
| `INVOICE_CONTRACT_ID`            | no              | —                                     | Invoice contract address                                                                                       |
| `HEALTH_PORT`                    | no              | `8081`                                | Port for the HTTP API (including `/health`)                                                                    |
| `COMPLIANCE_ADMIN_TOKEN`         | **yes in prod** | — (API open)                          | Shared secret for the admin endpoints — see [Admin authentication](#admin-authentication)                      |
| `STRUCTURING_THRESHOLD`          | no              | `10000000000`                         | Deposit amount (raw token units) below which a deposit counts as "sub-threshold" for structuring detection     |
| `STRUCTURING_WINDOW_MS`          | no              | `3600000` (1h)                        | Sliding window for counting sub-threshold deposits                                                             |
| `STRUCTURING_MAX_COUNT`          | no              | `5`                                   | Number of sub-threshold deposits in the window that triggers a `structuring` alert                             |
| `SCREENING_PROVIDER_URL`         | no              | — (use `MockScreener`)                | Base URL of a real upstream screening provider                                                                 |
| `SCREENING_PROVIDER_API_KEY`     | no              | —                                     | Sent to the provider as `Authorization: Bearer <key>`                                                          |
| `SCREENING_PROVIDER_TIMEOUT_MS`  | no              | `5000`                                | Hard timeout on upstream provider calls                                                                        |
| `RESCREEN_CHECK_INTERVAL_MS`     | no              | `3600000` (1h)                        | How often the monitor checks tracked subjects against `get_rescreening_interval` / their record's `expires_at` |

## Admin authentication

Every endpoint except `GET /health` is an admin endpoint.

- When `COMPLIANCE_ADMIN_TOKEN` is **unset**, admin endpoints are **open**.
  This is intended for local development only — always set it in any shared
  or production deployment.
- When it is set, a request must carry the token in one of:
  - `X-Admin-Token: <token>`, or
  - `Authorization: Bearer <token>` (a bare `Authorization: <token>` is also
    accepted).

  If both headers are present, `X-Admin-Token` is the one checked. Tokens
  are compared in constant time. A missing or wrong token returns
  `401 {"error":"unauthorized"}`.

## REST API

All responses are JSON. Unknown routes return `404 {"status":"not_found"}`;
unexpected failures (including malformed JSON bodies and on-chain submission
errors) return `500 {"error":"<message>"}`.

### `GET /health`

Unauthenticated liveness probe (used by the Docker healthcheck).

```json
{
  "status": "ok",
  "processed": 1234,
  "complianceContractId": "C...",
  "monitor": true
}
```

`processed` is the number of Horizon operations the monitor has seen;
`complianceContractId` is `null` when unset; `monitor` is whether the monitor
is running.

### `GET /flags` (admin)

Returns the most recent (up to 100) monitor alerts, oldest first.

```json
{
  "alerts": [
    {
      "id": "1760000000000-GABCDEFG",
      "address": "GABC...",
      "reason": "structuring: 5 sub-threshold deposits in window",
      "at": "2026-01-01T00:00:00.000Z",
      "pattern": "structuring"
    }
  ]
}
```

`pattern` is `structuring` or `rapid_cycle`. Alerts are held in memory and
are lost on restart.

### `POST /screen` (admin)

Screens an address and optionally submits the result on-chain.

Request body:

| Field           | Type    | Description                                                                                                            |
| --------------- | ------- | ---------------------------------------------------------------------------------------------------------------------- |
| `address`       | string  | **Required.** Address to screen                                                                                        |
| `name`          | string  | Optional context passed to the screener                                                                                |
| `jurisdiction`  | string  | Optional context passed to the screener                                                                                |
| `submitOnChain` | boolean | When `true`, call `submit_screening_result` on the compliance contract and start tracking the address for re-screening |
| `status`        | string  | Operator override of the decision. One of `Unscreened`, `Cleared`, `Flagged`, `Blocked`, `PendingReview`               |
| `riskTier`      | string  | Override of the risk tier (applied together with `status`). One of `Low`, `Medium`, `High`                             |
| `reasonCode`    | number  | Override of the reason code (applied together with `status`). Integer in the `u32` range                               |
| `notesHash`     | string  | Notes / evidence hash stored with the record (truncated to 64 chars on-chain)                                          |

The screener always runs first. If `status` is supplied, it replaces the
screener's decision, and `riskTier` / `reasonCode` / `notesHash` replace the
screener's values when present.

Override fields are validated against the compliance contract's
`ComplianceStatus` / `RiskTier` enums before anything is submitted; an
invalid value returns `400` (#1556), e.g.

```json
{
  "error": "invalid status: must be one of Unscreened, Cleared, Flagged, Blocked, PendingReview"
}
```

A missing `address` returns `400 {"error":"address required"}`.

Response:

```json
{
  "result": {
    "address": "GABC...",
    "status": "Cleared",
    "reasonCode": 0,
    "riskTier": "Low",
    "notes": "No sanctions hit"
  },
  "txHash": "abc123..."
}
```

`txHash` is only present when `submitOnChain` was `true`. Submitted records
expire 180 days after submission.

### `POST /monitor/deposit` (admin)

Feeds a deposit into the structuring heuristic manually (e.g. from another
service or for testing), in addition to what the monitor observes on Horizon.

```json
{ "address": "GABC...", "amount": "5000000000" }
```

`amount` is a raw token amount as a decimal string (parsed as a `BigInt`).
Both fields are required (`400` otherwise). Returns `{"ok":true}`. This is a
no-op when the monitor is disabled.

## Reason codes (`MockScreener`)

| Code   | Meaning                                       |
| ------ | --------------------------------------------- |
| `0`    | Clear — no sanctions hit                      |
| `9001` | Sanctions list hit (fixture or runtime block) |
| `2001` | High-risk jurisdiction                        |
| `2002` | Elevated recent transaction volume            |
| `2003` | High debtor concentration                     |
