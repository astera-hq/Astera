# Astera

[![Soroban Contracts CI](https://github.com/Jayy4rl/Astera/actions/workflows/contracts.yml/badge.svg?branch=main)](https://github.com/Jayy4rl/Astera/actions/workflows/contracts.yml)

**Real World Assets on Stellar. Invoice financing for emerging markets.**

Astera lets SMEs tokenize unpaid invoices as Soroban-based RWA tokens. Community investors
fund a USDC liquidity pool. Smart contracts handle escrow, repayment, and yield distribution.
Every paid invoice builds an on-chain credit history.

---

## Architecture

```
contracts/
  invoice/   — RWA invoice token contract (Soroban/Rust)
  pool/      — Liquidity pool + yield distribution (Soroban/Rust)
frontend/    — Next.js 14 app (Freighter wallet, Stellar SDK)
```

## Contracts

### Invoice Contract

- `create_invoice` — SME mints an invoice token with amount, debtor, due date
- `mark_funded` — Called by pool when invoice is funded
- `mark_paid` — SME or pool marks invoice as repaid
- `mark_defaulted` — Pool flags missed repayment

### Pool Contract

- `initialize` — Sets admin, first accepted stablecoin (`initial_token`), and invoice contract
- `add_token` / `remove_token` — Admin maintains a whitelist of accepted stablecoin SAC addresses
- `deposit` — Investor deposits a whitelisted stablecoin into the pool (positions are per token)
- `init_co_funding` — Admin opens an invoice for co-funding in a specific stablecoin
- `commit_to_invoice` — Investors commit **available balance in that invoice’s token** until the principal target is met
- `repay_invoice` — SME repays principal + simple interest (8% APY default) **in the same token the invoice was funded with**
- `withdraw` — Investor withdraws available (undeployed) balance **in the chosen token**

---

## Getting Started

Start with the guide that matches how you want to work:

- [Local Development with Docker Compose](docs/local-development.md)
- [Windows/WSL2 Setup Guide](docs/windows-wsl-setup.md)
- [Testnet Deployment Guide](docs/deployment.md)
- [Smart Contract Interaction Guide](docs/interacting-with-contracts.md)

### Rapid Local Development (Docker Compose)
We provide a Docker Compose workflow for the Stellar local network, the Next.js
frontend, a contracts development shell, mock services, and the optional local
indexer.

```bash
docker compose up --build
```
After the stack is ready:
- **Frontend** is available at http://localhost:3000
- **Stellar Horizon/RPC** is available at http://localhost:8000
- **Mock Services** are available at http://localhost:4000

Read the full [local development guide](docs/local-development.md) before the
first run. It explains the required frontend env file, manual contract workflow,
common failures, and reset commands.

---

### Manual Setup

### Prerequisites

If you are on Windows, we strongly recommend using WSL2. See our [Windows/WSL2 Setup Guide](docs/windows-wsl-setup.md) for details.

- [Rust + Cargo](https://rustup.rs/)
- [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools/stellar-cli)
- [Node.js 20+](https://nodejs.org/)
- [Freighter wallet](https://www.freighter.app/) browser extension

### 1. Build contracts

```bash
cd astera
cargo build --target wasm32-unknown-unknown --release
```

### 2. Deploy to Testnet

```bash
# Fund a testnet account
stellar keys generate --global deployer --network testnet
stellar keys fund deployer --network testnet

# Deploy invoice contract
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/invoice.wasm \
  --source deployer \
  --network testnet

# Deploy pool contract
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/pool.wasm \
  --source deployer \
  --network testnet
```

### 3. Initialize contracts

```bash
# Initialize invoice contract
stellar contract invoke \
  --id <INVOICE_CONTRACT_ID> \
  --source deployer \
  --network testnet \
  -- initialize \
  --admin <YOUR_ADDRESS> \
  --pool <POOL_CONTRACT_ID>

# Initialize pool contract
stellar contract invoke \
  --id <POOL_CONTRACT_ID> \
  --source deployer \
  --network testnet \
  -- initialize \
  --admin <YOUR_ADDRESS> \
  --usdc_token <USDC_TOKEN_ID> \
  --invoice_contract <INVOICE_CONTRACT_ID>
```

### 4. Run frontend

```bash
cd frontend
cp .env.example .env.local
# Fill in contract IDs in .env.local

npm install
npm run dev
```

Open [http://localhost:3000](http://localhost:3000).

---

## User Flows

### SME Flow

1. Connect Freighter wallet
2. Go to **New Invoice** — fill debtor, amount, due date
3. Sign transaction — invoice minted on Stellar
4. Monitor status on **Dashboard** — see funding, credit score
5. When customer pays, call `repay_invoice` to settle

### Investor Flow

1. Connect Freighter wallet
2. Go to **Invest** — choose a whitelisted stablecoin and deposit into the pool
3. Pool admin deploys liquidity to approved invoices
4. When invoices are repaid, yield accumulates in the pool
5. Withdraw available balance anytime

---

## Testnet USDC

Use the Stellar testnet USDC asset or deploy a mock token:

```bash
stellar contract invoke \
  --id <TOKEN_ID> \
  --source deployer \
  --network testnet \
  -- mint \
  --to <YOUR_ADDRESS> \
  --amount 1000000000000
```

---

## Deployment

### Frontend Deployment

Deploy your own instance of the Astera frontend with one click:

[![Deploy with Vercel](https://vercel.com/button)](https://vercel.com/new/clone?repository-url=https://github.com/astera-hq/Astera&root-directory=frontend&env=NEXT_PUBLIC_NETWORK,NEXT_PUBLIC_INVOICE_CONTRACT_ID,NEXT_PUBLIC_POOL_CONTRACT_ID,NEXT_PUBLIC_USDC_TOKEN_ID)

For detailed instructions on various hosting options, see the [Frontend Deployment Guide](docs/frontend-deployment.md).

### Testnet Deployment

For development and testing, see the [Testnet Deployment Guide](docs/deployment.md) for step-by-step instructions.

### Mainnet Deployment

For production deployment, see the comprehensive [Mainnet Deployment Guide](docs/mainnet-deployment.md) which includes:

- Pre-deployment security checklist
- Contract verification procedures
- Monitoring and alerting setup
- Rollback and emergency procedures
- Post-deployment verification steps

For upgrade runbooks and migration safety checks, see the [Contract Upgrade Guide](docs/contract-upgrade-guide.md).

**⚠️ Important:** Mainnet deployment involves real assets. Complete all security audits and testing before deploying to production.

---

## Network Information

### Testnet

- **RPC:** https://soroban-testnet.stellar.org
- **Horizon:** https://horizon-testnet.stellar.org
- **Explorer:** https://stellar.expert/explorer/testnet

### Mainnet

- **RPC:** https://soroban-mainnet.stellar.org
- **Horizon:** https://horizon.stellar.org
- **Explorer:** https://stellar.expert/explorer/public

---

## Continuous Integration

The [`contracts.yml`](.github/workflows/contracts.yml) workflow runs on every push
to `main` and on every pull request targeting `main`. It:

- Installs Rust stable with the `wasm32-unknown-unknown` target
- Caches `~/.cargo/registry` and `target/` via `Swatinem/rust-cache`
- Builds the workspace with `cargo build --target wasm32-unknown-unknown --release`
- Runs `cargo test` for each contract (`invoice`, `pool`, `credit_score`)

### Branch protection

Branch protection for `main` should be configured so that:

- Pull requests cannot be merged unless the **`Build & test Soroban contracts`** check passes.
- Direct pushes to `main` are disabled (PRs only).

To apply, in GitHub: **Settings → Branches → Branch protection rules → Add rule** for
`main`, enable **Require status checks to pass before merging**, and select
`Build & test Soroban contracts` from the list of checks.

## 💰 Bounty Contribution

- **Task:** Add SME onboarding verification — prevent duplicate SME accounts
- **Reward:** $10
- **Source:** GitHub-Paid
- **Date:** 2026-04-27

