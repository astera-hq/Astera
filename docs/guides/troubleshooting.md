# Troubleshooting Guide

Common issues and their solutions when using Astera.

## Table of Contents

1. [Wallet Issues](#wallet-issues)
2. [Transaction Failures](#transaction-failures)
3. [Invoice Issues](#invoice-issues)
4. [Investment Issues](#investment-issues)
5. [Network Issues](#network-issues)
6. [UI/Display Issues](#uidisplay-issues)

---

## Wallet Issues

### "Freighter is not installed"

**Problem**: The app can't detect your wallet.

**Solutions**:
1. Install Freighter from [freighter.app](https://www.freighter.app/)
2. Refresh the page after installation
3. Check that the extension is enabled in your browser
4. Try a hard refresh (Ctrl/Cmd + Shift + R)

### "User rejected request"

**Problem**: You clicked "Reject" in Freighter or closed the popup.

**Solutions**:
1. Try the action again
2. When Freighter opens, carefully review the transaction
3. Click "Approve" to proceed
4. Make sure you're on the correct website

### "Wallet locked"

**Problem**: Freighter is locked and needs your password.

**Solutions**:
1. Click the Freighter extension icon
2. Enter your password to unlock
3. Try the action again

### Wrong account active

**Problem**: Freighter is using a different account than expected.

**Solutions**:
1. Click the Freighter icon
2. Switch to the correct account
3. Refresh the page
4. Reconnect wallet if needed

---

## Transaction Failures

### "Insufficient balance"

**Problem**: Not enough tokens for the transaction.

**Solutions**:
| Scenario | Solution |
|----------|----------|
| Creating invoice | Ensure you have XLM for fees |
| Depositing | Ensure you have the token amount + XLM for fees |
| Committing | Ensure you have available balance in that token |
| Repaying | Ensure you have enough tokens to cover principal + interest |

### "amount must be positive"

**Problem**: Trying to create invoice or deposit with zero/negative amount.

**Solutions**:
1. Enter a positive number greater than 0
2. Check for extra characters or spaces
3. Ensure proper decimal format

### "due date must be in the future"

**Problem**: Invoice due date is today or in the past.

**Solutions**:
1. Select a future date
2. Check your system clock is correct
3. Add at least 1 day buffer

### "unauthorized"

**Problem**: You don't have permission for this action.

**Common causes**:
- Trying to mark someone else's invoice as paid
- Trying to perform admin actions
- Wrong wallet address

### "invoice not found"

**Problem**: The invoice ID doesn't exist.

**Solutions**:
1. Check the invoice ID is correct
2. Verify the invoice was created successfully
3. Check if you're on the right network (testnet vs mainnet)

---

## Invoice Issues

### Invoice stuck in "Pending"

**Problem**: Invoice created but not getting funded.

**Possible reasons**:
- No investors have committed yet
- Oracle verification pending (if enabled)
- Invoice not initialized in pool yet (admin action)

**Solutions**:
1. Wait for investors to discover your invoice
2. Contact admin if oracle verification is stuck
3. Check dashboard for status updates

### Can't mark invoice as paid

**Problem**: "invoice is not funded" error when trying to repay.

**Solutions**:
1. Check invoice status on dashboard
2. If not funded yet, wait for full funding
3. If already paid/defaulted, no further action needed

### "invoice is not in fundable state"

**Problem**: Trying to fund an invoice that can't accept funding.

**Possible states**:
- Already funded
- Already paid
- Defaulted
- Disputed

**Check**: View invoice details to see current status.

---

## Investment Issues

### "token not accepted"

**Problem**: Trying to deposit a token not in the whitelist.

**Solutions**:
1. Check accepted tokens list in the app
2. Use USDC or other whitelisted tokens
3. Contact admin about adding new tokens

### "insufficient available balance"

**Problem**: Trying to commit more than your available amount.

**Solutions**:
1. Check your position on the Invest page
2. "Available" shows what you can commit
3. Deposit more if needed
4. Wait for deployed funds to return from repaid invoices

### "investor has no position in this invoice token"

**Problem**: Trying to commit to invoice without depositing that token first.

**Solutions**:
1. Deposit the token type needed for that invoice
2. Check the invoice's token on the Invest page
3. Each token type requires a separate deposit

### "amount exceeds remaining funding gap"

**Problem**: Commitment larger than what's needed to fund the invoice.

**Solutions**:
1. Check "Remaining" amount on the invoice
2. Commit only up to that amount
3. Or commit exactly the remaining to complete the funding

### "invoice already fully funded"

**Problem**: Someone else completed the funding while you were preparing.

**Solutions**:
1. Refresh the page
2. Find another invoice to fund
3. Set up notifications for new invoices

---

## Network Issues

### "Network Error" or timeout

**Problem**: Can't connect to Stellar network.

**Solutions**:
1. Check your internet connection
2. Wait a moment and retry
3. The Stellar testnet may be temporarily slow
4. Try refreshing the page

### "Simulation failed"

**Problem**: The transaction simulation returned an error.

**Solutions**:
1. Check the error message for specific issue
2. Verify all inputs are correct
3. Ensure you have sufficient balance
4. Try again after a short wait

### RPC errors

**Problem**: Issues connecting to Soroban RPC.

**Solutions**:
1. The testnet RPC may be experiencing issues
2. Wait and retry
3. Check [Stellar Status](https://status.stellar.org/)

---

## UI/Display Issues

### Page not loading

**Solutions**:
1. Hard refresh (Ctrl/Cmd + Shift + R)
2. Clear browser cache
3. Try a different browser
4. Check console for JavaScript errors

### Data not updating

**Problem**: Dashboard shows old information.

**Solutions**:
1. Manual refresh button if available
2. Page refresh
3. Reconnect wallet
4. Wait a few seconds (block confirmation time)

### Numbers look wrong

**Problem**: Amounts display with too many/few decimals.

**Note**: Stellar uses 7 decimal places. 10000000 = 1.0 USDC. The UI should handle conversion, but raw values may appear in some places.

---

## Still Need Help?

If your issue isn't covered here:

1. **Check the [FAQ](./faq.md)** for general questions
2. **Open a GitHub issue** with:
   - Clear description
   - Steps to reproduce
   - Error messages (exact text)
   - Screenshots
   - Browser and wallet version
   - Network (testnet/mainnet)

3. **Include transaction hash** if available - this helps trace the exact issue

---

## Prevention Tips

1. **Always keep some XLM** for transaction fees (at least 5 XLM)
2. **Double-check amounts** before confirming
3. **Verify due dates** are in the future
4. **Refresh before committing** to see latest status
5. **Start with small amounts** to test the flow
