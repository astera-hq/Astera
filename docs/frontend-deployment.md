# Frontend Deployment Guide

The Astera frontend is a Next.js 14 app that connects to Stellar via the
Freighter wallet. This guide covers deploying it to Vercel (one-click) and to
other static/Node hosts.

## Required environment variables

The frontend reads the following public env vars (prefix `NEXT_PUBLIC_`):

| Variable                          | Description                                  |
| --------------------------------- | -------------------------------------------- |
| `NEXT_PUBLIC_NETWORK`             | `testnet` | `mainnet` | `local`             |
| `NEXT_PUBLIC_INVOICE_CONTRACT_ID` | Invoice contract ID                          |
| `NEXT_PUBLIC_POOL_CONTRACT_ID`    | Pool contract ID                             |
| `NEXT_PUBLIC_USDC_TOKEN_ID`       | USDC token contract ID                       |
| `NEXT_PUBLIC_SECONDARY_MARKET_CONTRACT_ID` | Secondary market contract ID     |

For a full deployment, also set the remaining contract IDs your UI surfaces
(`governance`, `credit_score`, `share`, `access_control`, `tranche`,
`arbitration`, `auction`, `insurance`, `compliance`, `oracle_registry`,
`referral`). Copy `frontend/.env.example` to `.env.local` (or your host's env
config) and fill them in.

## Deploy with Vercel (one-click)

Use the deploy button in the [README](../README.md#frontend-deployment) — it
pre-fills the root directory (`frontend`) and the required env vars. After the
project is created:

1. Open **Settings → Environment Variables** and set the contract IDs above.
2. Set `NEXT_PUBLIC_NETWORK` to `testnet` or `mainnet`.
3. Trigger a redeploy so the new env vars are baked in.

## Deploy to other hosts

The app is a standard Next.js project under `frontend/`:

```bash
cd frontend
cp .env.example .env.local   # fill in contract IDs
npm install
npm run build
npm run start                # or `npm run dev` for local
```

- **Static/Node hosts (Netlify, Render, Fly, Railway):** point the build command
  at `npm run build` in the `frontend` directory and the start command at
  `npm run start`.
- **Docker:** `frontend/Dockerfile` is already wired into `docker-compose.yml`
  for local use; reuse it for container-based hosts.

## After deploying

Verify the app loads and that wallet connection works against your chosen
network. Mismatched `NEXT_PUBLIC_NETWORK` vs. contract IDs is the most common
failure — make sure the contract IDs belong to the same network the frontend
targets.
