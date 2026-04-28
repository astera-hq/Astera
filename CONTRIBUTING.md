# Contributing to Astera

Thank you for your interest in contributing to **Astera**! We welcome participation from developers of all experience levels, especially through the **Drips Wave Program**, a sprint-based open source contribution model that rewards meaningful contributions.

---

## 🌊 The Drips Wave Program

The Drips Wave Program is a structured, sprint-based initiative that:

- Defines a short contribution cycle (typically ~1 week)
- Has a shared reward pool tied to merged pull requests
- Tracks contribution points for transparent reward distribution
- Enables contributors to earn based on impact, not just activity

For more about Waves and how they work, visit the official docs: https://docs.drips.network/wave.

---

## 🧭 How to Find and Claim an Issue

1. Go to the **Issues** tab of this repository
2. Look for labels such as:
   - `good first issue`
   - `help wanted`
   - `wave`
3. Comment on the issue you want to work on using:

   ```text
   I'd like to work on this
   ```

   This lets maintainers know you're interested and prevents duplication of effort.

4. Wait for a maintainer to tag you or confirm assignment

---

## 🛠 Development Environment Setup

Before contributing, set up your environment as follows. If you are using Windows, please follow the [Windows/WSL2 Setup Guide](docs/windows-wsl-setup.md).

### Required Tools

| Tool | Purpose | Install |
| --- | --- | --- |
| **Rust + Cargo** | Build Soroban smart contracts | https://rustup.rs/ |
| **Stellar CLI** | Deploy/test contracts locally | https://developers.stellar.org/docs/ |
| **Node.js** (v20+) | Frontend development | https://nodejs.org/ |
| **Freighter Wallet** | Browser extension for signing transactions | https://freighter.app/ |

### Step-by-step Setup

```bash
# 1. Clone the repository
git clone https://github.com/<your-username>/Astera.git
cd Astera

# 2. Add upstream remote (for keeping your fork in sync)
git remote add upstream https://github.com/astera-hq/Astera.git

# 3. Install the Soroban WASM target
rustup target add wasm32-unknown-unknown

# 4. Build and test the smart contracts
cd contracts
cargo test
cargo build --target wasm32-unknown-unknown --release

# 5. Install frontend dependencies
cd ../frontend
cp .env.example .env.local   # Edit with your contract IDs & network settings
npm install

# 6. Start the development server
npm run dev
# → http://localhost:3000
```

---

## 🧪 Testing Requirements

### Smart Contracts (Rust / Soroban)

All contract changes **must** include or update tests.

```bash
cd contracts
cargo test
```

Build for deployment:

```bash
cargo build --target wasm32-unknown-unknown --release
```

- Every public function must have at least one test covering the happy path.
- Edge cases (e.g. zero amounts, unauthorized callers, duplicate operations) should be covered.
- Tests live in `mod test` blocks at the bottom of each contract's `lib.rs`.

### Frontend (TypeScript / Next.js)

```bash
cd frontend
npm run lint      # ESLint
npm run build     # Type-check + production build
```

- Ensure `npm run lint` passes with no errors before opening a PR.
- Ensure `npm run build` succeeds (catches TypeScript errors).

---

## 🎨 Code Style Guidelines

### Rust (Smart Contracts)

| Rule | Detail |
| --- | --- |
| **Formatter** | `cargo fmt` — run before every commit |
| **Linter** | `cargo clippy` — no warnings allowed |
| **`#![no_std]`** | All contracts must be `no_std` compatible |
| **Naming** | `snake_case` for functions/variables, `PascalCase` for types/enums |
| **Error handling** | Use `panic!("descriptive message")` for contract errors; keep messages concise |
| **Events** | Use `symbol_short!` for event topics; publish events for all state-changing operations |
| **Storage keys** | Define all keys in the `DataKey` enum |
| **Auth** | Always call `.require_auth()` on the relevant `Address` before mutating state |
| **Constants** | Use `const` for fixed values (e.g. `DEFAULT_YIELD_BPS`, `SECS_PER_YEAR`) |

### TypeScript (Frontend)

