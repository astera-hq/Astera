# Frequently Asked Questions (FAQ)

## General Questions

### What is Astera?

Astera is a Real World Asset (RWA) platform on Stellar that allows SMEs to tokenize unpaid invoices as Soroban-based tokens. Community investors fund a USDC liquidity pool, and smart contracts handle escrow, repayment, and yield distribution.

### How does Astera work?

1. **SMEs** create invoice tokens representing money owed by their customers
2. **Investors** deposit stablecoins into the liquidity pool
3. **Co-funding** occurs when investors commit to fund specific invoices
4. **Funding** happens automatically when the principal target is met
5. **Repayment** includes principal + interest back to the pool
6. **Distribution** sends yield to investors proportional to their commitment

### Is Astera on mainnet?

Currently, Astera runs on Stellar Testnet for development and testing. Mainnet deployment will follow security audits and testing.

---

## For SMEs

### What do I need to get started?

- A Freighter wallet (browser extension)
- Some XLM for transaction fees (testnet XLM is free)
- Invoice details: customer name, amount, due date, description

### How much does it cost?

- **Invoice creation**: Small transaction fee in XLM
- **Interest rate**: 8% APY default (paid when you repay)
- **No platform fees** during testnet phase

### What if my customer pays late?

If you repay late but within 7 days after due date, it's recorded as "Paid Late" with +15 points. After 7 days, it may be marked as Defaulted (-50 points).

### Can I create multiple invoices?

Yes, you can have multiple active invoices. Each contributes to your credit score independently.

### What is the verification hash?

An optional field for linking off-chain documents. You can hash an invoice PDF or other verification documents and include the hash for transparency.

### How do I improve my credit score?

- Pay invoices on time or early
- Create more invoices (milestone bonuses at 5, 10, 20 invoices)
- Build larger total volume
- Pay faster than due date for bonus points

---

## For Investors

### What tokens can I deposit?

Currently supported tokens include USDC and EURC. The admin can add more whitelisted stablecoins over time.

### Is my deposited capital safe?

Deployed capital is committed to invoices and at risk if SMEs default. Available (undeployed) capital can be withdrawn anytime. Diversify across multiple invoices to reduce risk.

### How is yield calculated?

Simple interest formula: `Interest = Principal × (APY/100) × (Days/365)`

Default APY is 8% but may vary.

### When do I receive my returns?

Returns are distributed automatically when the SME repays the invoice. Principal + your share of interest becomes available for withdrawal.

### Can I withdraw before an invoice is repaid?

You can withdraw any **available** balance (undeployed funds). Deployed funds are locked until repayment.

### What happens if an invoice defaults?

- Your deployed capital is at risk
- The SME's credit score decreases significantly
- The default is recorded on-chain
- You may recover partial funds depending on pool mechanisms

### How do I choose which invoices to fund?

Consider:
- SME credit score (higher = safer)
- Invoice amount vs. your available balance
- Due date (shorter = faster return)
- Diversification across multiple SMEs

---

## Technical Questions

### What blockchain does Astera use?

Astera is built on **Stellar** using **Soroban** smart contracts.

### Do I need to know how to code?

No. The frontend provides a user-friendly interface. You only need a wallet and tokens.

### What wallet should I use?

**Freighter** is the recommended wallet. It's a browser extension designed for Stellar.

### Why are transactions failing?

Common reasons:
- Insufficient XLM for fees
- Invoice in wrong state (e.g., already funded)
- Amount exceeds available balance
- Due date in the past

See the [Troubleshooting Guide](./troubleshooting.md) for more.

### How do I get testnet tokens?

**XLM**: Use [Stellar Laboratory](https://laboratory.stellar.org/#account-creator?network=test) or ask in community channels.

**USDC**: Mint from the testnet contract address (ask admins for current address).

---

## Security Questions

### Are the smart contracts audited?

Contracts undergo internal testing and review. Comprehensive third-party audits are planned before mainnet.

### Can the admin steal funds?

The admin can:
- Add/remove accepted tokens
- Initialize invoices for funding
- Set yield rates (capped at 50%)

The admin **cannot**:
- Withdraw user deposits
- Access committed funds
- Modify invoice data

### Is my data private?

All data is on-chain and public (blockchain nature). Invoice details, credit scores, and transactions are visible to all.

---

## Getting Help

### Where can I get support?

1. Check these guides and [Troubleshooting](./troubleshooting.md)
2. Open an issue on GitHub
3. Join community channels (Discord/Telegram if available)

### How do I report a bug?

Open an issue on GitHub with:
- Clear description of the problem
- Steps to reproduce
- Expected vs. actual behavior
- Screenshots if applicable
- Wallet and browser info

### How can I contribute?

See [CONTRIBUTING.md](../CONTRIBUTING.md) for details on:
- Code contributions
- Documentation improvements
- Bug reports
- Feature suggestions

---

**Still have questions?** Check the [Troubleshooting Guide](./troubleshooting.md) or open an issue.
