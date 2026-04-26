# Mainnet Deployment Guide

This guide provides comprehensive instructions for deploying the Astera invoice financing platform to Stellar mainnet for production use. Follow each step carefully and complete all security checks before going live.

---

## ⚠️ Critical Warning

**Mainnet deployment involves real assets and real money.** Unlike testnet:

- All transactions are irreversible
- Contract bugs can result in permanent loss of funds
- Security vulnerabilities can be exploited by malicious actors
- You are responsible for all deployed contracts and their behavior

**DO NOT proceed unless:**

- ✅ All contracts have been thoroughly tested on testnet
- ✅ Security audit has been completed (recommended for production)
- ✅ You have reviewed and understand all contract code
- ✅ You have a incident response plan in place
- ✅ You have adequate insurance or risk mitigation strategies

---

## Prerequisites

### Required Tools

Ensure you have the following installed and up to date:

- **Rust & Cargo** (latest stable version)
- **Stellar CLI** v21.0.0 or later
- **Node.js** 20+ and npm
- **Freighter Wallet** (or other Stellar wallet for mainnet)

### Required Resources

- **XLM for fees**: Minimum 100 XLM recommended for deployment and operations
- **Production domain**: For frontend hosting
- **SSL certificate**: For secure HTTPS connections
- **Monitoring infrastructure**: For contract and system monitoring
- **Backup systems**: For data redundancy and disaster recovery

### Required Knowledge

- Deep understanding of Soroban smart contracts
- Stellar network operations and fee structure
- Security best practices for blockchain applications
- Incident response and crisis management

---

## Pre-Deployment Security Checklist

Complete this checklist before deploying to mainnet:

### Code Security

- [ ] All contracts have comprehensive unit tests with >90% coverage
- [ ] Integration tests cover all critical user flows
- [ ] Fuzz testing completed for all contract entry points
- [ ] No hardcoded addresses or test values in production code
- [ ] All panic conditions have been reviewed and are intentional
- [ ] Integer overflow/underflow protections are in place
- [ ] Access control mechanisms properly restrict admin functions
- [ ] Re-entrancy attacks have been considered and mitigated
- [ ] External contract calls are properly validated
- [ ] Time-dependent logic accounts for block timestamp manipulation

### Contract Architecture

- [ ] Contract upgrade strategy is defined and tested
- [ ] Emergency pause mechanism is implemented (if applicable)
- [ ] Admin key management strategy is documented
- [ ] Multi-signature requirements are configured (recommended)
- [ ] Rate limiting or circuit breakers are in place for high-risk operations
- [ ] Maximum transaction sizes are enforced to prevent DoS
- [ ] Contract storage limits are understood and monitored

### Audit & Review

- [ ] Professional security audit completed (highly recommended)
- [ ] Audit findings have been addressed and verified
- [ ] Code review by multiple experienced Soroban developers
- [ ] Economic model reviewed by financial experts
- [ ] Legal compliance reviewed by counsel (especially for RWA)

### Operational Security

- [ ] Deployer private keys stored in hardware wallet or HSM
- [ ] Admin keys use multi-signature setup (2-of-3 or 3-of-5 recommended)
- [ ] Key backup and recovery procedures documented and tested
- [ ] Team members trained on security protocols
- [ ] Incident response plan documented and rehearsed
- [ ] Communication channels established for security incidents

### Infrastructure Security

- [ ] Frontend hosted on secure infrastructure with DDoS protection
- [ ] SSL/TLS certificates properly configured
- [ ] API endpoints rate-limited and authenticated
- [ ] Database backups automated and tested
- [ ] Monitoring and alerting systems configured
- [ ] Log aggregation and analysis tools in place

---

## Step 1: Prepare Mainnet Keypairs

### Generate Deployer Keypair

**CRITICAL:** Use a hardware wallet (Ledger) or secure key management system for mainnet.

```bash
# Generate deployer keypair
stellar keys generate deployer --network mainnet

# IMMEDIATELY back up the secret key to secure offline storage
# Consider using a hardware wallet instead
```

### Generate Admin Keypair(s)

For production, use a multi-signature setup:

```bash
# Generate multiple admin keys
stellar keys generate admin1 --network mainnet
stellar keys generate admin2 --network mainnet
stellar keys generate admin3 --network mainnet
```