| Rule | Detail |
| --- | --- |
| **Formatter** | Prettier (runs automatically via `lint-staged` on commit) |
| **Linter** | ESLint with `eslint-config-next` and `@typescript-eslint` |
| **Framework** | Next.js 15 App Router — use `'use client'` directive only when needed |
| **State** | Zustand for global state (`lib/store.ts`); React state for local UI |
| **Styling** | Tailwind CSS utility classes; follow existing `brand-*` design tokens |
| **Naming** | `PascalCase` for components, `camelCase` for functions/variables |
| **Imports** | Use `@/` path alias (e.g. `@/lib/store`, `@/components/Navbar`) |
| **Types** | Define shared types in `lib/types.ts`; prefer `interface` over `type` for objects |
| **Contract calls** | All Soroban interaction builders go in `lib/contracts.ts` |
| **SDK helpers** | Conversion/formatting utilities go in `lib/stellar.ts` |

### General

- Keep files focused — one component or module per file.
- Avoid adding unnecessary dependencies.
- Do not commit `.env.local` or secret keys.
- Use English for all code comments and documentation.

---

## 🧾 Branch Naming Guidelines

Please use the following format for feature or fix branches:

```text
feat/short-description
fix/short-description
docs/short-description
```

Examples:

- `feat/add-invoice-support`
- `fix/api-endpoint-typo`
- `docs/update-api-docs`

---

## 📜 Commit Message Format

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```text
type(scope?): short description
```

Where the `type` is one of:

| Type | Meaning |
| --- | --- |
| `feat` | New feature |
| `fix` | Bug fix |
| `docs` | Documentation changes |
| `chore` | Maintenance or tooling updates |
| `refactor` | Non-functional code changes |
| `test` | Adding or updating tests |
| `style` | Formatting, whitespace (no logic changes) |
| `ci` | CI/CD changes |
| `build` | Build system changes |
| `perf` | Performance improvements |
| `revert` | Revert previous changes |

Examples:

```text
feat(invoice): add due date validation
fix(pool): resolve withdraw edge case
docs: update API endpoint details
test(pool): add co-funding edge case tests
```

## 📋 Changelog

This project maintains a `CHANGELOG.md` following the [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format. Releases are versioned using [Semantic Versioning](https://semver.org/spec/v2.0.0.html). When opening a PR that introduces a user-facing change, please ensure the change is reflected in the `Unreleased` section of `CHANGELOG.md` under the appropriate category (`Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, or `Security`).

Release notes are generated automatically from conventional commits using `git-cliff` when a version tag (`v*`) is pushed.

---

## 📋 Pull Request Process

### Before Opening a PR

1. **Sync your fork** with the latest upstream `main`:
   ```bash
   git fetch upstream
   git rebase upstream/main
   ```
2. **Run all checks locally**:
   ```bash
   # Contracts
   cd contracts && cargo fmt --check && cargo clippy && cargo test

   # Frontend
   cd frontend && npm run lint && npm run build
   ```
3. **Ensure your branch is clean** — no unrelated changes, no merge commits.

### PR Checklist

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

### PR Description Template

```markdown
## Summary
Brief description of the changes.

## Related Issue
Closes #<issue-number>

## Changes
- Change 1
- Change 2

## Testing
Describe how you tested your changes.

## Screenshots (if applicable)
```

---

## 🧑‍💻 Code Review Process

Once a PR is opened:

- Maintainers aim to review within 1–3 business days
- You may be asked to address requested changes
- Once approved, your PR will be merged
- Contributions during Wave cycles may earn points toward rewards

---

## 🧭 Expected Turnaround Time

- PR reviews typically take 2–5 days
- Urgent or Wave-labeled issues may get faster triage

---

## 📜 Code of Conduct

Please abide by the project's Code of Conduct to ensure a welcoming and respectful environment:

👉 https://opensource.guide/code-of-conduct

---

## ❤️ Thank You

Thank you for contributing! We appreciate your time, ideas, and energy — especially if you're participating in the Wave Program and helping grow Astera's ecosystem!
