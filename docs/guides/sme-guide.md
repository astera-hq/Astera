# SME User Guide

A complete guide for businesses using Astera to tokenize invoices and access liquidity.

## Table of Contents

1. [Overview](#overview)
2. [Prerequisites](#prerequisites)
3. [Step-by-Step: Creating Your First Invoice](#step-by-step-creating-your-first-invoice)
4. [Managing Your Invoices](#managing-your-invoices)
5. [Repaying an Invoice](#repaying-an-invoice)
6. [Understanding Your Credit Score](#understanding-your-credit-score)
7. [Best Practices](#best-practices)

---

## Overview

As an SME (Small and Medium Enterprise), Astera allows you to:

- **Tokenize unpaid invoices** - Convert your accounts receivable into tradable tokens
- **Access instant liquidity** - Get funded by community investors instead of waiting for customer payment
- **Build on-chain credit history** - Every repaid invoice improves your credit score
- **Pay lower rates** - Better credit scores unlock better terms

## Prerequisites

Before you begin:

1. **[Freighter Wallet](https://www.freighter.app/)** installed and configured
2. **Testnet XLM** for transaction fees (on testnet, get from [Stellar Laboratory](https://laboratory.stellar.org/#account-creator?network=test))
3. **Invoice details** ready: customer name, amount, due date, description

## Step-by-Step: Creating Your First Invoice

### Step 1: Connect Your Wallet

1. Open the Astera app
2. Click "Connect Wallet" in the top right
3. Select "Freighter" from the options
4. Approve the connection in your Freighter wallet

![Wallet Connection](assets/wallet-connect.png)

### Step 2: Navigate to New Invoice

1. From the dashboard, click **"New Invoice"**
2. You'll see the invoice creation form

### Step 3: Fill Invoice Details

| Field | Description | Example |
|-------|-------------|---------|
| **Debtor** | Name of the customer who owes payment | "ABC Manufacturing Ltd" |
| **Amount** | Invoice amount in USDC (7 decimals) | 10000 = 1000.00 USDC |
| **Due Date** | When payment is expected | Select future date |
| **Description** | Details about the invoice | "Q4 Services - Consulting" |
| **Verification Hash** | Optional document hash for verification | "a1b2c3d4..." |

![Invoice Form](assets/invoice-form.png)

### Step 4: Submit and Sign

1. Review all details carefully
2. Click **"Create Invoice"**
3. Freighter will prompt you to sign the transaction
4. Approve the transaction
5. Wait for confirmation (usually 5-10 seconds)

### Step 5: Track Your Invoice

After creation, your invoice appears on the dashboard with status:

- **Pending** - Awaiting investor funding
- **AwaitingVerification** - Pending oracle verification (if enabled)
- **Verified** - Approved and ready for funding
- **Funded** - Investors have committed funds
- **Paid** - You have repaid (principal + interest)
- **Defaulted** - Payment missed

## Managing Your Invoices

### Dashboard Overview

The dashboard shows:
- **Active Invoices** - Currently funded invoices
- **Pending Invoices** - Awaiting funding
- **Credit Score** - Your on-chain credit rating
- **Total Volume** - Historical invoice volume

### Invoice Status Flow

```
Created → Pending → [Verified] → Funded → Repaid
                              ↓
                           Defaulted (if not repaid)
```

## Repaying an Invoice

When your customer pays you (or when you have funds available):

1. Go to **Dashboard**
2. Find the funded invoice
3. Click **"Repay Invoice"**
4. Review the total due (principal + interest)
5. Sign the transaction
6. Funds are distributed to investors automatically

### Repayment Calculation

```
Total Due = Principal + (Principal × Interest Rate × Time)
```

Default rate: 8% APY

Example: 1000 USDC for 30 days = 1000 + (1000 × 0.08 × 30/365) ≈ 1006.58 USDC

## Understanding Your Credit Score

Your credit score (200-850) affects:
- **Funding priority** - Higher scores get funded faster
- **Interest rates** - Better scores may get better rates
- **Trust level** - Visible to investors

### Score Factors

| Factor | Impact |
|--------|--------|
| On-time payments | +30 points each |
| Late payments | +15 points each |
| Defaults | -50 points each |
| Invoice volume | Bonus at 1B+, 10B+, 100B+ |
| Payment speed | Early payment bonuses |

### Score Bands

| Score | Rating | Meaning |
|-------|--------|---------|
| 800+ | Excellent | Premium borrower |
| 740-799 | Very Good | Trusted borrower |
| 670-739 | Good | Reliable borrower |
| 580-669 | Fair | Acceptable risk |
| 500-579 | Poor | Higher risk |
| <500 | Very Poor | Limited access |

## Best Practices

1. **Verify invoice accuracy** - Double-check all details before creating
2. **Set realistic due dates** - Match your actual payment terms
3. **Repay early when possible** - Improves credit score faster
4. **Maintain open communication** - Update investors if issues arise
5. **Start small** - Test with smaller invoices first
6. **Build gradually** - Establish trust with consistent repayments

---

**Next Steps:**
- Read the [Investor Guide](./investor-guide.md) to understand the other side
- Check [FAQ](./faq.md) for common questions
- Visit [Troubleshooting](./troubleshooting.md) if you encounter issues
