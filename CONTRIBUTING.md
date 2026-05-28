# Contributing to Astera

Thank you for your interest in contributing to **Astera**! We welcome developers of all experience levels, especially those interested in Soroban smart contract development and tokenized real-world asset infrastructure.

---

## 🌊 The Drips Wave Program

Astera participates in the **Drips Wave Program**, a sprint-based open source contribution model that rewards meaningful contributions:

- Defines a short contribution cycle (typically ~1 week)
- Has a shared reward pool tied to merged pull requests
- Tracks contribution points for transparent reward distribution
- Enables contributors to earn based on impact, not just activity

For more about Waves and how they work, visit the official docs: https://docs.drips.network/wave.

---

## 🧭 How to Find and Claim an Issue

1. Go to the **Issues** tab of this repository
2. Look for labels such as:
   - `good first issue` — suitable for first-time contributors
   - `help wanted` — contributions explicitly requested
   - `wave` — eligible for Wave program rewards
   - `smart-contract` — Soroban/Rust contract changes
   - `frontend` — Next.js frontend changes
   - `security` — security improvements and fixes
3. Comment on the issue you want to work on:

   ```text
   I'd like to work on this
   ```

   This prevents duplicate effort and lets maintainers confirm assignment.

4. Wait for a maintainer to confirm. Once assigned, you can begin work.

---

## 🎯 Your First Contribution

New to open source or Astera? Follow this step-by-step walkthrough to make your first meaningful contribution.

### Step 1: Find a good first issue

Look for issues tagged with `good first issue` or `help wanted`. These are scoped to be achievable in a few hours.

Example: You find issue #223: "Add CONTRIBUTING.md first-time contributor guide"

### Step 2: Claim the issue

Comment on the issue:
```text
I'd like to work on this
```

A maintainer will confirm assignment. This prevents duplicate effort.

### Step 3: Fork and clone

```bash
# Fork via GitHub UI, then clone your fork
git clone https://github.com/YOUR_USERNAME/Astera.git
cd Astera

# Add upstream for easy syncing
git remote add upstream https://github.com/astera-hq/Astera.git

# Create a feature branch
git checkout -b feat/your-feature-name
```

### Step 4: Set up your environment

Follow the **Development Environment Setup** section below to install Rust,
Node.js, and other tools. If you plan to use Docker Compose locally, read
[docs/local-development.md](docs/local-development.md) first.

### Step 5: Make your change

**Example: Adding a unit test to the share token contract**

Open `contracts/share/src/lib.rs` and add:

```rust
#[test]
fn test_mint_increases_balance() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ShareToken);
    let client = ShareTokenClient::new(&env, &contract_id);
    
    let user = Address::generate(&env);
    client.mock_all_auths().mint(&user, &100);
    
    assert_eq!(client.balance(&user), 100);
}
```

Or if you're updating frontend documentation:

Open `CONTRIBUTING.md` and add your walkthrough section (like this one).

### Step 6: Run tests and linting

```bash
# For contracts
cd contracts
cargo fmt
cargo clippy
cargo test

# For frontend
cd ../frontend
npm run lint
npm run build
```

Ensure all checks pass before proceeding.

### Step 7: Commit your changes

```bash
# Stage your changes
git add contracts/share/src/lib.rs

# Commit with a clear, conventional message
git commit -m "test(share): add unit tests for token mint operation"
```

### Step 8: Push and open a pull request

```bash
# Push your branch
git push origin feat/your-feature-name
```

Then:
1. Go to https://github.com/astera-hq/Astera
2. Click **"Compare & pull request"** for your branch
3. Fill in the PR template with:
   - A clear summary of what you changed and why
   - Link the issue: `Closes #123`
   - Describe how you tested it
4. Click **"Create pull request"**

### Step 9: Respond to review feedback

A maintainer will review your PR within 1–3 business days. They may request changes. That's normal and part of learning!

- **Request Changes**: Fix the issue and push new commits to the same branch
- **Approved**: Your PR will be merged soon

---

## � Prerequisites

Before contributing, set up your environment as follows. If you are using
Docker Compose for local development, start with the
[Local Development Guide](docs/local-development.md). If you are using Windows,
please follow the [Windows/WSL2 Setup Guide](docs/windows-wsl-setup.md).
For a guide on how to interact with the contracts, see the [Smart Contract Interaction Guide](docs/interacting-with-contracts.md).

