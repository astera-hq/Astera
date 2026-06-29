#!/usr/bin/env bash
# #634: Quick smoke-test for testnet deployments.
# Iterates all contract IDs from env vars and calls version() on each.
# Exits non-zero if any contract call fails.
#
# Usage:
#   export INVOICE_CONTRACT_ID=...
#   export POOL_CONTRACT_ID=...
#   export CREDIT_SCORE_CONTRACT_ID=...
#   export GOVERNANCE_CONTRACT_ID=...
#   bash scripts/smoke-test-testnet.sh
#
# Environment:
#   STELLAR_RPC_URL          — Soroban RPC endpoint (default: https://soroban-testnet.stellar.org)
#   STELLAR_NETWORK_PASSPHRASE — network passphrase (default: Test SDF Network ; September 2015)

set -eu

RPC_URL="${STELLAR_RPC_URL:-https://soroban-testnet.stellar.org}"
PASSPHRASE="${STELLAR_NETWORK_PASSPHRASE:-Test SDF Network ; September 2015}"
FAILED=0

CONTRACTS=(
  "INVOICE_CONTRACT_ID"
  "POOL_CONTRACT_ID"
  "CREDIT_SCORE_CONTRACT_ID"
  "GOVERNANCE_CONTRACT_ID"
)

echo "==> Smoke-testing contracts on testnet"
echo "    RPC URL: $RPC_URL"
echo ""

for VAR in "${CONTRACTS[@]}"; do
  ID="${!VAR:-}"
  if [ -z "$ID" ]; then
    echo "  [SKIP] $VAR is not set"
    continue
  fi

  echo -n "  $VAR ($ID) ... "
  OUTPUT=$(stellar contract invoke \
    --id "$ID" \
    --rpc-url "$RPC_URL" \
    --network-passphrase "$PASSPHRASE" \
    -- \
    version 2>&1) && RC=$? || RC=$?

  if [ "$RC" -eq 0 ] && [ -n "$OUTPUT" ]; then
    echo "OK — version: $OUTPUT"
  else
    echo "FAIL (exit=$RC)"
    echo "    stderr: $OUTPUT"
    FAILED=$((FAILED + 1))
  fi
done

echo ""
if [ "$FAILED" -eq 0 ]; then
  echo "==> All contracts passed smoke test"
else
  echo "==> $FAILED contract(s) FAILED smoke test"
fi
exit "$FAILED"
