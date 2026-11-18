#!/usr/bin/env sh
# #1562: Deploy ALL 14 Astera contracts to the local Stellar quickstart network
# and write the resulting contract IDs to /contract-ids/env so other services
# (frontend, oracle, indexer) can consume them.
#
# Usage (inside the contracts container):
#   sh /app/scripts/deploy-local.sh
#
# Environment variables (with defaults):
#   STELLAR_RPC_URL            — Soroban RPC endpoint
#   STELLAR_NETWORK_PASSPHRASE — network passphrase
#   HORIZON_URL                — Horizon endpoint

set -eu

RPC_URL="${STELLAR_RPC_URL:-http://stellar:8000/soroban/rpc}"
NETWORK_PASSPHRASE="${STELLAR_NETWORK_PASSPHRASE:-Standalone Network ; February 2017}"
HORIZON_URL="${HORIZON_URL:-http://stellar:8000}"
OUTPUT_FILE="${CONTRACT_IDS_FILE:-/contract-ids/env}"

echo "==> Building all contracts..."
cargo build --target wasm32-unknown-unknown --release --manifest-path /app/Cargo.toml

WASM_DIR="/app/target/wasm32-unknown-unknown/release"

# ── Generate a local deployer key if one is not already configured ──────────
if ! stellar keys show deployer > /dev/null 2>&1; then
  echo "==> Generating local deployer key..."
  stellar keys generate deployer --network local --fund || true
fi

deploy_contract() {
  local name="$1"
  local wasm="$2"
  echo "==> Deploying $name..."
  stellar contract deploy \
    --wasm "$wasm" \
    --source deployer \
    --rpc-url "$RPC_URL" \
    --network-passphrase "$NETWORK_PASSPHRASE" \
    2>&1 | tail -1
}

# ── Core lending contracts ───────────────────────────────────────────────────
INVOICE_CONTRACT_ID=$(deploy_contract "invoice" "$WASM_DIR/invoice.wasm")
POOL_CONTRACT_ID=$(deploy_contract "pool" "$WASM_DIR/pool.wasm")
CREDIT_SCORE_CONTRACT_ID=$(deploy_contract "credit_score" "$WASM_DIR/credit_score.wasm")
SHARE_CONTRACT_ID=$(deploy_contract "share" "$WASM_DIR/share.wasm")

# ── Market + liquidity contracts ─────────────────────────────────────────────
SECONDARY_MARKET_CONTRACT_ID=$(deploy_contract "secondary_market" "$WASM_DIR/secondary_market.wasm")
TRANCHE_CONTRACT_ID=$(deploy_contract "tranche" "$WASM_DIR/tranche.wasm")
AUCTION_CONTRACT_ID=$(deploy_contract "auction" "$WASM_DIR/auction.wasm")

# ── Governance + access ───────────────────────────────────────────────────────
GOVERNANCE_CONTRACT_ID=$(deploy_contract "governance" "$WASM_DIR/governance.wasm")
ACCESS_CONTROL_CONTRACT_ID=$(deploy_contract "access_control" "$WASM_DIR/access_control.wasm")

# ── Risk + compliance contracts ───────────────────────────────────────────────
COMPLIANCE_CONTRACT_ID=$(deploy_contract "compliance" "$WASM_DIR/compliance.wasm")
INSURANCE_CONTRACT_ID=$(deploy_contract "insurance" "$WASM_DIR/insurance.wasm")
ARBITRATION_CONTRACT_ID=$(deploy_contract "arbitration" "$WASM_DIR/arbitration.wasm")

# ── Oracle + referral contracts ───────────────────────────────────────────────
ORACLE_REGISTRY_CONTRACT_ID=$(deploy_contract "oracle_registry" "$WASM_DIR/oracle_registry.wasm")
REFERRAL_CONTRACT_ID=$(deploy_contract "referral" "$WASM_DIR/referral.wasm")

echo ""
echo "==> Contract IDs:"
echo "    Invoice:          $INVOICE_CONTRACT_ID"
echo "    Pool:             $POOL_CONTRACT_ID"
echo "    Credit Score:     $CREDIT_SCORE_CONTRACT_ID"
echo "    Share:            $SHARE_CONTRACT_ID"
echo "    Secondary Market: $SECONDARY_MARKET_CONTRACT_ID"
echo "    Tranche:          $TRANCHE_CONTRACT_ID"
echo "    Auction:          $AUCTION_CONTRACT_ID"
echo "    Governance:       $GOVERNANCE_CONTRACT_ID"
echo "    Access Control:   $ACCESS_CONTROL_CONTRACT_ID"
echo "    Compliance:       $COMPLIANCE_CONTRACT_ID"
echo "    Insurance:        $INSURANCE_CONTRACT_ID"
echo "    Arbitration:      $ARBITRATION_CONTRACT_ID"
echo "    Oracle Registry:  $ORACLE_REGISTRY_CONTRACT_ID"
echo "    Referral:         $REFERRAL_CONTRACT_ID"

# Write IDs to shared volume so other containers can source them
mkdir -p "$(dirname "$OUTPUT_FILE")"
cat > "$OUTPUT_FILE" <<EOF
INVOICE_CONTRACT_ID=$INVOICE_CONTRACT_ID
POOL_CONTRACT_ID=$POOL_CONTRACT_ID
CREDIT_SCORE_CONTRACT_ID=$CREDIT_SCORE_CONTRACT_ID
SHARE_CONTRACT_ID=$SHARE_CONTRACT_ID
SECONDARY_MARKET_CONTRACT_ID=$SECONDARY_MARKET_CONTRACT_ID
TRANCHE_CONTRACT_ID=$TRANCHE_CONTRACT_ID
AUCTION_CONTRACT_ID=$AUCTION_CONTRACT_ID
GOVERNANCE_CONTRACT_ID=$GOVERNANCE_CONTRACT_ID
ACCESS_CONTROL_CONTRACT_ID=$ACCESS_CONTROL_CONTRACT_ID
COMPLIANCE_CONTRACT_ID=$COMPLIANCE_CONTRACT_ID
INSURANCE_CONTRACT_ID=$INSURANCE_CONTRACT_ID
ARBITRATION_CONTRACT_ID=$ARBITRATION_CONTRACT_ID
ORACLE_REGISTRY_CONTRACT_ID=$ORACLE_REGISTRY_CONTRACT_ID
REFERRAL_CONTRACT_ID=$REFERRAL_CONTRACT_ID
EOF

echo "==> Contract IDs written to $OUTPUT_FILE"
