#!/usr/bin/env sh
# Initialize all 14 Astera contracts in dependency order after deployment.
#
# Usage:
#   export DEPLOYER_KEY=<secret_key>  # or use --source <key_alias>
#   export NETWORK=testnet             # or mainnet / standalone
#   export RPC_URL=<soroban_rpc_url>   # optional, overrides --rpc-url
#
#   # Source contract IDs (either set these or source a deployed.env file)
#   export ACCESS_CONTROL_CONTRACT_ID=...
#   export ARBITRATION_CONTRACT_ID=...
#   export AUCTION_CONTRACT_ID=...
#   export COMPLIANCE_CONTRACT_ID=...
#   export CREDIT_SCORE_CONTRACT_ID=...
#   export GOVERNANCE_CONTRACT_ID=...
#   export INSURANCE_CONTRACT_ID=...
#   export INVOICE_CONTRACT_ID=...
#   export ORACLE_REGISTRY_CONTRACT_ID=...
#   export POOL_CONTRACT_ID=...
#   export REFERRAL_CONTRACT_ID=...
#   export SECONDARY_MARKET_CONTRACT_ID=...
#   export SHARE_CONTRACT_ID=...
#   export TRANCHE_CONTRACT_ID=...
#
#   # #1042: optional — who the access_control SuperAdmin role starts with.
#   # Defaults to a single-signer "multisig" of just ADMIN_ADDRESS so a
#   # fresh deployment can adopt access_control without extra setup; ops
#   # MUST raise this to a real M-of-N signer set (via AddSigner/SetThreshold
#   # proposals) before relying on it as the actual security boundary.
#   export SUPER_ADMIN_SIGNERS='["G...","G..."]'   # optional, JSON array
#   export SUPER_ADMIN_THRESHOLD=1                  # optional
#
#   sh scripts/init-contracts.sh

set -eu

: "${DEPLOYER_KEY:?DEPLOYER_KEY not set}"
: "${NETWORK:=testnet}"
: "${ACCESS_CONTROL_CONTRACT_ID:?ACCESS_CONTROL_CONTRACT_ID not set}"
: "${ARBITRATION_CONTRACT_ID:?ARBITRATION_CONTRACT_ID not set}"
: "${AUCTION_CONTRACT_ID:?AUCTION_CONTRACT_ID not set}"
: "${COMPLIANCE_CONTRACT_ID:?COMPLIANCE_CONTRACT_ID not set}"
: "${CREDIT_SCORE_CONTRACT_ID:?CREDIT_SCORE_CONTRACT_ID not set}"
: "${GOVERNANCE_CONTRACT_ID:?GOVERNANCE_CONTRACT_ID not set}"
: "${INSURANCE_CONTRACT_ID:?INSURANCE_CONTRACT_ID not set}"
: "${INVOICE_CONTRACT_ID:?INVOICE_CONTRACT_ID not set}"
: "${ORACLE_REGISTRY_CONTRACT_ID:?ORACLE_REGISTRY_CONTRACT_ID not set}"
: "${POOL_CONTRACT_ID:?POOL_CONTRACT_ID not set}"
: "${REFERRAL_CONTRACT_ID:?REFERRAL_CONTRACT_ID not set}"
: "${SECONDARY_MARKET_CONTRACT_ID:?SECONDARY_MARKET_CONTRACT_ID not set}"
: "${SHARE_CONTRACT_ID:?SHARE_CONTRACT_ID not set}"
: "${TRANCHE_CONTRACT_ID:?TRANCHE_CONTRACT_ID not set}"
: "${ADMIN_ADDRESS:?ADMIN_ADDRESS not set}"
: "${USDC_TOKEN_ID:?USDC_TOKEN_ID not set}"
: "${SUPER_ADMIN_SIGNERS:="[\"$ADMIN_ADDRESS\"]"}"
: "${SUPER_ADMIN_THRESHOLD:=1}"

# Optional tuning knobs for contracts that take stake/tranche parameters.
: "${ARBITRATION_MIN_STAKE:=1000000000}"
: "${ORACLE_MIN_STAKE:=1000000000}"
# Senior/Junior tranche share tokens. In production these should be distinct
# tranche token contract addresses; they default to the share token so a fresh
# deploy runs without extra setup. Override per environment as needed.
: "${TRANCHE_SENIOR_SHARE_TOKEN_ID:=$SHARE_CONTRACT_ID}"
: "${TRANCHE_JUNIOR_SHARE_TOKEN_ID:=$SHARE_CONTRACT_ID}"
: "${TRANCHE_CONFIG:={senior_target_yield_bps: 1000, senior_advance_rate_bps: 5000, junior_first_loss_bps: 2000}}"

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

