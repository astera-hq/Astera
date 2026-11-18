#!/bin/bash

set -euo pipefail

# TypeScript Bindings Generator for Astera Soroban Contracts
#
# Generates TypeScript type definitions / contract clients from the compiled
# contract WASM using `stellar contract bindings typescript`, and writes the
# resulting module to frontend/src/generated/<contract>.ts.
#
# Usage: ./scripts/gen-bindings.sh [contract...]
#   contract:  one or more contract names (default: all 14 contracts)
#
# Examples:
#   ./scripts/gen-bindings.sh                     # regenerate all bindings
#   ./scripts/gen-bindings.sh invoice             # regenerate just invoice
#   ./scripts/gen-bindings.sh invoice credit_score
#
# Requirements:
#   - Rust toolchain with the wasm32-unknown-unknown target
#   - stellar-cli (pinned in CI; install with: cargo install stellar-cli --locked)
#
# #1563: DEFAULT_CONTRACTS now covers all 14 contracts so running the script
# with no arguments regenerates bindings for the full contract suite.
#
# CI regenerates bindings and runs `git diff --exit-code` to keep them in sync
# with the contract source. See .github/workflows/ci.yml (bindings-check job)
# and CONTRIBUTING.md ("Regenerating contract bindings") for details.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

WASM_DIR="target/wasm32-unknown-unknown/release"
OUTPUT_DIR="frontend/src/generated"

# #1563: all 14 contracts — previously only 4 were listed here.
DEFAULT_CONTRACTS=(
  invoice
  pool
  credit_score
  share
  secondary_market
  tranche
  auction
  governance
  access_control
  compliance
  insurance
  arbitration
  oracle_registry
  referral
)

# Contracts to process: CLI args if given, otherwise the full default set.
if [ "$#" -gt 0 ]; then
  CONTRACTS=("$@")
else
  CONTRACTS=("${DEFAULT_CONTRACTS[@]}")
fi

command -v stellar >/dev/null 2>&1 || {
  echo "error: 'stellar' CLI not found. Install it with: cargo install stellar-cli --locked" >&2
  exit 1
}

mkdir -p "$OUTPUT_DIR"

TMP_ROOT="$(mktemp -d)"
trap 'rm -rf "$TMP_ROOT"' EXIT

for contract in "${CONTRACTS[@]}"; do
  echo "==> Building $contract (release wasm)"
  cargo build --target wasm32-unknown-unknown --release -p "$contract"

  wasm_path="$WASM_DIR/${contract}.wasm"
  if [ ! -f "$wasm_path" ]; then
    echo "error: expected wasm not found at $wasm_path" >&2
    exit 1
  fi

  echo "==> Generating TypeScript bindings for $contract"
  pkg_dir="$TMP_ROOT/$contract"
  stellar contract bindings typescript \
    --wasm "$wasm_path" \
    --output-dir "$pkg_dir" \
    --overwrite

  # The CLI emits a full NPM package; we only commit the generated module.
  cp "$pkg_dir/src/index.ts" "$OUTPUT_DIR/${contract}.ts"
  echo "==> Wrote $OUTPUT_DIR/${contract}.ts"
done

echo "Done. Generated bindings for: ${CONTRACTS[*]}"