**Security Best Practices:**

- Store each key on separate hardware wallets
- Distribute keys among trusted team members
- Never store keys in plain text or version control
- Use encrypted storage for any digital backups
- Test key recovery procedures before deployment

### Fund Mainnet Accounts

Purchase XLM from a reputable exchange and transfer to your deployer address:

```bash
# Get your deployer address
stellar keys address deployer

# Send at least 100 XLM to this address from your exchange
# Wait for confirmation before proceeding
```

Verify balance:

```bash
stellar keys balance deployer --network mainnet
```

---

## Step 2: Build and Optimize Contracts

### Build for Production

```bash
cd astera
cargo build --target wasm32-unknown-unknown --release
```

### Optimize WASM Files

Optimize contracts to minimize deployment costs and execution fees:

```bash
# Install wasm-opt (part of binaryen)
# macOS
brew install binaryen

# Linux
sudo apt-get install binaryen

# Optimize contracts
wasm-opt -Oz \
  target/wasm32-unknown-unknown/release/invoice.wasm \
  -o target/wasm32-unknown-unknown/release/invoice_optimized.wasm

wasm-opt -Oz \
  target/wasm32-unknown-unknown/release/pool.wasm \
  -o target/wasm32-unknown-unknown/release/pool_optimized.wasm
```

### Verify Optimized Contracts

```bash
# Check file sizes
ls -lh target/wasm32-unknown-unknown/release/*_optimized.wasm

# Verify contracts still function correctly
# Run full test suite against optimized builds
cargo test --release
```

---

## Step 3: Deploy Contracts to Mainnet

### Deploy Invoice Contract

```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/invoice_optimized.wasm \
  --source deployer \
  --network mainnet
```

**Save the contract ID immediately:**

```bash
export INVOICE_CONTRACT_ID=<OUTPUT_FROM_ABOVE>
echo "Invoice Contract: $INVOICE_CONTRACT_ID" >> mainnet_deployment.txt
```

### Deploy Pool Contract

```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/pool_optimized.wasm \
  --source deployer \
  --network mainnet
```

**Save the contract ID:**

```bash
export POOL_CONTRACT_ID=<OUTPUT_FROM_ABOVE>
echo "Pool Contract: $POOL_CONTRACT_ID" >> mainnet_deployment.txt
```

### Record Deployment Details

Create a deployment record:

```bash
cat > mainnet_deployment.txt << EOF
Deployment Date: $(date -u +"%Y-%m-%d %H:%M:%S UTC")
Network: Mainnet
Deployer: $(stellar keys address deployer)
Invoice Contract: $INVOICE_CONTRACT_ID
Pool Contract: $POOL_CONTRACT_ID
Git Commit: $(git rev-parse HEAD)
EOF
```

---

## Step 4: Contract Verification

Contract verification ensures transparency and builds trust with users.

### Verify on Stellar Expert