# Order: access_control -> share -> compliance -> oracle_registry -> invoice ->
# pool -> credit_score -> governance -> secondary_market -> auction -> insurance
# -> referral -> arbitration -> tranche

echo "=== Initializing contracts (all 14) ==="

invoke "$ACCESS_CONTROL_CONTRACT_ID" \
  initialize \
  --super_admin_signers "$SUPER_ADMIN_SIGNERS" \
  --super_admin_threshold "$SUPER_ADMIN_THRESHOLD" \
  --proposal_expiry_secs 604800

invoke "$SHARE_CONTRACT_ID" \
  initialize \
  --admin "$ADMIN_ADDRESS" \
  --decimals 7 \
  --name '"Astera Share Token"' \
  --symbol '"ASTR"'

invoke "$COMPLIANCE_CONTRACT_ID" \
  initialize \
  --admin "$ADMIN_ADDRESS"

invoke "$ORACLE_REGISTRY_CONTRACT_ID" \
  initialize \
  --admin "$ADMIN_ADDRESS" \
  --stake_token "$USDC_TOKEN_ID" \
  --min_stake "$ORACLE_MIN_STAKE"

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

invoke "$SECONDARY_MARKET_CONTRACT_ID" \
  initialize \
  --admin "$ADMIN_ADDRESS" \
  --pool_contract "$POOL_CONTRACT_ID"

invoke "$AUCTION_CONTRACT_ID" \
  initialize \
  --admin "$ADMIN_ADDRESS" \
  --pool "$POOL_CONTRACT_ID"

invoke "$INSURANCE_CONTRACT_ID" \
  initialize \
  --admin "$ADMIN_ADDRESS" \
  --pool_contract "$POOL_CONTRACT_ID" \
  --invoice_contract "$INVOICE_CONTRACT_ID"

invoke "$REFERRAL_CONTRACT_ID" \
  initialize \
  --admin "$ADMIN_ADDRESS" \
  --pool "$POOL_CONTRACT_ID"

invoke "$ARBITRATION_CONTRACT_ID" \
  initialize \
  --admin "$ADMIN_ADDRESS" \
  --invoice_contract "$INVOICE_CONTRACT_ID" \
  --stake_token "$USDC_TOKEN_ID" \
  --min_stake "$ARBITRATION_MIN_STAKE"

invoke "$TRANCHE_CONTRACT_ID" \
  initialize \
  --admin "$ADMIN_ADDRESS" \
  --token "$USDC_TOKEN_ID" \
  --senior_share_token "$TRANCHE_SENIOR_SHARE_TOKEN_ID" \
  --junior_share_token "$TRANCHE_JUNIOR_SHARE_TOKEN_ID" \
  --config "$TRANCHE_CONFIG"

# #1042: adopt access_control as an additional, additive admin path on every
# contract that supports it. The legacy ADMIN_ADDRESS-gated path above stays
# fully functional on all of them — this does not disable or replace it.
echo "=== Wiring access_control into invoice/pool/credit_score/governance/compliance/oracle_registry/referral ==="

invoke "$INVOICE_CONTRACT_ID" \
  set_access_control \
  --admin "$ADMIN_ADDRESS" \
  --access_control "$ACCESS_CONTROL_CONTRACT_ID"

invoke "$POOL_CONTRACT_ID" \
  set_access_control \
  --admin "$ADMIN_ADDRESS" \
  --access_control "$ACCESS_CONTROL_CONTRACT_ID"

invoke "$CREDIT_SCORE_CONTRACT_ID" \
  set_access_control \
  --admin "$ADMIN_ADDRESS" \
  --access_control "$ACCESS_CONTROL_CONTRACT_ID"

invoke "$GOVERNANCE_CONTRACT_ID" \
  set_access_control \
  --caller "$ADMIN_ADDRESS" \
  --access_control "$ACCESS_CONTROL_CONTRACT_ID"

invoke "$COMPLIANCE_CONTRACT_ID" \
  set_access_control \
  --admin "$ADMIN_ADDRESS" \
  --access_control "$ACCESS_CONTROL_CONTRACT_ID"

invoke "$ORACLE_REGISTRY_CONTRACT_ID" \
  set_access_control \
  --admin "$ADMIN_ADDRESS" \
  --access_control "$ACCESS_CONTROL_CONTRACT_ID"

invoke "$REFERRAL_CONTRACT_ID" \
  set_access_control \
  --admin "$ADMIN_ADDRESS" \
  --access_control "$ACCESS_CONTROL_CONTRACT_ID"

echo "=== All contracts initialized successfully ==="
