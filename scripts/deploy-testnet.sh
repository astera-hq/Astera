#!/usr/bin/env sh
# Deploy all 5 Astera contracts to Stellar testnet in dependency order.
#
# Usage:
#   export DEPLOYER_KEY=<secret_key>   # required
#   export NETWORK=testnet             # optional, defaults to testnet
#   export RPC_URL=<soroban_rpc_url>   # optional, overrides --rpc-url
#   export FUND_WITH_FRIENDBOT=true    # optional, fund deployer if balance is low
#
#   sh scripts/deploy-testnet.sh
#
# Output: contract IDs printed to stdout and written to deployed-testnet.env

set -eu

: "${DEPLOYER_KEY:?DEPLOYER_KEY not set}"
: "${NETWORK:=testnet}"

STELLAR_ARGS="--source $DEPLOYER_KEY --network $NETWORK"
if [ -n "${RPC_URL:-}" ]; then
  STELLAR_ARGS="$STELLAR_ARGS --rpc-url $RPC_URL"
fi

OUTPUT_FILE="${OUTPUT_FILE:-deployed-testnet.env}"
WASM_DIR="/app/target/wasm32-unknown-unknown/release"

if [ ! -d "$WASM_DIR" ]; then
  echo "ERROR: WASM directory not found at $WASM_DIR. Build contracts first:" >&2
  echo "  cargo build --target wasm32-unknown-unknown --release" >&2
  exit 1
fi

# ── Optional: fund deployer via Friendbot if balance is low ──────────────
if [ "${FUND_WITH_FRIENDBOT:-false}" = "true" ]; then
  echo "==> Checking deployer balance..."
  DEPLOYER_PUBLIC=$(stellar keys address "$DEPLOYER_KEY" 2>/dev/null || echo "")
  if [ -n "$DEPLOYER_PUBLIC" ]; then
    BALANCE=$(curl -s "https://horizon-testnet.stellar.org/accounts/$DEPLOYER_PUBLIC" | python3 -c "import sys,json; d=json.load(sys.stdin); print([b['balance'] for b in d.get('balances',[]) if b.get('asset_type')=='native'][0])" 2>/dev/null || echo "0")
    BALANCE_INT=$(echo "$BALANCE" | cut -d. -f1)
    if [ -z "$BALANCE_INT" ] || [ "$BALANCE_INT" -lt 100 ]; then
      echo "==> Balance low ($BALANCE XLM), funding via Friendbot..."
      curl -s "https://friendbot-testnet.stellar.org?addr=$DEPLOYER_PUBLIC" > /dev/null
      echo "==> Funded."
    else
      echo "==> Balance sufficient ($BALANCE XLM), skipping Friendbot."
    fi
  fi
fi

deploy_contract() {
  local name="$1"
  local wasm_path="$2"
  echo "==> Deploying $name..." >&2
  stellar contract deploy \
    --wasm "$wasm_path" \
    $STELLAR_ARGS \
    2>&1 | tail -1
}

echo "=== Deploying contracts (order: invoice -> pool -> credit_score -> share -> governance) ==="

INVOICE_CONTRACT_ID=$(deploy_contract "invoice" "$WASM_DIR/invoice.wasm")
POOL_CONTRACT_ID=$(deploy_contract "pool" "$WASM_DIR/pool.wasm")
CREDIT_SCORE_CONTRACT_ID=$(deploy_contract "credit_score" "$WASM_DIR/credit_score.wasm")
SHARE_CONTRACT_ID=$(deploy_contract "share" "$WASM_DIR/share.wasm")
GOVERNANCE_CONTRACT_ID=$(deploy_contract "governance" "$WASM_DIR/governance.wasm")

echo ""
echo "=== Contract IDs ==="
echo "  Invoice:      $INVOICE_CONTRACT_ID"
echo "  Pool:         $POOL_CONTRACT_ID"
echo "  Credit Score: $CREDIT_SCORE_CONTRACT_ID"
echo "  Share:        $SHARE_CONTRACT_ID"
echo "  Governance:   $GOVERNANCE_CONTRACT_ID"

cat > "$OUTPUT_FILE" <<EOF
INVOICE_CONTRACT_ID=$INVOICE_CONTRACT_ID
POOL_CONTRACT_ID=$POOL_CONTRACT_ID
CREDIT_SCORE_CONTRACT_ID=$CREDIT_SCORE_CONTRACT_ID
SHARE_CONTRACT_ID=$SHARE_CONTRACT_ID
GOVERNANCE_CONTRACT_ID=$GOVERNANCE_CONTRACT_ID
EOF

echo "=== Contract IDs written to $OUTPUT_FILE ==="
