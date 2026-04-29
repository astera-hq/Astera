# Local Development with Docker Compose

This guide documents the current `docker-compose.yml` workflow for Astera. The
commands below use Docker Compose v2 syntax and are suitable for macOS, Linux,
and WSL2. If you are on Windows, run them from WSL2 and keep the repository in
the Linux filesystem rather than under `/mnt/c/...`.

## 1. Prerequisites

Install the following before starting the stack:

- Docker Desktop, or Docker Engine, with Docker Compose v2 enabled
- Node.js 20+
- Rust 1.76+
- Freighter browser extension for signing frontend transactions

Recommended checks:

```bash
docker --version
docker compose version
node --version
rustc --version
```

Notes:

- Use `docker compose`, not the legacy `docker-compose` v1 binary.
- Make sure ports `3000`, `3001`, `4000`, `8000`, and `11626` are free before
  starting the stack.
- For WSL2, follow the extra setup notes in
  [docs/windows-wsl-setup.md](windows-wsl-setup.md).

## 2. Services Overview

The current Compose file starts five services.

| Service | What it does | Ports | How to use it |
| --- | --- | --- | --- |
| `stellar` | Runs `stellar/quickstart:testing --local` and provides a local Stellar/Soroban network. This is the "stellar quickstart" service in the repo. | `8000`, `11626` | Use `http://localhost:8000` for Horizon-style APIs and `http://localhost:8000/soroban/rpc` for Soroban RPC from the browser. Port `11626` is the local core/peer port and is mostly for debugging. |
| `contracts` | A Rust 1.76 + `stellar-cli` development shell with `./contracts` mounted at `/app`. | none | This container does **not** auto-deploy contracts today. It stays alive so you can run `cargo` and `stellar` commands inside it with `docker compose exec contracts ...`. |
| `frontend` | Runs the Next.js development server from `frontend/`. | `3000` | The service reads `frontend/.env.local` from the bind mount. If contract IDs are missing, the container will fail fast on env validation. |
| `mock-service` | Runs `json-server` against `mock-service/db.json` for frontend-only development. | `4000` | Use it when you want realistic invoice/pool fixtures without a live Stellar node. |
| `indexer` | Polls Horizon effects and exposes a local API backed by SQLite. | `3001` | Optional helper for history/event work. Set `CONTRACT_IDS` if you want it to track specific contracts. |

## 3. First Run

### Prepare frontend environment

The frontend validates required contract IDs at startup, so create
`frontend/.env.local` before bringing up the full stack:

```bash
cp frontend/.env.example frontend/.env.local
```

Then choose one of these modes.

### Fastest path: UI work with mock data

Edit `frontend/.env.local` so it contains valid-looking placeholder contract IDs
plus mock mode:

```dotenv
NEXT_PUBLIC_NETWORK=testnet
NEXT_PUBLIC_INVOICE_CONTRACT_ID=CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM
NEXT_PUBLIC_POOL_CONTRACT_ID=CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM
NEXT_PUBLIC_USDC_TOKEN_ID=CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM
NEXT_PUBLIC_USE_MOCK=true
NEXT_PUBLIC_MOCK_API_URL=http://localhost:4000
```

This is the fastest way to get the UI running without deploying contracts first.

### Local chain path: standalone contracts

If you want the frontend to talk to the local quickstart node, set:

```dotenv
NEXT_PUBLIC_NETWORK=standalone
NEXT_PUBLIC_HORIZON_URL=http://localhost:8000
NEXT_PUBLIC_SOROBAN_RPC_URL=http://localhost:8000/soroban/rpc
NEXT_PUBLIC_INVOICE_CONTRACT_ID=<replace-after-local-deploy>
NEXT_PUBLIC_POOL_CONTRACT_ID=<replace-after-local-deploy>
NEXT_PUBLIC_USDC_TOKEN_ID=<replace-after-local-deploy>
```

You will fill in the contract IDs after deploying locally from the `contracts`
container.

### Start the stack

```bash
docker compose up --build
```

Useful checks while the stack starts:

```bash
docker compose ps
docker compose logs stellar --follow
```

How to tell services are ready:

- `stellar` is ready when its logs settle into normal ledger activity and
  `http://localhost:8000` responds.
- `contracts` is ready as soon as `docker compose exec contracts stellar --version`
  succeeds.
- `frontend` is ready when `http://localhost:3000` loads without env validation
  errors.
- `mock-service` is ready when `http://localhost:4000/invoices` returns JSON.

Open the app at `http://localhost:3000`.

### Deploy contracts from the contracts container

