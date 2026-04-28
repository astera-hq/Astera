# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- PR title format enforcement via `amannn/action-semantic-pull-request@v5`.
- Automated release notes generation on tag push via `git-cliff`.
- `CHANGELOG.md` following Keep a Changelog format.

## [0.5.0] - 2026-04-27

### Added
- Minimum deposit requirements for pool investors.
- Revenue tracking and yield distribution improvements.
- Withdrawal limits for investor positions.
- Co-fund share transfer functionality.
- Invoice dispute UI with CSV import and PDF export.
- Real-time validation on invoice creation form.
- Explorer links for on-chain transactions.
- Mock service for local development and testing.
- Grace period override for invoice default marking.
- GitHub issue and pull request templates.
- Local development setup guide.

### Changed
- Replaced `unwrap`/`panic` with structured `PoolError` across pool contract.
- Improved event emission for pool operations and collateral updates.

### Fixed
- Multiple bounty contributions addressing issues #201, #203, #205, #238, #246.

## [0.4.0] - 2026-04-26

### Added
- Analytics dashboard for SMEs and investors.
- SSE Events polling service for real-time contract updates.
- Emergency pause functionality for critical contract operations.
- Grace period before marking invoices as defaulted.
- Access control for invoice status transitions.
- Reentrancy guard on pool contract.
- ConfirmActionModal for destructive admin actions.
- Invoice search and filter functionality on the dashboard.
- Role-based 5-step onboarding tour for SMEs and investors.
- `/api/health` monitoring endpoint with alert rules and webhook.
- Reward-per-share yield distribution and `claim_yield`.
- Error code namespaces for contracts and frontend error mapping.

## [0.3.0] - 2026-04-23

### Added
- Theme toggle, improved wallet UX, portfolio dashboard, and invoice notifications.
- Fuzz testing, integration tests, gas optimizations, and grace period logic.
- Multi-currency support, KYC whitelist, transaction export, and property-based tests.
- Collateral requirement enforcement for high-value invoices.
- Batch operations and dashboard detail improvements.
- Invoice expiration after configurable unfunded duration.
- Configurable maximum invoice amount enforcement.
- Yield change cooldown and maximum step controls.
- Technical architecture documentation.
- API reference documentation for contract methods.

### Fixed
- Resolved issues #56, #61, #64, and #114.

## [0.2.0] - 2026-04-02

### Added
- Complete dispute resolution and reconciliation protocol.
- Circuit breaker pattern for emergency protocol shutdown.
- Invoice factoring fee structure.
- Investor share token mechanics.
- On-chain credit scoring mechanism calculated from repayment history.
- Docker-based development environment.
- Storage optimization ADR and oracle environment configuration.

## [0.1.0] - 2026-03-25

### Added
- Initial Rust workspace with Soroban resolver and core smart contracts (invoice, pool, credit_score, share).
- Frontend Next.js 14 scaffold with Stellar SDK, TypeScript, and Tailwind CSS.
- Landing page with hero section, stats, feature grid, and how-it-works.
- SME dashboard with invoice list, quick stats, and credit score panel.
- Investor page with pool overview, deposit, and withdraw flows.
- Invoice creation form with mint preview and cost estimate.
- Invoice detail page with status timeline and owner actions.
- WalletConnect component with Freighter API integration.
- PoolStats component with utilization bar and APY display.
- CreditScore component calculated from invoice repayment history.
- InvoiceCard component with status badges and due date countdown.
- Partial invoice co-funding by multiple investors.
- Transaction history page with on-chain event logs.
- Mobile-responsive navigation drawer.
- Search, filter, and sort on SME dashboard.
- ESLint, Prettier, and Husky for frontend code quality.
- Next.js v15 upgrade, stellar-sdk v14, and freighter-api v6 compatibility.
- README with project overview, setup guide, and deployment instructions.
- Contributing guidelines.

[Unreleased]: https://github.com/astera-hq/Astera/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/astera-hq/Astera/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/astera-hq/Astera/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/astera-hq/Astera/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/astera-hq/Astera/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/astera-hq/Astera/releases/tag/v0.1.0
