# Security Policy

## Overview

Astera takes security seriously. This document outlines our security practices, how to report vulnerabilities, and how security auditing is integrated into our development process.

Operational incident handling procedures live in the
[Disaster Recovery Runbook](docs/disaster-recovery.md). Use that runbook for
contract exploits, admin key incidents, RPC outages, and other production
recovery scenarios.

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| Testnet | :white_check_mark: |
| Mainnet | :x: (pending audit) |

## Security Auditing

### Automated Security Checks

Our CI/CD pipeline includes the following automated security checks:

1. **cargo-audit** - Scans Rust dependencies for known vulnerabilities
   - Runs on every PR and push to main
   - Checks against RustSec Advisory Database

2. **Clippy** - Rust linter with security-focused lints
   - Enforces `#![no_std]` compatibility
   - Checks for unsafe code patterns
   - Runs with `-D warnings` (deny all warnings)

3. **npm audit** - Scans Node.js dependencies
   - Fails on high/critical severity vulnerabilities
   - Runs for frontend dependencies

### Static Analysis

The following static analysis tools are used:

| Tool | Purpose | Frequency |
|------|---------|-----------|
| cargo-clippy | Rust linting | Every PR |
| cargo-audit | Dependency scanning | Every PR |
| npm audit | Node.js dependency scanning | Every PR |
| rustfmt | Code formatting enforcement | Every PR |

### Security Best Practices for Contracts

All smart contracts follow these security practices:

1. **Authentication**: All state-changing operations require `.require_auth()` on the caller
2. **No unsafe code**: Contracts use `#![no_std]` and avoid unsafe blocks
3. **Integer overflow protection**: Enabled in release profile
4. **Error handling**: Explicit error messages for all failure cases
5. **Events**: All state changes emit events for transparency
6. **TTL management**: Proper storage TTL handling to prevent data loss

### Access Control Matrix

> See the full [Authorization Matrix](./docs/authorization-matrix.md) for a complete cross-contract call table and auditor checklist.

| Operation | Invoice Contract | Pool Contract | Credit Score |
|-----------|-----------------|---------------|--------------|
| Initialize | Admin | Admin | Admin |
| Create Invoice | Any (authenticated) | - | - |
| Fund Invoice | Pool only | Investors | - |
| Mark Paid | Owner/Pool/Admin | - | - |
| Mark Defaulted | Pool only | - | - |
| Add Token | - | Admin | - |
| Set Oracle | Admin | - | - |
| Record Payment | - | - | Pool only |

## Reporting a Vulnerability

If you discover a security vulnerability, please follow these steps:

1. **Do NOT** open a public issue
2. Email security details to: [security contact to be added]
3. Include:
   - Detailed description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

We will:
- Acknowledge receipt within 48 hours
- Provide a timeline for a fix within 5 business days
- Keep you updated on progress
- Credit you in the advisory (with your permission)

## Security Checklist for Deployment

Before any deployment:

- [ ] All tests pass
- [ ] cargo-audit shows no vulnerabilities
- [ ] Clippy shows no warnings
- [ ] Code review by at least one maintainer
- [ ] Security review for contract changes
- [ ] Events emitted for all state changes
- [ ] TTL properly configured for all storage

## Regular Security Audits

Security audits are scheduled:
- **Weekly**: Automated dependency scanning (cargo-audit, npm audit)
- **Every PR**: Static analysis and linting
- **Monthly**: Manual review of contract changes
- **Quarterly**: Third-party security audit (planned for mainnet)

## Security Resources

- [Disaster Recovery Runbook](docs/disaster-recovery.md)
- [RustSec Advisory Database](https://rustsec.org/)
- [Soroban Security Best Practices](https://soroban.stellar.org/docs/)
- [Stellar Security Guidelines](https://developers.stellar.org/docs/)

## License

Security issues found in this codebase are subject to responsible disclosure. See our [Code of Conduct](../CONTRIBUTING.md) for details.
