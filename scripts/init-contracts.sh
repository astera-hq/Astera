#!/usr/bin/env sh
# Initialize all 5 Astera contracts in dependency order after deployment.
#
# Usage:
#   export DEPLOYER_KEY=<secret_key>  # or use --source <key_alias>
#   export NETWORK=testnet             # or mainnet / standalone
#   export RPC_URL=<soroban_rpc_url>   # optional, overrides --rpc-url
#
#   # Source contract IDs (either set these or source a deployed.env file)
#   export SHARE_CONTRACT_ID=...
#   export INVOICE_CONTRACT_ID=...
#   export POOL_CONTRACT_ID=...
#   export CREDIT_SCORE_CONTRACT_ID=...
#   export GOVERNANCE_CONTRACT_ID=...
#
#   sh scripts/init-contracts.sh

set -eu

: "${DEPLOYER_KEY:?DEPLOYER_KEY not set}"
: "${NETWORK:=testnet}"
: "${SHARE_CONTRACT_ID:?SHARE_CONTRACT_ID not set}"
: "${INVOICE_CONTRACT_ID:?INVOICE_CONTRACT_ID not set}"
: "${POOL_CONTRACT_ID:?POOL_CONTRACT_ID not set}"
: "${CREDIT_SCORE_CONTRACT_ID:?CREDIT_SCORE_CONTRACT_ID not set}"
: "${GOVERNANCE_CONTRACT_ID:?GOVERNANCE_CONTRACT_ID not set}"
: "${ADMIN_ADDRESS:?ADMIN_ADDRESS not set}"
: "${USDC_TOKEN_ID:?USDC_TOKEN_ID not set}"

STELLAR_ARGS="--source $DEPLOYER_KEY --network $NETWORK"
if [ -n "${RPC_URL:-}" ]; then
  STELLAR_ARGS="$STELLAR_ARGS --rpc-url $RPC_URL"
fi

invoke() {
  local contract_id="$1"
  shift
  echo "==> initialize $1 ($contract_id)..."
  if ! stellar contract invoke --id "$contract_id" $STELLAR_ARGS -- "$@" 2>&1; then
    echo "ERROR: Failed to initialize $1. Check if already initialized." >&2
    return 1
  fi
}

# Order: share -> invoice -> pool -> credit_score -> governance

echo "=== Initializing contracts (order: share -> invoice -> pool -> credit_score -> governance) ==="

invoke "$SHARE_CONTRACT_ID" \
  initialize \
  --admin "$ADMIN_ADDRESS" \
  --decimals 7 \
  --name '"Astera Share Token"' \
  --symbol '"ASTR"'

invoke "$INVOICE_CONTRACT_ID" \
  initialize \
  --admin "$ADMIN_ADDRESS" \
  --pool "$POOL_CONTRACT_ID" \
  --max_invoice_amount 10000000000000 \
  --expiration_duration_secs 2592000 \
  --grace_period_days 30

invoke "$POOL_CONTRACT_ID" \
  initialize \
  --admin "$ADMIN_ADDRESS" \
  --initial_token "$USDC_TOKEN_ID" \
  --initial_share_token "$SHARE_CONTRACT_ID" \
  --invoice_contract "$INVOICE_CONTRACT_ID"

invoke "$CREDIT_SCORE_CONTRACT_ID" \
  initialize \
  --admin "$ADMIN_ADDRESS" \
  --invoice_contract "$INVOICE_CONTRACT_ID" \
  --pool_contract "$POOL_CONTRACT_ID"

invoke "$GOVERNANCE_CONTRACT_ID" \
  initialize \
  --admin "$ADMIN_ADDRESS" \
  --share_token "$SHARE_CONTRACT_ID" \
  --voting_period_secs 604800 \
  --quorum_bps 1000 \
  --pass_bps 5100 \
  --execution_delay_secs 86400 \
  --min_share_balance 10000000

echo "=== All contracts initialized successfully ==="
