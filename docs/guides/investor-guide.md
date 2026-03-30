# Investor User Guide

A complete guide for investors providing liquidity on Astera and earning yield.

## Table of Contents

1. [Overview](#overview)
2. [Prerequisites](#prerequisites)
3. [Step-by-Step: Making Your First Deposit](#step-by-step-making-your-first-deposit)
4. [Committing to Invoices](#committing-to-invoices)
5. [Understanding Yield](#understanding-yield)
6. [Withdrawing Funds](#withdrawing-funds)
7. [Managing Risk](#managing-risk)
8. [Best Practices](#best-practices)

---

## Overview

As an investor on Astera, you can:

- **Deposit stablecoins** - USDC, EURC, and other whitelisted tokens
- **Earn yield** - Default 8% APY on funded invoices
- **Diversify risk** - Spread investments across multiple invoices
- **Withdraw anytime** - Access undeployed capital when needed
- **Support real businesses** - Help SMEs with working capital

## Prerequisites

Before you begin:

1. **[Freighter Wallet](https://www.freighter.app/)** installed
2. **Stablecoins** (USDC on Stellar testnet) - Get from testnet faucet or mint
3. **XLM** for transaction fees

## Step-by-Step: Making Your First Deposit

### Step 1: Connect Your Wallet

1. Open the Astera app
2. Click "Connect Wallet" 
3. Select "Freighter"
4. Approve the connection

### Step 2: Navigate to Invest

1. Click **"Invest"** in the navigation
2. You'll see the investment interface

### Step 3: Choose Token and Amount

| Field | Description |
|-------|-------------|
| **Token** | Select USDC or other accepted stablecoin |
| **Amount** | How much you want to deposit |

Example: Deposit 5000 USDC

### Step 4: Approve Token Transfer

1. Click **"Deposit"**
2. Freighter will ask you to approve the token transfer
3. Review the amount
4. Sign the transaction

### Step 5: Confirm Deposit

After confirmation:
- Your position is created
- Available balance shows your deposit
- Funds are ready to commit to invoices

![Deposit Confirmation](assets/deposit-confirm.png)

## Committing to Invoices

### Viewing Available Invoices

The Invest page shows:
- **Invoice ID** - Unique identifier
- **SME** - Business requesting funding
- **Principal** - Total funding needed
- **Committed** - Amount already pledged
- **Remaining** - Funding gap
- **Due Date** - Expected repayment
- **Yield** - Expected APY

### Making a Commitment

1. Find an invoice with remaining funding needed
2. Click **"Commit"**
3. Enter commitment amount (up to your available balance)
4. Click **"Confirm"**
5. Sign the transaction

### Full Funding Process

When an invoice reaches full commitment:
1. All committed funds are transferred to the SME
2. Invoice status changes to "Funded"
3. Your position shows "deployed" amount
4. Yield starts accruing

## Understanding Yield

### How Yield Works

- **Default Rate**: 8% APY
- **Calculation**: Simple interest based on funding duration
- **Payment**: Principal + interest returned on repayment

### Yield Formula

```
Interest = Principal × (APY / 100) × (Days / 365)
```

### Example

- Commit: 1000 USDC to 30-day invoice
- APY: 8%
- Return: 1000 + (1000 × 0.08 × 30/365) = 1006.58 USDC
- Your yield: 6.58 USDC

### Proportional Distribution

If multiple investors fund one invoice:
- Each gets yield proportional to their commitment
- If you committed 40% of principal, you get 40% of total interest

## Withdrawing Funds

### Withdraw Available Balance

1. Go to **Invest** page
2. View your position for a token
3. Click **"Withdraw"**
4. Enter amount (up to available balance)
5. Confirm and sign

### Important Notes

| Balance Type | Can Withdraw? |
|--------------|---------------|
| **Available** | Yes, anytime |
| **Deployed** | No, locked until repayment |
| **Earned** | Yes, after invoice repaid |

### Withdrawal Process

1. Request withdrawal
2. Smart contract transfers tokens
3. Position updated
4. Funds appear in wallet

## Managing Risk

### Risk Factors to Consider

1. **SME Credit Score**
   - Check the credit score before committing
   - Higher scores = lower default risk
   - New SMEs have higher risk

2. **Invoice Due Date**
   - Shorter terms = faster returns
   - Longer terms = higher risk of issues

3. **Diversification**
   - Spread commitments across multiple invoices
   - Don't put all funds in one SME
   - Mix of credit scores

4. **Platform Risk**
   - Smart contract risk (audited but not zero)
   - Stellar network availability
   - Oracle reliability (if verification enabled)

### Risk Mitigation Strategies

1. **Start small** - Test with small amounts
2. **Diversify** - Multiple invoices, multiple SMEs
3. **Monitor** - Check dashboard regularly
4. **Research** - Look at payment history

### What Happens in Default?

If an invoice defaults:
- SME credit score decreases significantly (-50 points)
- Your deployed capital may be at risk
- Pool may have mechanisms to recover
- Documented on-chain for future reference

## Best Practices

1. **Keep available buffer** - Don't commit 100% of deposits
2. **Monitor funding status** - Act quickly on good opportunities
3. **Review credit scores** - Higher scores are safer
4. **Track your returns** - Monitor yield over time
5. **Reinvest returns** - Compound your earnings
6. **Stay informed** - Follow platform updates

### Portfolio Management Tips

| Strategy | Approach |
|----------|----------|
| Conservative | Only high credit scores (700+) |
| Balanced | Mix of 600-800 scores |
| Aggressive | Include newer SMEs for higher yield |

---

**Next Steps:**
- Read the [SME Guide](./sme-guide.md) to understand borrowers
- Check [FAQ](./faq.md) for common questions
- Visit [Troubleshooting](./troubleshooting.md) if you encounter issues