### Required Tools

#### **Rust Toolchain**
- **Why**: Compiles Soroban smart contracts written in Rust to WebAssembly (WASM)
- **Install**: https://rustup.rs/
- **Verification**: `rustc --version` and `cargo --version`
- **What you'll use it for**: Building contracts, running tests, linting with Clippy, formatting with `rustfmt`

#### **Soroban CLI (Stellar CLI)**
- **Why**: Deploys contracts to testnet and mainnet; initializes and invokes contract functions
- **Install**: https://developers.stellar.org/docs/tools/developer-tools/stellar-cli
- **Verification**: `stellar --version`
- **What you'll use it for**: Deploying contracts, managing testnet accounts, invoking contract methods

#### **Node.js 20+**
- **Why**: Runs the Next.js 14 frontend and its build/test tools
- **Install**: https://nodejs.org/ (recommend using a version manager like `nvm` or `fnm`)
- **Verification**: `node --version` (should be v20.0.0 or higher) and `npm --version`
- **What you'll use it for**: Installing frontend dependencies, running linters (ESLint), building the frontend, running E2E tests (Playwright)

#### **Freighter Wallet**
- **Why**: Browser extension that signs transactions on the Stellar network; critical for local testing
- **Install**: https://www.freighter.app/
- **What you'll use it for**: Testing contract interactions, funding testnet accounts, signing deployment transactions

#### **Git** (optional but recommended)
- For syncing your fork, managing branches, and contributing changes
- Usually pre-installed on macOS and Linux

