# Mainnet Deployment Checklist

Quick reference checklist for mainnet deployment. See [Mainnet Deployment Guide](mainnet-deployment.md) for detailed instructions.

---

## Pre-Deployment (Complete ALL items)

- [ ] **Multi-sig required before deployment**

### Security Audit

- [ ] Professional security audit completed
- [ ] All critical and high-severity findings resolved
- [ ] Audit report reviewed and published
- [ ] Economic model validated by financial experts
- [ ] Legal compliance review completed
- [ ] Audit findings recorded in [security audit tracking](security-audit/audit-tracking.md)
- [ ] No unresolved critical/high findings remain in the audit tracker

### Code Quality

- [ ] Unit test coverage >90%
- [ ] Integration tests pass for all user flows
- [ ] Fuzz testing completed
- [ ] No hardcoded test values or addresses
- [ ] All panic conditions reviewed and justified
- [ ] Access control properly implemented
- [ ] Re-entrancy protections in place

### Infrastructure

- [ ] Hardware wallet or HSM for key management
- [ ] Multi-signature admin setup (2-of-3 or 3-of-5)
- [ ] Key backup and recovery procedures tested
- [ ] Production domain and SSL certificates ready
- [ ] CDN and DDoS protection configured
- [ ] Monitoring and alerting systems operational
- [ ] Log aggregation configured

### Documentation

- [ ] Mainnet deployment guide reviewed
- [ ] Incident response plan documented ([disaster recovery runbook](disaster-recovery.md))
- [ ] Emergency contact list updated
- [ ] User documentation complete
- [ ] Terms of service and privacy policy published
- [ ] Contract verification information prepared

---

## Deployment Steps

### 1. Prepare Accounts

- [ ] Generate mainnet keypairs securely
- [ ] Fund deployer account with 100+ XLM
- [ ] Verify account balances
- [ ] Back up all keys to secure offline storage

### 2. Build & Optimize

- [ ] Build contracts in release mode
- [ ] Optimize WASM files with wasm-opt
- [ ] Verify optimized contracts with tests
- [ ] Record git commit hash

### 3. Deploy Contracts

- [ ] Deploy invoice contract to mainnet
- [ ] Deploy pool contract to mainnet
- [ ] Save all contract IDs
- [ ] Record deployment details (date, deployer, commit)

### 4. Verify Contracts

- [ ] Verify contracts on Stellar Expert
- [ ] Generate and publish WASM hashes
- [ ] Create VERIFICATION.md document
- [ ] Commit verification info to repository

### 5. Initialize Contracts

- [ ] Get mainnet USDC token address
- [ ] Initialize invoice contract
- [ ] Initialize pool contract
- [ ] Verify initialization successful

### 6. Configure Security

- [ ] Set up multi-signature admin (if implemented)
- [ ] Test admin operations with multi-sig
- [ ] Verify access controls working correctly
- [ ] Document admin procedures

### 7. Deploy Frontend

- [ ] Configure production environment variables
- [ ] Build production frontend
- [ ] Deploy to hosting provider
- [ ] Configure CDN and SSL
- [ ] Set up WAF and rate limiting

### 8. Set Up Monitoring

- [ ] Deploy contract monitoring scripts
- [ ] Configure alerting rules
- [ ] Set up log aggregation
- [ ] Create health check endpoints
- [ ] Test alert notifications

### 9. Security Monitoring

- [ ] Deploy transaction monitoring
- [ ] Configure anomaly detection
- [ ] Test incident response procedures against [disaster-recovery.md](disaster-recovery.md)
- [ ] Verify emergency contacts

---

## Post-Deployment

### Verification

- [ ] Test deposit with small amount
- [ ] Test invoice creation
- [ ] Test withdrawal
- [ ] Verify all contract functions work
- [ ] Check frontend connectivity
- [ ] Monitor performance metrics

### Testing

- [ ] Conduct user acceptance testing
- [ ] Beta test with limited users
- [ ] Monitor for unexpected behavior
- [ ] Collect and address feedback

### Go-Live Checklist

- [ ] All technical systems operational
- [ ] Security monitoring active
- [ ] Support channels established
- [ ] Launch announcement prepared
- [ ] Bug bounty program launched
- [ ] Team trained on operations

---

## Ongoing Maintenance

### Daily

- [ ] Review transaction logs
- [ ] Check monitoring dashboards
- [ ] Respond to user issues

### Weekly

- [ ] Review security alerts
- [ ] Analyze usage patterns
- [ ] Check system performance

### Monthly

- [ ] Security posture assessment
- [ ] Review and update documentation
- [ ] Analyze metrics and KPIs

### Quarterly

- [ ] Code audit and dependency updates
- [ ] Review the [disaster recovery runbook](disaster-recovery.md)
- [ ] Conduct security drills

### Annually

- [ ] Comprehensive security audit
- [ ] Review and update insurance
- [ ] Strategic planning and roadmap

---

## Emergency Procedures

### If Critical Vulnerability Discovered

Follow [docs/disaster-recovery.md](disaster-recovery.md), Scenario 1, and then
use this checklist:

1. [ ] Notify all team members immediately
2. [ ] Assess threat and confirm severity
3. [ ] Execute emergency pause (if available)
4. [ ] Engage security audit firm
5. [ ] Prepare public communication
6. [ ] Document all actions

### If Unusual Activity Detected

Follow [docs/disaster-recovery.md](disaster-recovery.md), Scenario 2 or
Scenario 4 depending on whether the issue is admin access or bad state.

1. [ ] Investigate and confirm anomaly
2. [ ] Determine if malicious or benign
3. [ ] Increase monitoring if needed
4. [ ] Alert team if threat confirmed
5. [ ] Take appropriate action

### If System Outage

Follow [docs/disaster-recovery.md](disaster-recovery.md), Scenario 3, before
marking the service restored.

1. [ ] Identify root cause
2. [ ] Notify users of outage
3. [ ] Implement fix or workaround
4. [ ] Verify system restored
5. [ ] Post-mortem analysis

---

## Key Contacts

### Internal

- Security Lead: ******\_\_\_\_******
- CTO: ******\_\_\_\_******
- CEO: ******\_\_\_\_******

### External

- Security Auditor: ******\_\_\_\_******
- Legal Counsel: ******\_\_\_\_******
- Insurance Provider: ******\_\_\_\_******

### Emergency

- Stellar Foundation: security@stellar.org
- Hosting Support: ******\_\_\_\_******
- CDN Support: ******\_\_\_\_******

---

## Important Links

- [Full Mainnet Deployment Guide](mainnet-deployment.md)
- [Disaster Recovery Runbook](disaster-recovery.md)
- [Testnet Deployment Guide](deployment.md)
- [Stellar Expert Mainnet](https://stellar.expert/explorer/public)
- [Stellar Status Page](https://status.stellar.org)
- [Soroban Documentation](https://soroban.stellar.org/)

---

## Notes

Use this space to record deployment-specific information:

**Deployment Date:** ******\_\_\_\_******

**Contract IDs:**

- Invoice: ******\_\_\_\_******
- Pool: ******\_\_\_\_******

**Git Commit:** ******\_\_\_\_******

**Team Members Present:** ******\_\_\_\_******

**Issues Encountered:** ******\_\_\_\_******

**Lessons Learned:** ******\_\_\_\_******

---

**Remember:** Mainnet deployment is irreversible. Double-check everything before proceeding.
