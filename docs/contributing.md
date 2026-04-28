# Contributing to Astera

Thank you for your interest in contributing to Astera! This document provides guidelines and instructions for contributing to the project.

## Development Setup

### Prerequisites

- Rust 1.75.0 or later
- Cargo
- wasm32-unknown-unknown target: `rustup target add wasm32-unknown-unknown`
- Stellar CLI: `cargo install --locked stellar-cli --features opt`
- Node.js 20+ (for frontend development)

### Clone and Build

```bash
git clone <repository-url>
cd astera
cargo build --workspace
```

## Running Tests

Run the full test suite:

```bash
cargo test --workspace
```

Run tests for a specific contract:

```bash
cargo test -p invoice
cargo test -p pool
```

## Running Benchmarks Locally

Benchmark the five key contract functions (deposit, repay_invoice, commit_to_invoice, create_invoice, mark_paid):

```bash
cargo bench --bench contract_benchmarks
```

Benchmark results are saved to `target/criterion/`. Each function's performance metrics include:

- Mean execution time
- Standard deviation
- Throughput measurements
- Historical comparison (if previous runs exist)

View detailed HTML reports:

```bash
open target/criterion/report/index.html
```

View previous CI benchmark artifacts from the [Actions tab](../../actions) in GitHub.

## Code Quality

### Formatting

Check code formatting:

```bash
cargo fmt --check --all
```

Auto-format code:

```bash
cargo fmt --all
```

### Linting

Run Clippy for lint checks:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

### Full CI Checks

Run all checks that CI will perform:

```bash
cargo check --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo bench --bench contract_benchmarks
```

## Contract Development

### Contract Structure

- `contracts/invoice/` - Invoice NFT contract
- `contracts/pool/` - Funding pool contract
- `benchmarks/` - Performance benchmarks

### Adding New Functions

1. Implement the function in the appropriate contract
2. Add unit tests in the same file
3. Add integration tests if needed
4. Update benchmarks if the function is performance-critical
5. Document the function with rustdoc comments

### Testing Guidelines

- Aim for 95%+ code coverage
- Test both success and failure cases
- Use descriptive test names: `test_<function>_<scenario>_<expected_result>`
- Use `#[should_panic(expected = "error message")]` for panic tests

Example:

```rust
#[test]
fn test_deposit_updates_balance() {
    // Test successful deposit
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_deposit_zero_amount_panics() {
    // Test validation
}
```

## Frontend Development

Navigate to the frontend directory:

```bash
cd frontend
npm install
npm run dev
```

The frontend runs on [http://localhost:3000](http://localhost:3000).

### Frontend Structure

- `app/` - Next.js pages and routes
- `components/` - React components
- `lib/` - Utilities, contract interfaces, state management

## Deployment

See [deployment.md](./deployment.md) for detailed testnet deployment instructions.

## Pull Request Process

1. Create a feature branch: `git checkout -b feat/your-feature-name`
2. Make your changes
3. Run all CI checks locally (see "Full CI Checks" above)
4. Commit with conventional commit messages: `feat:`, `fix:`, `docs:`, `test:`, etc.
5. Push your branch and open a pull request
6. Ensure all CI checks pass
7. Request review from maintainers

### Commit Message Format

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `test`: Test additions or changes
- `refactor`: Code refactoring
- `perf`: Performance improvements
- `ci`: CI/CD changes

Example:

```
feat(pool): add multi-investor co-funding support

Implements incremental commitment tracking allowing multiple
investors to fund a single invoice proportionally.

Closes #123
```

## Performance Benchmarking

### When to Benchmark

Add benchmarks for:
- Core contract functions (deposit, withdraw, fund, repay)
- Functions with complex calculations
- Functions called frequently in typical workflows

### Benchmark Best Practices

- Use realistic input data from actual test cases
- Use `black_box()` to prevent compiler optimizations
- Use `iter_batched()` for setup/teardown between iterations
- Document what each benchmark measures

Example:

```rust
fn bench_deposit(c: &mut Criterion) {
    c.bench_function("deposit", |b| {
        b.iter_batched(
            || setup_pool_env(),
            |(env, client, investor)| {
                client.deposit(&investor, &black_box(1_000_000_000))
            },
            criterion::BatchSize::SmallInput,
        )
    });
}
```

## CI/CD

### Continuous Integration

The CI pipeline runs on every push and pull request:

1. **Test Job**
   - Runs full test suite with timing
   - Warns if tests exceed 120 seconds
   - Runs benchmarks
   - Uploads benchmark results as artifacts

2. **Lint Job**
   - Checks code formatting
   - Runs Clippy lints

### Test Execution Time Monitoring

The CI tracks test execution time and warns if the test suite exceeds 120 seconds. This helps identify performance regressions early.

If you see the warning:
1. Check if new tests are inefficient
2. Consider mocking expensive operations
3. Review test setup/teardown overhead

## Getting Help

- Open an issue for bugs or feature requests
- Join our community discussions
- Review existing issues and PRs for context

## License

By contributing, you agree that your contributions will be licensed under the same license as the project.