#### **Docker** (optional but recommended for rapid setup)
- For running the entire Astera stack (Stellar network, frontend, contracts, and mock services) with one command
- See [Rapid Local Development](#rapid-local-development-docker-compose) below

---

## 🚀 Local Setup

### Rapid Local Development (Docker Compose)

If you want a fully integrated development environment without manual setup, use Docker Compose:

```bash
# Clone the repository
git clone https://github.com/<your-username>/Astera.git
cd Astera

# Start the entire stack
docker-compose up -d
```

After the containers start:
- **Frontend**: http://localhost:3000
- **Stellar RPC**: http://localhost:8000
- **Mock Services**: http://localhost:4000

To stop: `docker-compose down`

### Manual Setup (Step-by-Step)

If you prefer to set up dependencies manually or don't have Docker:

#### 1. Clone the Repository

```bash
# Clone your fork
git clone https://github.com/<your-username>/Astera.git
cd Astera

# Add upstream remote (for syncing with the main repository)
git remote add upstream https://github.com/astera-hq/Astera.git

# Create a feature branch
git checkout -b feat/your-feature-name
```

#### 2. Set Up Rust Environment

```bash
# Install the WASM compilation target (required for Soroban contracts)
rustup target add wasm32-unknown-unknown

# Verify your Rust installation
cargo --version
rustc --version
```

#### 3. Build and Test Smart Contracts

```bash
# Navigate to contracts directory
cd contracts

# Run the full contract test suite
cargo test

# Build all contracts for deployment
cargo build --target wasm32-unknown-unknown --release

# Run linting checks (required before PRs)
cargo fmt && cargo clippy -- -D warnings
```

The built WASM binaries will be in `target/wasm32-unknown-unknown/release/`. You'll see:
- `invoice.wasm`
- `pool.wasm`
- `credit_score.wasm`
- `share.wasm`

#### 4. Set Up Frontend Dependencies

```bash
# Navigate to frontend directory
cd ../frontend

# Copy the example environment file
cp .env.example .env.local

# Edit .env.local with your testnet contract IDs and network settings
# (You can leave these as placeholder values for initial development)

# Install dependencies
npm install
```

#### 5. Verify the Setup

Run the frontend development server to confirm everything is working:

```bash
# From the frontend directory
npm run dev
```

Visit http://localhost:3000 in your browser. You should see the Astera home page. If you see build errors, check that:
- Node.js is v20 or higher: `node --version`
- All npm dependencies installed: `npm install`
- `.env.local` exists with required contract ID placeholders

---

## 🧪 Running Tests

Comprehensive testing is critical for smart contract correctness and security. You must run the appropriate tests for your changes before submitting a PR.

### Smart Contract Tests (Rust/Soroban)

All contract code **must** be tested. The test suite includes unit tests and fuzz tests.

#### Unit Tests

Run tests for all contracts:

```bash
cd contracts

# Run all tests in all contracts (invoice, pool, credit_score, share)
cargo test

# Run tests for a specific contract (example: invoice)
cd invoice
cargo test

# Run a specific test function
cargo test test_initialize

# Run tests with verbose output (shows individual test results)
cargo test -- --nocapture
```

**Testing requirements**:
- Every public function must have **at least one unit test** covering the happy path
- Edge cases (zero amounts, unauthorized callers, duplicate operations, boundary conditions) should be covered
- Tests live in `#[cfg(test)]` modules at the bottom of each contract's `lib.rs`
- All tests must pass before opening a PR

#### Fuzz Tests

Fuzz tests generate random inputs to find edge cases and crashes. They are defined in `contracts/<contract>/tests/fuzz_tests.rs`.

```bash
cd contracts/<contract>

# Run fuzz tests (defined in tests/fuzz_tests.rs)
cargo test --test fuzz_tests --verbose

# Run fuzz tests with a specific seed for reproducibility
PROPTEST_SEED=<seed> cargo test --test fuzz_tests

# Each contract declares a named `fuzz_tests` target in Cargo.toml so CI can run this command directly.
```

Fuzz tests are **required to pass** in the CI pipeline. If fuzzing finds an issue, fix the underlying code logic and re-run the tests.

#### Integration Tests

End-to-end tests that verify multiple contracts interacting together:

```bash
cd contracts

# Run all integration tests
cargo test --test integration_tests --verbose

# Run a specific integration test
cargo test --test integration_tests test_deposit_and_repay
```

Fuzz harness smoke tests are also part of CI for the contract crates that support them:

```bash
cd contracts/invoice
cargo test --test fuzz_tests --verbose
```

Build for deployment:

```bash
cd contracts

# Build all contracts for mainnet/testnet deployment
cargo build --target wasm32-unknown-unknown --release

# Verify WASM binary sizes (required by CI; max 200 KB per contract)
for wasm in target/wasm32-unknown-unknown/release/*.wasm; do
  SIZE=$(wc -c < "$wasm" | tr -d ' ')
  echo "$wasm: $SIZE bytes"
  if [ "$SIZE" -gt 204800 ]; then
    echo "ERROR: Binary too large for deployment"
    exit 1
  fi
done
```

WASM binaries must not exceed **204,800 bytes** (200 KB). The CI pipeline checks this automatically.

#### Linting and Formatting

Before submitting contract changes, ensure linting and formatting pass:

```bash
cd contracts

# Format all code according to Rust style guidelines
cargo fmt

# Run Clippy linter (warnings are treated as errors in CI)
cargo clippy -- -D warnings

# Check formatting without modifying files (useful in pre-commit hooks)
cargo fmt -- --check
```

All three commands must pass for contracts in a PR. The CI pipeline runs these checks automatically.

---

### Frontend Tests (TypeScript/Next.js)

#### Unit and Component Tests (Jest)

```bash
cd frontend

# Run all Jest tests
npm test

# Run tests in watch mode (re-runs on file changes)
npm run test:watch

# Run a specific test file
npm test -- MyComponent.test.tsx

# Run tests with coverage report
npm test -- --coverage
```

All new components and utilities should have test coverage. Check that:
- Tests exercise the happy path and error cases
- Component props are validated
- Hooks are tested with `renderHook`

#### Build and Type Checking

Ensure the production build succeeds (this catches TypeScript errors):

```bash
cd frontend

# Build the Next.js app (verifies TypeScript and Webpack compilation)
npm run build

# You should see output like: "✓ Linting and checking validity of types"
```

The build failing indicates a TypeScript error that must be fixed before PR submission.

#### Linting

```bash
cd frontend

# Run ESLint on all frontend code
npm run lint

# Fix auto-fixable linting issues
npm run lint -- --fix
```

All ESLint violations must be resolved. The CI pipeline fails if linting doesn't pass.

#### End-to-End Tests (Playwright)

Playwright tests verify the entire application flow from a user's perspective:

```bash
cd frontend

# Run all E2E tests in headless mode
npm run test:e2e

# Run E2E tests with UI browser visible (useful for debugging)
npm run test:e2e:ui

# Run a specific E2E test
npm run test:e2e -- tests/e2e/invest.spec.ts

# Run E2E tests in debug mode (step through with DevTools)
npx playwright test --debug
```

E2E tests are located in `frontend/e2e/`. They verify critical user flows such as:
- Connecting Freighter wallet
- Creating invoices
- Depositing into the pool
- Investing in invoice co-funding

E2E tests require a running Stellar testnet RPC (usually `localhost:8000` in development). See [Rapid Local Development](#rapid-local-development-docker-compose) to set up the local Stellar network.

All new components and utilities should have test coverage. Check that:
- Tests exercise the happy path and error cases
- Component props are validated
- Hooks are tested with `renderHook`

### Running the Complete Test Suite Locally

Before opening a PR, run the full test suite locally to catch issues early:

```bash
# Contracts
cd contracts
cargo fmt
cargo clippy -- -D warnings
cargo test
cargo test --test fuzz_tests
cargo test --test integration_tests
cargo audit  # Security dependency check

# Frontend
cd ../frontend
npm run lint
npm run build
npm test
npm run test:e2e  # (requires running Stellar network)
```

The CI pipeline runs a similar sequence. Passing locally means your PR is more likely to pass CI.

#### Running CI Locally with `act` (macOS)

To simulate the GitHub Actions CI pipeline locally before pushing, you can use [`act`](https://nektosact.com/):

```bash
# Install act (on macOS with Homebrew)
brew install act

# Run the CI pipeline locally
act --reuse-containers

# Run a specific job (e.g., check-wasm-size)
act --job check-wasm-size
```

**Important for macOS users**: The CI scripts use cross-platform utilities that work on both Linux and macOS. Specifically, we use `wc -c` instead of the Linux-only `stat -c` for binary size checks. If you encounter errors when running CI locally on macOS, ensure you're using standard POSIX utilities and avoid Linux-specific GNU tools. If you need to debug a size check, use:

```bash
# Cross-platform way to check WASM binary size on macOS or Linux
wc -c < target/wasm32-unknown-unknown/release/invoice.wasm | tr -d ' '
```

---

## 🚀 Deploying to Testnet

Once you've made changes to smart contracts and verified they pass all local tests, you can deploy them to the Soroban testnet to test in a live environment.

**Prerequisites for deployment**:
- Contracts build successfully: `cargo build --target wasm32-unknown-unknown --release`
- Stellar CLI installed and verified: `stellar --version`
- Freighter wallet with a testnet account

### Create a Testnet Account

If you don't have a testnet account, create one:

```bash
# Generate a new keypair
stellar keys generate --global deployer --network testnet

# Fund the account with testnet Lumens (XLM)
stellar keys fund deployer --network testnet
```

Verify the account is funded:

```bash
stellar account info deployer --network testnet
```

You should see a balance in XLM.

### Build Contracts for Deployment

```bash
cd contracts

# Build all contracts in release mode
cargo build --target wasm32-unknown-unknown --release

# Verify WASM binaries exist
ls -lah target/wasm32-unknown-unknown/release/*.wasm
```

You should see four `.wasm` files:
- `invoice.wasm`
- `pool.wasm`
- `credit_score.wasm`
- `share.wasm`

### Deploy Invoice Contract

```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/invoice.wasm \
  --source deployer \
  --network testnet
```

This command will output a contract ID. **Note this ID** — you'll need it to initialize the contract and in environment variables. Example output:

```
Contract ID: CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM
```

### Deploy Pool Contract

```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/pool.wasm \
  --source deployer \
  --network testnet
```

Again, note the contract ID.

### Deploy Credit Score Contract

```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/credit_score.wasm \
  --source deployer \
  --network testnet
```

### Initialize Contracts

Contract initialization sets up configuration and permissions. You need to initialize both the invoice and pool contracts.

#### Initialize Invoice Contract

```bash
stellar contract invoke \
  --id <INVOICE_CONTRACT_ID> \
  --source deployer \
  --network testnet \
  -- initialize \
  --admin <YOUR_ACCOUNT_ADDRESS> \
  --pool <POOL_CONTRACT_ID>
```

Replace:
- `<INVOICE_CONTRACT_ID>` with the ID you received from the deploy step above
- `<YOUR_ACCOUNT_ADDRESS>` with your account public key (from `stellar keys show deployer --network testnet`)
- `<POOL_CONTRACT_ID>` with the pool contract ID from deployment

#### Initialize Pool Contract

```bash
stellar contract invoke \
  --id <POOL_CONTRACT_ID> \
  --source deployer \
  --network testnet \
  -- initialize \
  --admin <YOUR_ACCOUNT_ADDRESS> \
  --usdc_token <USDC_TOKEN_ID> \
  --invoice_contract <INVOICE_CONTRACT_ID>
```

Replace:
- `<POOL_CONTRACT_ID>` with the pool contract ID
- `<YOUR_ACCOUNT_ADDRESS>` with your account public key
- `<USDC_TOKEN_ID>` with the testnet USDC token ID (or deploy your own mock token)
- `<INVOICE_CONTRACT_ID>` with the invoice contract ID

### Verify Deployment

You can verify contracts are deployed and initialized by reading their state:

```bash
# Check invoice contract initialization
stellar contract invoke \
  --id <INVOICE_CONTRACT_ID> \
  --source deployer \
  --network testnet \
  -- get_config

# Check pool contract state
stellar contract invoke \
  --id <POOL_CONTRACT_ID> \
  --source deployer \
  --network testnet \
  -- get_config
```

If both return data (no errors), your contracts are deployed and initialized.

### View Deployment on Explorer

Visit the Stellar Expert explorer to confirm your deployment:

- **Testnet**: https://stellar.expert/explorer/testnet/contract/<CONTRACT_ID>

Replace `<CONTRACT_ID>` with the actual contract ID to view the contract details.

### Use Deployed Contracts in Frontend

Update your frontend `.env.local` with the deployed contract IDs:

```bash
# In frontend/.env.local
NEXT_PUBLIC_NETWORK=testnet
NEXT_PUBLIC_INVOICE_CONTRACT_ID=<INVOICE_CONTRACT_ID>
NEXT_PUBLIC_POOL_CONTRACT_ID=<POOL_CONTRACT_ID>
NEXT_PUBLIC_USDC_TOKEN_ID=<USDC_TOKEN_ID>
```

Then restart the frontend dev server: `npm run dev`

---

## 🔐 Security Checklist

**Before submitting a PR with smart contract changes, review and check off the following security items.** This checklist helps catch common vulnerabilities and ensures code quality.

### Authorization & Access Control

- [ ] All state-changing functions require proper authorization with `.require_auth()`
- [ ] Admin-only functions are guarded by checking against a stored admin address
- [ ] Pool-only functions verify the caller is the registered pool contract
- [ ] No auth bypass—authorization checks cannot be skipped or circumvented
- [ ] Auth is checked BEFORE state mutations (not after)

### Input Validation

- [ ] All numeric inputs (amounts, rates, delays) are validated for reasonable bounds
- [ ] Zero or negative amounts are rejected where they shouldn't be allowed
- [ ] Addresses are validated before use (e.g., not checking against empty addresses)
- [ ] Due dates are validated to be in the future for invoices
- [ ] Token addresses are validated to be whitelisted or registered

### Reentrancy & State Safety

- [ ] Functions follow the Check-Effects-Interactions pattern:
  - **Check**: Validate inputs and authorization first
  - **Effects**: Update internal state
  - **Interactions**: Call external contracts last
- [ ] No dangerous nested calls that could re-enter the contract
- [ ] Idempotency guards (e.g., `executed` or `paid` flags) prevent duplicate execution
- [ ] Events are emitted consistently for all state changes

### Integer Arithmetic

- [ ] Overflow and underflow are not possible (Rust enforces this in release mode, but reason through math)
- [ ] Interest calculations use appropriate precision (avoid losing funds due to rounding)
- [ ] Checked arithmetic operations (`.checked_add()`, `.checked_sub()`, `.checked_mul()`) for complex operations
- [ ] Division by zero is impossible (divisors are validated to be > 0)

### Storage & Data Integrity

- [ ] All mutable storage reads (`env.storage().instance().get()`) are followed by validation
- [ ] Storage keys are defined in a `DataKey` enum to avoid collisions
- [ ] Critical data (balances, permissions) is stored durably, not transiently
- [ ] Data migrations (if any) maintain consistency and don't corrupt state

### Error Handling & Recovery

- [ ] Errors are clear and descriptive (help developers debug)
- [ ] `panic!()` is used for contract errors; messages are concise
- [ ] No silent failures—failed operations are explicitly rejected
- [ ] Recovery paths are tested (e.g., what happens if a withdrawal fails?)

### Cryptography & Secrets

- [ ] No private keys, seed phrases, or secrets hardcoded in the contract
- [ ] All signing is delegated to Stellar's transaction signing mechanism
- [ ] Contract does not attempt to implement custom cryptography
- [ ] Nonces or timestamps are only used with explicit purpose and validation

### Testing & Coverage

- [ ] New functions include unit tests covering happy path + edge cases
- [ ] Edge cases are explicitly tested (zero amounts, max values, boundary conditions)
- [ ] Fuzz tests pass without panicking on random inputs
- [ ] Integration tests verify multi-contract interactions
- [ ] Authorization tests verify unauthorized callers are blocked

### Code Quality

- [ ] `cargo clippy -- -D warnings` passes (no warnings)
- [ ] `cargo fmt` has been run (consistent code formatting)
- [ ] `cargo audit` reports no critical or high-severity dependency vulnerabilities
- [ ] Comments explain non-obvious logic, especially around security-critical code
- [ ] No useless clones, allocations, or performance issues flagged by Clippy

### Configuration & Constants

- [ ] Magic numbers (rates, caps, limits) are defined as named `const` values at the module level
- [ ] Default yield rate and factoring fees are capped and validated
- [ ] Timelock durations are appropriate for mainnet (typically 24+ hours)
- [ ] All constants are documented with their purpose and unit

### Event Logging

- [ ] All state-changing operations emit clear events
- [ ] Events include relevant indexed topics (contract, user, invoice_id, etc.)
- [ ] Events are emitted AFTER state changes to maintain consistency
- [ ] Events are queryable and useful for frontend and monitoring

---

## 🤖 Dependabot PRs

Dependabot checks the Rust workspace, frontend npm dependencies, and GitHub
Actions workflows for dependency updates. It opens pull requests with existing
area labels so maintainers can route them quickly:

- `smart-contract` and `devops` for Cargo workspace updates
- `frontend` and `devops` for frontend npm updates
- `devops` and `tooling` for GitHub Actions updates

When reviewing a Dependabot PR:

1. Prioritize security updates and runtime dependency updates before tooling-only changes.
2. Confirm the affected area by checking the labels and changed lockfiles.
3. Run the relevant local checks from the testing section above.
4. Merge only one dependency update PR at a time when failures would be hard to isolate.

---

## 📋 Pull Request Process

### Before Opening a PR

1. **Sync your fork** with the latest upstream `main`:
   ```bash
   git fetch upstream
   git rebase upstream/main
   ```

2. **Run all checks locally** to catch issues early:
   ```bash
   # Contracts (if you modified any)
   cd contracts && cargo fmt && cargo clippy -- -D warnings && cargo test && cargo test --test fuzz_tests

   # Frontend (if you modified any)
   cd frontend && npm run lint && npm run build
   ```

3. **Clean up your commits**:
   ```bash
   # No merge commits—use rebase if needed
   git rebase upstream/main
   ```

Before opening a PR, make sure you:

- [ ] Linked an issue (e.g., `Closes #123`)
- [ ] Built the project locally
- [ ] All tests pass (`cargo test` and `npm run build`)
- [ ] Code is formatted consistently (`cargo fmt`, Prettier)
- [ ] Linting passes (`cargo clippy`, `npm run lint`)
- [ ] Commit messages follow Conventional Commits
- [ ] PR title follows the `type(scope): description` format
- [ ] PR description clearly explains the change
- [ ] New public contract functions include tests
- [ ] No secrets or `.env.local` committed
- [ ] `CHANGELOG.md` updated under the `Unreleased` section if the change is user-facing
- [ ] **Smart contract security checklist** completed (see `.github/pull_request_template.md`) if `contracts/` changed

```
feat/short-description
fix/short-description
docs/short-description
refactor/short-description
```

Examples:
- `feat/add-invoice-repayment-tracking`
- `fix/pool-withdraw-edge-case`
- `docs/add-deployment-guide`

### Commit Message Format

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): short description
```

Where `type` is one of:
- `feat` — New feature
- `fix` — Bug fix
- `docs` — Documentation
- `test` — Tests only
- `refactor` — Code restructuring
- `perf` — Performance improvements
- `chore` — Build/tooling changes
- `ci` — CI/CD changes
- `style` — Formatting (no logic change)

Examples:
```
feat(invoice): add due date validation in create_invoice
fix(pool): resolve withdraw edge case for fractional shares
docs(contributing): update development setup instructions
test(pool): add tests for deposit rounding behavior
```

### PR Title & Description

Use the same Conventional Commits format for your PR title:

```
type(scope): description
```

In the description, include:

- **Summary**: What changed and why?
- **Related Issue**: `Closes #<issue-number>` (links the PR to the issue)
- **Changes**: Bullet list of key changes
- **Testing**: How you verified the changes work
- **Security Impact** (if applicable): Any security-relevant changes
- **Screenshots** (if UI changes): Before and after

Example PR description:

```markdown
## Summary
Add validation to ensure invoice due dates are in the future. This prevents SMEs from creating invoices with past due dates.

## Related Issue
Closes #156

## Changes
- Added `validate_due_date()` function in invoice contract
- Checks that due_date > current_ledger_sequence
- Rejects with clear error message if date is in past
- Added unit tests for edge cases (today, tomorrow, far future)

## Testing
- All unit tests pass: `cargo test`
- Fuzz tests pass: `cargo test --test fuzz_tests`
- Tested manually on testnet with Freighter

## Security Checklist
- [x] Authorization checks in place
- [x] Input validation for due_date
- [x] No reentrancy issues
- [x] All tests pass
```

### PR Checklist

Before clicking "Create pull request", verify:

- [ ] PR title follows `type(scope): description` format
- [ ] Issue is linked: `Closes #<number>` in description
- [ ] **Contract changes?** All tests pass: `cargo test`, `cargo test --test fuzz_tests`, `cargo test --test integration_tests`
- [ ] **Frontend changes?** Build and lint pass: `npm run lint` and `npm run build`
- [ ] All code is formatted: `cargo fmt` (contracts), Prettier (frontend)
- [ ] No secrets or `.env.local` committed
- [ ] Commit messages follow Conventional Commits format
- [ ] **New public contract functions?** Includes unit tests
- [ ] **User-facing changes?** `CHANGELOG.md` updated in `Unreleased` section
- [ ] **Smart contract changes?** Security checklist reviewed above and all items checked if applicable

### Code Review Turnaround

- Initial review typically within **1–3 business days**
- Maintainers may request changes—this is normal and helps ensure quality
- Once approved, your PR will be merged and you'll earn contribution points if part of a Wave

---

## 🏷️ Issue Labels

The following labels are used in the issue tracker to categorize work:

| Label | Meaning | Who Should Pick It Up |
| --- | --- | --- |
| `good first issue` | Scoped for new contributors; achievable in a few hours | New contributors looking for an entry point |
| `help wanted` | Explicitly requesting external contributions | Anyone interested in helping |
| `wave` | Eligible for Drips Wave Program rewards | Wave participants earning contributions points |
| `smart-contract` | Changes to Soroban contracts (Rust code) | Rust/Soroban developers |
| `frontend` | Changes to the Next.js frontend | TypeScript/React developers |
| `security` | Security improvements, audits, vulnerability fixes | Security-focused developers |
| `documentation` | README, guides, API docs, comments | Technical writers, anyone familiar with the feature |
| `bug` | Something is broken or not working as designed | Anyone who can debug and fix the issue |
| `enhancement` | New feature or improvement to existing feature | Feature developers |
| `refactor` | Internal code restructuring without behavior change | Developers improving code quality |
| `testing` | Tests, test infrastructure, CI/CD improvements | QA and automation-focused developers |
| `windows` / `wsl` | Windows or WSL-specific issues | Windows developers |
| `deps` | Dependency updates, upgrades, version bumps | DevOps/maintainers |

---
## 🎨 Code Style Guidelines

### Rust (Smart Contracts)

| Rule | Detail |
| --- | --- |
| **Formatter** | `cargo fmt` — run before every commit |
| **Linter** | `cargo clippy -- -D warnings` — no warnings allowed |
| **`#![no_std]`** | All contracts must be `no_std` compatible |
| **Naming** | `snake_case` for functions/variables, `PascalCase` for types/enums |
| **Error handling** | Use `panic!("descriptive message")` for contract errors; keep messages concise and helpful |
| **Events** | Use `symbol_short!` for event topics; publish events for all state-changing operations |
| **Storage keys** | Define all keys in the `DataKey` enum to prevent collisions |
| **Authorization** | Always call `.require_auth()` on the relevant `Address` before mutating state |
| **Constants** | Use `const` for fixed values (e.g. `DEFAULT_YIELD_BPS`, `SECS_PER_YEAR`) with uppercase names |

### TypeScript (Frontend)

| Rule | Detail |
| --- | --- |
| **Formatter** | Prettier (runs automatically via `lint-staged` on commit) |
| **Linter** | ESLint with `eslint-config-next` and `@typescript-eslint` |
| **Framework** | Next.js 14 App Router — use `'use client'` directive only when needed |
| **State** | Zustand for global state (`lib/store.ts`); React hooks for local UI state |
| **Styling** | Tailwind CSS utility classes; follow existing `brand-*` design tokens |
| **Naming** | `PascalCase` for components, `camelCase` for functions/variables/hooks |
| **Imports** | Use `@/` path alias (e.g. `@/lib/store`, `@/components/Navbar`) |
| **Types** | Define shared types in `lib/types.ts`; prefer `interface` over `type` for objects |
| **Contract calls** | All Soroban interaction builders go in `lib/contracts.ts` |
| **SDK helpers** | Stellar SDK utilities in `lib/stellar.ts` |

### General Best Practices

- Keep files focused — ideally one component or module per file
- Avoid unnecessary dependencies—prefer standard library alternatives when available
- Never commit `.env.local` or secret keys to the repository
- Use English for all code comments, commit messages, and documentation

---

## 📋 Changelog

This project maintains a [CHANGELOG.md](CHANGELOG.md) following the [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format. Releases are versioned using [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

**When opening a PR with user-facing changes**, update the `Unreleased` section in `CHANGELOG.md` under the appropriate category:
- `Added` — New features
- `Changed` — Changes to existing functionality
- `Deprecated` — Features to be removed in the future
- `Removed` — Removed features
- `Fixed` — Bug fixes
- `Security` — Security patches and vulnerability fixes

Release notes are generated automatically from conventional commits using `git-cliff` when a version tag (`v*`) is pushed.

---

## 🧑‍💻 Code Review Process

Once you open a PR:

1. **Initial review** — Maintainers will review your PR within 1–3 business days
2. **Request changes** — If changes are requested, push new commits to the same branch; the PR updates automatically
3. **Approval** — Once approved, your PR will be merged
4. **Wave rewards** — If your work is part of a Wave, you'll earn contribution points toward rewards

Reviewing PRs is active feedback to help ensure quality. Don't hesitate to ask questions or discuss suggestions in the PR comments.

---

## 🚀 Expected Turnaround Time

- **PR reviews**: typically 1–3 business days for initial feedback
- **Wave issues**: may get faster triage during active Wave cycles
- **Urgent/security issues**: prioritized and reviewed within 24 hours when possible

---

## 📜 Code of Conduct

Please abide by the project's Code of Conduct to ensure a welcoming and respectful environment for all contributors:

👉 https://opensource.guide/code-of-conduct

---

## ❤️ Thank You

Thank you for contributing to Astera! We genuinely appreciate your time, ideas, and energy. Whether you're fixing a bug, improving documentation, building a new feature, or participating in the Wave Program, your work helps advance tokenized real-world assets on Stellar. 

Happy contributing! 🚀
