# GitHub Actions Workflows

This document describes the GitHub Actions workflows used in the Astera project for continuous integration, testing, deployment, and releases.

## Overview

The project uses multiple workflows to automate the development lifecycle:

1. **CI/CD Pipeline** (`.github/workflows/ci.yml`) - Main workflow for building, testing, and deployment
2. **Release Workflow** (`.github/workflows/release.yml`) - Manual release creation

## CI/CD Pipeline (`ci.yml`)

### Triggers

- **Push events**: `main`, `develop` branches, and version tags (`v*`)
- **Pull requests**: Opened, synchronized, or closed events targeting `main` or `develop`

### Jobs

#### 1. Security Audit
- Runs security vulnerability scanning on Rust dependencies
- Uses `cargo-audit` to check for known security issues
- Runs on all workflow triggers

#### 2. Rust Contracts
- Matrix build for all three contracts: `invoice`, `pool`, `credit_score`
- **Steps**:
  - Format checking with `rustfmt`
  - Linting with `clippy`
  - Building for WebAssembly target
  - Running unit tests
  - Uploading WASM artifacts

#### 3. Frontend
- Builds and tests the frontend application
- **Steps**:
  - Node.js setup with caching
  - Dependency installation with `npm ci`
  - ESLint code quality checks
  - Production build
  - Artifact upload

#### 4. Dependency Scanning
- Scans frontend dependencies for vulnerabilities
- Uses `npm audit` with high severity threshold

#### 5. Integration Tests
- Runs comprehensive tests across all contracts
- Only executes on pull requests
- Downloads artifacts and runs workspace tests

#### 6. Deploy to Testnet (PR Merge)
- **Trigger**: When a PR is merged into `main` or `develop`
- **Steps**:
  - Downloads contract artifacts
  - Installs Stellar CLI
  - Deploys all contracts to Stellar Testnet
  - Comments on the PR with deployment information
  - Provides deployment summary

#### 7. Deploy to Testnet (Main Push)
- **Trigger**: Direct push to `main` branch
- Similar to PR deployment but without PR comments

#### 8. Create Release
- **Trigger**: Version tag push (`v*`)
- **Steps**:
  - Creates GitHub release with comprehensive notes
  - Uploads contract WASM files as release assets
  - Uploads frontend build as release asset

## Release Workflow (`release.yml`)

### Manual Release Creation

This workflow allows manual release creation through GitHub's workflow dispatch:

- **Version input**: Specify release version (e.g., `1.0.0`)
- **Tag creation**: Option to automatically create git tag
- **Full build and test**: Ensures release quality
- **Asset uploads**: Includes all contracts and frontend build

### Usage

1. Go to Actions tab in GitHub
2. Select "Release Workflow"
3. Click "Run workflow"
4. Enter version number
5. Choose whether to create a tag
6. Click "Run workflow"

## Required Secrets

The workflows require the following GitHub repository secrets:

### `TESTNET_DEPLOYER_KEY`
- Stellar Testnet deployer account secret key
- Used for contract deployment to testnet
- Must have sufficient XLM balance for deployment fees

### Environment Protection Rules

- **testnet**: Required for deployment jobs
- **production**: Required for release creation
- Configure approvers and wait periods as needed

## Artifact Management

### Build Artifacts
- Contract WASM files: Retained for 30 days
- Frontend build: Retained for 30 days
- Automatically downloaded by deployment jobs

### Release Assets
- Contract WASM files attached to releases
- Frontend build ZIP file
- Permanent storage with release

## Deployment Process

### PR Merge Deployment
1. PR passes all checks and is merged
2. Integration tests run
3. Contracts deployed to testnet
4. PR updated with deployment information
5. Deployment summary added to workflow run

### Main Branch Deployment
1. Code pushed to main branch
2. Contracts built and deployed
3. No PR comments (direct deployment)

### Release Deployment
1. Version tag pushed or manual release created
2. Full build and test cycle
3. GitHub release created
4. Assets uploaded to release

## Monitoring and Notifications

### Workflow Status
- All failures are reported in the Actions tab
- PR status checks indicate build/deployment status
- Deployment summaries available in workflow runs

### PR Comments
- Automatic deployment information posted on merged PRs
- Includes contract IDs and deployment links

## Best Practices

### Branch Strategy
- `main`: Production-ready code
- `develop`: Integration branch for features
- Feature branches: Create PRs to `develop`

### Release Process
1. Ensure all tests pass
2. Create version tag (`v1.0.0`) or use manual workflow
3. Review automated release notes
4. Test release artifacts

### Security
- Regular security audits run automatically
- Dependency scanning for vulnerabilities
- Environment protection rules for deployments

## Troubleshooting

### Common Issues

#### Deployment Failures
- Check `TESTNET_DEPLOYER_KEY` secret
- Verify sufficient XLM balance
- Review Stellar CLI command outputs

#### Build Failures
- Check Rust version compatibility
- Verify Node.js version
- Review dependency conflicts

#### Test Failures
- Examine test logs in workflow runs
- Check contract integration points
- Verify test environment setup

### Debugging Steps

1. **Review Workflow Logs**: Check individual step outputs
2. **Download Artifacts**: Examine build outputs locally
3. **Local Testing**: Replicate issues in development environment
4. **Check Secrets**: Verify all required secrets are configured

## Configuration Files

### Workflow Configuration
- Location: `.github/workflows/`
- Main workflow: `ci.yml`
- Release workflow: `release.yml`

### Environment Files
- Rust: `Cargo.toml` (workspace and individual contracts)
- Node.js: `package.json` (frontend)
- GitHub: Environment protection rules and secrets

## Future Enhancements

Potential improvements to consider:

1. **Multi-network Support**: Mainnet deployment workflow
2. **Automated Versioning**: Semantic versioning based on commits
3. **Performance Testing**: Contract performance benchmarks
4. **Slack/Discord Notifications**: Deployment status notifications
5. **Rollback Mechanism**: Automated rollback on deployment failures
6. **Contract Verification**: Automated contract source verification
7. **Documentation Generation**: Auto-generated API docs with releases
