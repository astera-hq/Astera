# Contributing to Astera

Thank you for your interest in contributing to **Astera**! We welcome participation from developers of all experience levels, especially through the **Drips Wave Program**, a sprint‑based open source contribution model that rewards meaningful contributions.

---

## 🌊 The Drips Wave Program

The Drips Wave Program is a structured, sprint‑based initiative that:

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



## 🛠 Development Environment Setup
Before contributing, set up your environment as follows:

### Required Tools
- **Rust + Cargo** – Needed to build Soroban smart contracts
(install with `rustup`) 
- **Stellar CLI** – To deploy/test contracts locally 
- **Node.js** (v20+ recommended) – For frontend development 
- **Freighter Wallet** – Browser extension for signing transactions 

Install Instructions
- Rust & Cargo: https://rustup.rs/
- Stellar CLI: https://developers.stellar.org/docs/
- Node.js: https://nodejs.org/
- Freighter Wallet: https://freighter.app/

## 🧪 Running Soroban Contract Tests
Most smart contracts live in `contracts/`:
```bash
cd contracts
cargo test
```
You can also build the contract for deployment:
```bash
cargo build --target wasm32-unknown-unknown --release
```

## 💻 Running the Frontend
The frontend is built with Next.js and depends on environment variables like contract IDs:
```bash
cd frontend
cp .env.example .env.local
# Edit .env.local with contract and network settings
npm install
npm run dev
```
The app will start at `http://localhost:3000`. 

## 🧾 Branch Naming Guidelines
Please use the following format for feature or fix branches:
```text
feat/short‑description
fix/short‑description
```
Examples:
`feat/add‑invoice‑support`
`fix/api‑endpoint‑typo`

## 📜 Commit Message Format
We follow Conventional Commits:
```text
type(scope?): short description
```
Where the `type` is one of:
| Type       | Meaning                        |
| ---------- | ------------------------------ |
| `feat`     | New feature                    |
| `fix`      | Bug fix                        |
| `docs`     | Documentation changes          |
| `chore`    | Maintenance or tooling updates |
| `refactor` | Non‑functional code changes    |

Examples:
```text
feat(invoice): add due date validation
fix(pool): resolve withdraw edge case
docs: update API endpoint details
```

## ✔️ PR Checklist
Before opening a PR, make sure you:
- Linked an issue (e.g., Closes #123)
- Built the project locally
- All tests pass
- Code is formatted consistently
- Commit messages follow conventions above
- Your PR description explains the change clearly

## 🧑‍💻 Code Review Process

Once a PR is opened:
- Maintainers aim to review within 1–3 business days
- You may be asked to address requested changes
- Once approved, your PR will be merged
- Contributions during Wave cycles may earn points toward rewards


## 🧭 Expected Turnaround Time
- PR reviews typically take 2–5 days
- Urgent or Wave‑labeled issues may get faster triage

## 📜 Code of Conduct
Please abide by the project’s Code of Conduct to ensure a welcoming and respectful environment:

👉 https://opensource.guide/code-of-conduct

## ❤️ Thank You
Thank you for contributing! We appreciate your time, ideas, and energy — especially if you’re participating in the Wave Program and helping grow Astera’s ecosystem!
