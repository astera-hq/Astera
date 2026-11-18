# Local Development with Docker Compose

This guide walks you through running the entire Astera stack locally with a single
command. It is the fastest way to get the contracts, frontend, indexer, oracle,
and compliance services running together against a local Stellar network.

## Prerequisites

- [Docker](https://docs.docker.com/get-docker/) (Docker Engine 20.10+ with Compose v2)
- [Git](https://git-scm.com/)

That's it. Docker Compose builds the Rust contracts, the Next.js frontend, the
indexer, and the off-chain services for you — you do **not** need Rust, Node, or
the Stellar CLI installed locally to run the stack.

## Start the stack

From the repository root:

```bash
docker compose up --build
```

This starts:

| Service            | URL / Port                              | Purpose                                        |
| ------------------ | --------------------------------------- | ---------------------------------------------- |
| `stellar`          | http://localhost:8000                   | Local Stellar quickstart network (RPC+Horizon) |
| `contracts`        | —                                       | Builds & deploys all contracts, writes IDs     |
| `frontend`         | http://localhost:3000                   | Next.js 14 app (Freighter wallet)              |
| `mock-service`     | http://localhost:4000                   | Mock REST API (json-server)                    |
| `indexer-postgres` | localhost:5433                          | Postgres for the off-chain indexer             |
| `indexer`          | http://localhost:3001                   | Event indexer + read API                       |
| `oracle-service`   | —                                       | Invoice verification oracle                    |
| `compliance-service` | —                                     | Compliance screening service                   |

The `stellar` container must be healthy before contracts deploy, and contracts
must finish deploying before the frontend starts. Compose encodes these
dependencies, so a plain `docker compose up` brings everything up in order.

## What gets deployed

The `contracts` service runs `scripts/deploy-local.sh`, which builds all 14
contracts and writes their IDs to a shared `/contract-ids/env` volume that the
frontend and services consume. The deployed contracts are:

`access_control`, `arbitration`, `auction`, `compliance`, `credit_score`,
`governance`, `insurance`, `invoice`, `oracle_registry`, `pool`, `referral`,
`secondary_market`, `share`, and `tranche`.

## Common tasks

**Reset everything (including chain state):**

```bash
docker compose down -v
docker compose up --build
```

**Rebuild only the contracts after a Rust change:**

```bash
docker compose up --build contracts
```

**View contract logs:**

```bash
docker compose logs -f contracts
```

**Frontend environment file:** the local frontend reads contract IDs from the
shared volume automatically. If you deploy manually (outside Compose), copy
`frontend/.env.example` to `frontend/.env.local` and fill in the contract IDs.

## Manual contract workflow (without Docker)

If you prefer to build and deploy contracts yourself, see
[Testnet Deployment Guide](deployment.md) and
[Smart Contract Interaction Guide](interacting-with-contracts.md). The local
commands are identical except the network is `Standalone Network ; February 2017`
with RPC `http://localhost:8000/soroban/rpc`.

## Troubleshooting

- **Frontend shows empty contract IDs:** the `contracts` container hasn't
  finished deploying yet. Wait for `docker compose logs contracts` to print
  `Contract IDs written to ...`.
- **`stellar` container unhealthy:** first run can take up to a minute to
  initialize. Give it time; Compose retries 20 times.
- **Port already in use:** stop whatever is bound to 8000/3000/4000/3001/5433,
  or edit the port mappings in `docker-compose.yml`.