1. Navigate to [Stellar Expert Mainnet](https://stellar.expert/explorer/public)
2. Search for your contract ID
3. Click "Verify Contract" button
4. Upload source code and build instructions

### Verify Contract Hash

Generate and publish contract hash for independent verification:

```bash
# Generate SHA-256 hash of optimized WASM
sha256sum target/wasm32-unknown-unknown/release/invoice_optimized.wasm
sha256sum target/wasm32-unknown-unknown/release/pool_optimized.wasm

# Add to deployment record
echo "Invoice WASM SHA-256: $(sha256sum target/wasm32-unknown-unknown/release/invoice_optimized.wasm | cut -d' ' -f1)" >> mainnet_deployment.txt
echo "Pool WASM SHA-256: $(sha256sum target/wasm32-unknown-unknown/release/pool_optimized.wasm | cut -d' ' -f1)" >> mainnet_deployment.txt
```

### Publish Verification Information

Create a public verification document:

```bash
cat > VERIFICATION.md << EOF
# Astera Mainnet Contract Verification

## Deployment Information
- **Network:** Stellar Mainnet
- **Deployment Date:** $(date -u +"%Y-%m-%d")
- **Git Commit:** $(git rev-parse HEAD)
- **Git Tag:** v1.0.0

## Contract Addresses
- **Invoice Contract:** $INVOICE_CONTRACT_ID
- **Pool Contract:** $POOL_CONTRACT_ID

## WASM Hashes
- **Invoice Contract SHA-256:** $(sha256sum target/wasm32-unknown-unknown/release/invoice_optimized.wasm | cut -d' ' -f1)
- **Pool Contract SHA-256:** $(sha256sum target/wasm32-unknown-unknown/release/pool_optimized.wasm | cut -d' ' -f1)

## Build Instructions
\`\`\`bash
git clone https://github.com/astera-hq/Astera.git
cd Astera
git checkout v1.0.0
cargo build --target wasm32-unknown-unknown --release
wasm-opt -Oz target/wasm32-unknown-unknown/release/invoice.wasm -o invoice_optimized.wasm
wasm-opt -Oz target/wasm32-unknown-unknown/release/pool.wasm -o pool_optimized.wasm
\`\`\`

## Verification
Anyone can verify these contracts by:
1. Following the build instructions above
2. Computing SHA-256 hash of the resulting WASM files
3. Comparing with the published hashes
4. Querying the contract on-chain to verify the deployed bytecode

## Audit Reports
- [Link to security audit report]
- [Link to economic model review]

## Contact
- Security issues: security@astera.example
- General inquiries: contact@astera.example
EOF
```

Commit and publish this verification document to your repository.

---

## Step 5: Initialize Contracts

### Get Mainnet USDC Address

```bash
# Mainnet USDC issued by Circle
export USDC_TOKEN_ID=$(stellar contract id asset \
  --asset USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN \
  --network mainnet)

echo "USDC Token: $USDC_TOKEN_ID" >> mainnet_deployment.txt
```

### Initialize Invoice Contract

```bash
export ADMIN_ADDRESS=$(stellar keys address admin1)

stellar contract invoke \
  --id $INVOICE_CONTRACT_ID \
  --source deployer \
  --network mainnet \
  -- initialize \
  --admin $ADMIN_ADDRESS \
  --pool $POOL_CONTRACT_ID
```

### Initialize Pool Contract

```bash
stellar contract invoke \
  --id $POOL_CONTRACT_ID \
  --source deployer \
  --network mainnet \
  -- initialize \
  --admin $ADMIN_ADDRESS \
  --usdc_token $USDC_TOKEN_ID \
  --invoice_contract $INVOICE_CONTRACT_ID
```

### Verify Initialization

```bash
# Verify invoice contract
stellar contract invoke \
  --id $INVOICE_CONTRACT_ID \
  --network mainnet \
  -- get_invoice_count

# Verify pool contract
stellar contract invoke \
  --id $POOL_CONTRACT_ID \
  --network mainnet \
  -- get_config
```

---

## Step 6: Configure Multi-Signature Admin (Recommended)

For production security, configure multi-signature requirements for admin operations.

### Set Up Multi-Sig Account

```bash
# This requires custom implementation in your contracts
# Example: Require 2 of 3 signatures for admin operations

stellar contract invoke \
  --id $POOL_CONTRACT_ID \
  --source admin1 \
  --network mainnet \
  -- set_admin_threshold \
  --threshold 2 \
  --signers '["'$ADMIN1_ADDRESS'", "'$ADMIN2_ADDRESS'", "'$ADMIN3_ADDRESS'"]'
```

**Note:** Multi-signature support must be implemented in your contract code. If not present, consider upgrading contracts before mainnet deployment.

---

## Step 7: Production Frontend Deployment

### Configure Environment Variables

```bash
cd frontend

# Create production environment file
cat > .env.production << EOF
NEXT_PUBLIC_NETWORK=mainnet
NEXT_PUBLIC_INVOICE_CONTRACT_ID=$INVOICE_CONTRACT_ID
NEXT_PUBLIC_POOL_CONTRACT_ID=$POOL_CONTRACT_ID
NEXT_PUBLIC_USDC_TOKEN_ID=$USDC_TOKEN_ID
NEXT_PUBLIC_RPC_URL=https://soroban-mainnet.stellar.org
NEXT_PUBLIC_HORIZON_URL=https://horizon.stellar.org
EOF
```

### Build Production Frontend

```bash
npm run build
```

### Deploy to Production Hosting

Choose a reliable hosting provider:

**Option 1: Vercel**

```bash
npm install -g vercel
vercel --prod
```

**Option 2: AWS Amplify**

```bash
# Follow AWS Amplify deployment guide
```

**Option 3: Self-hosted**

```bash
# Use PM2 or similar process manager
npm install -g pm2
pm2 start npm --name "astera-frontend" -- start
pm2 save
pm2 startup
```

### Configure CDN and DDoS Protection

- Set up Cloudflare or similar CDN
- Enable DDoS protection
- Configure rate limiting
- Set up Web Application Firewall (WAF)

---

## Step 8: Monitoring Setup

### Contract Monitoring

Set up monitoring for contract events and state:

```bash
# Create monitoring script
cat > scripts/monitor_contracts.sh << 'EOF'
#!/bin/bash

INVOICE_CONTRACT=$1
POOL_CONTRACT=$2

while true; do
  echo "=== $(date) ==="

  # Monitor pool stats
  stellar contract invoke \
    --id $POOL_CONTRACT \
    --network mainnet \
    -- get_config

  # Monitor invoice count
  stellar contract invoke \
    --id $INVOICE_CONTRACT \
    --network mainnet \
    -- get_invoice_count

  sleep 300  # Check every 5 minutes
done
EOF

chmod +x scripts/monitor_contracts.sh
```

### Set Up Alerting

Configure alerts for critical events:

**Prometheus + Grafana Setup:**

```yaml
# prometheus.yml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: "stellar_contracts"
    static_configs:
      - targets: ["localhost:9090"]

alerting:
  alertmanagers:
    - static_configs:
        - targets: ["localhost:9093"]

rule_files:
  - "alerts.yml"
```

**Alert Rules:**

```yaml
# alerts.yml
groups:
  - name: contract_alerts
    rules:
      - alert: HighFailureRate
        expr: contract_failures > 10
        for: 5m
        annotations:
          summary: "High contract failure rate detected"

      - alert: LowPoolLiquidity
        expr: pool_liquidity < 1000000
        for: 10m
        annotations:
          summary: "Pool liquidity below threshold"

      - alert: UnusualTransactionVolume
        expr: rate(transactions[5m]) > 100
        for: 5m
        annotations:
          summary: "Unusual transaction volume detected"
```

### Log Aggregation

Set up centralized logging:

```bash
# Example: Using ELK Stack (Elasticsearch, Logstash, Kibana)
# Or use cloud services like Datadog, New Relic, or Splunk
```

### Health Check Endpoints

Create health check endpoints for your frontend:

```typescript
// pages/api/health.ts
export default async function handler(req, res) {
  try {
    // Check contract connectivity
    const invoiceHealth = await checkContractHealth(INVOICE_CONTRACT_ID);
    const poolHealth = await checkContractHealth(POOL_CONTRACT_ID);

    if (invoiceHealth && poolHealth) {
      res.status(200).json({ status: "healthy" });
    } else {
      res.status(503).json({ status: "degraded" });
    }
  } catch (error) {
    res.status(500).json({ status: "unhealthy", error: error.message });
  }
}
```

---

## Step 9: Security Monitoring

### Transaction Monitoring

Monitor all contract transactions for suspicious activity:

```bash
# Create transaction monitoring script
cat > scripts/monitor_transactions.sh << 'EOF'
#!/bin/bash

CONTRACT_ID=$1
WEBHOOK_URL=$2

stellar contract events \
  --id $CONTRACT_ID \
  --network mainnet \
  --start-ledger latest \
  --follow | while read event; do

  # Parse event and check for anomalies
  # Send alerts to webhook if suspicious activity detected

  echo "$event" | jq '.'
done
EOF

chmod +x scripts/monitor_transactions.sh
```

### Anomaly Detection

Implement anomaly detection for:

- Unusual transaction patterns
- Large withdrawals or deposits
- Rapid succession of admin operations
- Failed transaction spikes
- Gas price anomalies

### Security Incident Response

Create an incident response runbook:

```markdown
# Security Incident Response Runbook

## Severity Levels

- **P0 (Critical):** Active exploit, funds at risk
- **P1 (High):** Potential vulnerability discovered
- **P2 (Medium):** Suspicious activity detected
- **P3 (Low):** Minor security concern

## P0 Response Procedure

1. Immediately notify all team members
2. Assess the situation and confirm the threat
3. Execute emergency pause if implemented
4. Engage security audit firm
5. Prepare public communication
6. Document all actions taken

## Contact List

- Security Lead: [phone/email]
- CTO: [phone/email]
- Legal Counsel: [phone/email]
- Audit Firm: [phone/email]
- PR Contact: [phone/email]

## Emergency Pause Procedure

[Document steps to pause contracts if implemented]

## Communication Templates

[Pre-approved templates for security disclosures]
```

---

## Step 10: Rollback Procedures

For the contract timelock upgrade workflow and migration planning, see the
[Contract Upgrade Guide](./contract-upgrade-guide.md).

### Contract Upgrade Strategy

If you need to fix bugs or upgrade contracts:

**Option 1: Deploy New Contract**

```bash
# Deploy new version
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/pool_v2_optimized.wasm \
  --source deployer \
  --network mainnet

# Migrate data from old contract to new contract
# Update frontend to point to new contract
```

**Option 2: Upgradeable Contracts (if implemented)**

```bash
# Upload new WASM
stellar contract install \
  --wasm target/wasm32-unknown-unknown/release/pool_v2_optimized.wasm \
  --source deployer \
  --network mainnet

# Update contract to use new WASM
stellar contract invoke \
  --id $POOL_CONTRACT_ID \
  --source admin1 \
  --network mainnet \
  -- upgrade \
  --new_wasm_hash <NEW_WASM_HASH>
```

### Data Migration Plan

Document data migration procedures:

```bash
# Export current state
stellar contract invoke \
  --id $POOL_CONTRACT_ID \
  --network mainnet \
  -- export_state > pool_state_backup.json

# Import to new contract
stellar contract invoke \
  --id $NEW_POOL_CONTRACT_ID \
  --source admin1 \
  --network mainnet \
  -- import_state \
  --state "$(cat pool_state_backup.json)"
```

### Emergency Shutdown Procedure

If critical vulnerability is discovered:

```bash
# 1. Pause all contract operations (if pause function exists)
stellar contract invoke \
  --id $POOL_CONTRACT_ID \
  --source admin1 \
  --network mainnet \
  -- pause

# 2. Notify all users immediately
# 3. Assess the vulnerability
# 4. Develop and test fix
# 5. Deploy fix or new contract
# 6. Migrate user funds safely
# 7. Resume operations
```

### Rollback Checklist

- [ ] Backup current contract state
- [ ] Test rollback procedure on testnet
- [ ] Notify users of planned maintenance
- [ ] Execute rollback during low-traffic period
- [ ] Verify new contract functionality
- [ ] Monitor for issues post-rollback
- [ ] Update documentation and verification info

---

## Step 11: Post-Deployment Verification

### Functional Testing

Test all critical functions on mainnet with small amounts:

```bash
# Test deposit (use small amount first!)
stellar contract invoke \
  --id $POOL_CONTRACT_ID \
  --source test_user \
  --network mainnet \
  -- deposit \
  --amount 1000000  # 0.1 USDC

# Test invoice creation
stellar contract invoke \
  --id $INVOICE_CONTRACT_ID \
  --source test_sme \
  --network mainnet \
  -- create_invoice \
  --amount 10000000 \
  --debtor "Test Debtor" \
  --due_date 1735689600

# Test withdrawal
stellar contract invoke \
  --id $POOL_CONTRACT_ID \
  --source test_user \
  --network mainnet \
  -- withdraw \
  --amount 1000000
```

### Performance Testing

Monitor contract performance:

- Transaction confirmation times
- Gas costs for each operation
- Contract response times
- Frontend load times

### User Acceptance Testing

- Have beta users test the platform
- Collect feedback on UX and functionality
- Monitor for any unexpected behavior
- Verify all features work as expected

---

## Step 12: Go-Live Checklist

Complete this final checklist before announcing mainnet launch:

### Technical Readiness

- [ ] All contracts deployed and verified
- [ ] All contracts initialized correctly
- [ ] Multi-signature admin configured
- [ ] Frontend deployed to production
- [ ] SSL certificates configured
- [ ] CDN and DDoS protection active
- [ ] Monitoring and alerting operational
- [ ] Backup systems tested
- [ ] Health checks passing

### Security Readiness

- [ ] Security audit completed and issues resolved
- [ ] Penetration testing completed
- [ ] Bug bounty program launched
- [ ] Incident response plan documented
- [ ] Emergency contacts established
- [ ] Insurance coverage in place (if applicable)

### Operational Readiness

- [ ] Team trained on operations
- [ ] Support channels established
- [ ] Documentation complete and published
- [ ] Terms of service and privacy policy published
- [ ] Legal compliance verified
- [ ] KYC/AML procedures implemented (if required)

### Communication Readiness

- [ ] Launch announcement prepared
- [ ] Social media accounts ready
- [ ] Community channels established (Discord, Telegram, etc.)
- [ ] Press kit prepared
- [ ] FAQ document published
- [ ] User guides and tutorials ready

---

## Ongoing Maintenance

### Regular Security Reviews

- **Weekly:** Review transaction logs for anomalies
- **Monthly:** Security posture assessment
- **Quarterly:** Code audit and dependency updates
- **Annually:** Comprehensive security audit

### Contract Monitoring

- Monitor contract state daily
- Review gas costs and optimize if needed
- Track user growth and usage patterns
- Monitor for any unusual activity

### Updates and Upgrades

- Keep dependencies up to date
- Monitor Stellar network upgrades
- Plan contract upgrades carefully
- Test all changes thoroughly on testnet first

### Community Engagement

- Respond to user issues promptly
- Maintain active communication channels
- Publish regular updates and reports
- Build trust through transparency

---

## Emergency Contacts

Maintain an up-to-date list of emergency contacts:

```markdown
## Emergency Contact List

### Internal Team

- Security Lead: [name] - [phone] - [email]
- CTO: [name] - [phone] - [email]
- CEO: [name] - [phone] - [email]

### External Partners

- Security Audit Firm: [company] - [contact] - [phone]
- Legal Counsel: [firm] - [contact] - [phone]
- Insurance Provider: [company] - [contact] - [phone]

### Service Providers

- Hosting Provider: [company] - [support contact]
- CDN Provider: [company] - [support contact]
- Monitoring Service: [company] - [support contact]

### Stellar Network

- Stellar Development Foundation: security@stellar.org
```

---

## Useful Resources

### Stellar Mainnet

- **RPC Endpoint:** https://soroban-mainnet.stellar.org
- **Horizon API:** https://horizon.stellar.org
- **Explorer:** https://stellar.expert/explorer/public
- **Status Page:** https://status.stellar.org

### Documentation

- [Stellar Documentation](https://developers.stellar.org/)
- [Soroban Documentation](https://soroban.stellar.org/)
- [Stellar CLI Reference](https://developers.stellar.org/docs/tools/developer-tools/stellar-cli)

### Security

- [Soroban Security Best Practices](https://soroban.stellar.org/docs/learn/security)
- [Smart Contract Security Verification Standard](https://github.com/securing/SCSVS)

### Community

- [Stellar Discord](https://discord.gg/stellar)
- [Stellar Stack Exchange](https://stellar.stackexchange.com/)

---

## Conclusion

Mainnet deployment is a significant milestone that requires careful planning, thorough testing, and ongoing vigilance. This guide provides a comprehensive framework, but every deployment is unique. Always prioritize security, maintain transparency with your users, and be prepared to respond quickly to any issues.

**Remember:** With great power comes great responsibility. You are now managing real user funds and must treat this responsibility with the utmost seriousness.

Good luck with your mainnet launch! 🚀

---

## Appendix: Cost Estimates

### Deployment Costs (Approximate)

- Contract deployment: ~10-20 XLM per contract
- Contract initialization: ~5-10 XLM per contract
- Testing transactions: ~10-20 XLM
- **Total estimated:** 50-100 XLM (~$5-10 USD at current prices)

### Ongoing Costs

- Transaction fees: ~0.00001 XLM per operation
- Contract storage: Minimal (included in deployment)
- Hosting: $50-500/month depending on scale
- Monitoring: $50-200/month
- Security audit: $10,000-50,000 (one-time)

### Budget Planning

Plan for:

- Initial deployment: $500-1,000
- First year operations: $5,000-20,000
- Security and audits: $10,000-50,000
- **Total first year:** $15,000-70,000

Costs vary significantly based on transaction volume, security requirements, and infrastructure choices.