The current `contracts` service is an interactive dev shell, not an auto-runner.
Use it like this:

```bash
docker compose exec contracts bash
cargo build --target wasm32-unknown-unknown --release
```

From there, use the contract commands in:

- [README.md](../README.md)
- [docs/deployment.md](deployment.md)
- [contracts/invoice/README.md](../contracts/invoice/README.md)
- [contracts/pool/README.md](../contracts/pool/README.md)

If you are targeting the local quickstart node instead of testnet, wait for the
`stellar` service first and then deploy from inside the `contracts` container.

## 4. Common Issues and Fixes

### Port already in use

Symptoms:

- `Bind for 0.0.0.0:3000 failed`
- `port is already allocated`

macOS, Linux, and WSL2:

```bash
lsof -i :3000
kill -9 <PID>
```

Repeat for `3001`, `4000`, or `8000` as needed.

### Frontend exits immediately with env validation errors

Symptoms:

- `NEXT_PUBLIC_INVOICE_CONTRACT_ID is required`
- `NEXT_PUBLIC_POOL_CONTRACT_ID is required`

Fix:

1. Create `frontend/.env.local` from `frontend/.env.example`.
2. Add real contract IDs, or use the documented mock-mode placeholders.
3. Restart just the frontend service:

```bash
docker compose up frontend
```

### Contracts are not deploying

Symptoms:

- Nothing happens in the `contracts` logs
- `stellar contract deploy` fails because the local node is not ready

Fix:

1. Remember that the `contracts` container intentionally runs `tail -f /dev/null`.
   It will not deploy anything until you `exec` into it.
2. Wait for the local quickstart node first:

```bash
docker compose logs stellar --follow
```

3. Then run your build/deploy commands manually:

```bash
docker compose exec contracts bash
```

### Frontend cannot reach local contracts

Symptoms:

- Browser requests fail against Soroban RPC
- Pages mention missing contract IDs or wrong network

Fix:

- For the local quickstart node, use:

```dotenv
NEXT_PUBLIC_NETWORK=standalone
NEXT_PUBLIC_HORIZON_URL=http://localhost:8000
NEXT_PUBLIC_SOROBAN_RPC_URL=http://localhost:8000/soroban/rpc
```

- For testnet work, keep `NEXT_PUBLIC_NETWORK=testnet` and leave the custom URLs
  unset unless you intentionally want a non-default endpoint.
- If you are doing deep standalone contract work, running the frontend on the
  host machine instead of inside Docker is often simpler because browser and
  server-side code both see `localhost:8000`.

### "Insufficient balance" on testnet

This usually means the Freighter account or CLI account needs testnet funds.

Fund a Stellar CLI key:

```bash
stellar keys fund deployer --network testnet
```

Fund a Freighter/browser address:

```bash
curl "https://friendbot.stellar.org/?addr=<YOUR_PUBLIC_KEY>"
```

Use Friendbot only for testnet. It is not part of the standalone local network.

### Docker volume permission errors on Linux or WSL2

Symptoms:

- `EACCES`
- Rust build artifacts owned by `root`
- SQLite files under `indexer/data` cannot be written

Fix:

- Clone the repo into your Linux home directory, not a Windows-mounted path.
- Reclaim ownership of bind-mounted folders if needed:

```bash
sudo chown -R "$USER":"$USER" frontend contracts indexer/data mock-service
```

- Then rebuild the affected containers:

```bash
docker compose up --build
```

## 5. Resetting the Environment

Use this when the local chain state, container filesystem, or cached dependencies
become confusing:

```bash
docker compose down -v --remove-orphans
docker compose up --build
```

Important details:

- `docker compose down -v` removes the named `stellar-data` volume, so the local
  chain resets completely.
- The `indexer` database lives in `indexer/data` as a bind mount. If you want a
  totally clean history/indexer state, delete the contents of `indexer/data/`
  before starting the stack again.

## 6. Running Individual Components

### Only the local Stellar node and contract shell

Useful for contract development when you do not need the frontend:

```bash
docker compose up stellar contracts
docker compose exec contracts bash
```

### Only the frontend against testnet contracts

This avoids Docker entirely for the UI and is usually the simplest path when you
already have deployed testnet contract IDs:

```bash
cd frontend
cp .env.example .env.local
npm ci
npm run dev
```

Set real testnet values in `.env.local` before starting the server.

### Frontend plus mock API only

Useful for UI work with no blockchain dependency:

```bash
docker compose up frontend mock-service
```

Make sure `frontend/.env.local` enables mock mode as shown in the first-run
section.
