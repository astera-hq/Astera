# Glossary

Terms and definitions used in the Astera platform.

## A

**APY (Annual Percentage Yield)**
The annual rate of return earned on an investment, accounting for compound interest. Astera's default APY is 8%.

**Available Balance**
The portion of your deposited funds that is not currently committed to any invoice. Can be withdrawn anytime.

## B

**BPS (Basis Points)**
One hundredth of a percent (0.01%). Used for precise interest rate calculations. 100 bps = 1%.

## C

**Co-funding**
The process where multiple investors commit funds to reach an invoice's principal target. When fully funded, the SME receives the capital.

**Co-funders**
Investors who have committed funds to a specific invoice.

**Credit Score**
An on-chain score (200-850) representing an SME's payment history and trustworthiness. Higher scores indicate better borrowers.

**Committed**
The total amount investors have pledged toward an invoice's principal target.

## D

**Defaulted**
Status when an invoice is not repaid within the late payment threshold (7 days past due). Results in -50 credit score points.

**Deployed**
The portion of an investor's position that is currently committed to active invoices. Locked until repayment.

**Debtor**
The customer or entity that owes payment on an invoice. Listed by name in the invoice details.

**Due Date**
The date by which an invoice should be repaid. Set by the SME when creating the invoice.

## E

**Earned**
The total yield an investor has accumulated from repaid invoices.

## F

**Freighter**
The recommended browser extension wallet for Stellar. Used to sign transactions on Astera.

**Funded**
Invoice status when the principal target has been fully committed and capital transferred to the SME.

**Funding Pool**
The smart contract that manages investor deposits, commitments, and repayments.

## I

**Invoice**
A tokenized representation of money owed to an SME by a customer. Created on-chain with amount, debtor, and due date.

**Invoice ID**
A unique numeric identifier for each invoice on the platform.

**Investor**
A user who deposits stablecoins into the pool and commits funds to invoices to earn yield.

## L

**Ledger**
Stellar's distributed ledger that records all transactions. The "ledger timestamp" is the current time on the blockchain.

**Liquidity Pool**
The pool of stablecoins deposited by investors, available for funding invoices.

## O

**Oracle**
An optional external verifier that can validate invoice authenticity before funding is allowed.

**On-chain**
Data or transactions recorded on the blockchain, visible to all and immutable.

## P

**Paid**
Invoice status when the SME has fully repaid principal + interest.

**Paid Late**
Invoice status when repaid after due date but within 7 days. Results in +15 credit score points.

**Paid On Time**
Invoice status when repaid on or before due date. Results in +30 credit score points.

**Pending**
Initial invoice status after creation, awaiting verification and/or funding.

**Principal**
The original amount of an invoice (excluding interest). The target funding amount.

## R

**Repayment**
The act of an SME paying back the principal plus interest to the pool.

**RWA (Real World Asset)**
A tangible or intangible asset from the traditional financial world that is tokenized on a blockchain. Invoices are RWAs.

## S

**SAC (Stellar Asset Contract)**
The Soroban contract representing a Stellar asset like USDC.

**SME (Small and Medium Enterprise)**
A business that uses Astera to tokenize invoices and access liquidity.

**Soroban**
Stellar's smart contract platform. Astera contracts are built on Soroban.

**Stablecoin**
A cryptocurrency designed to maintain a stable value, typically pegged to a fiat currency like USD (USDC) or EUR (EURC).

**Status**
The current state of an invoice in its lifecycle (Pending, Funded, Paid, Defaulted, etc.).

## T

**Testnet**
Stellar's testing network. Currently where Astera operates for development and testing.

**Tokenize**
The process of converting a real-world asset (like an invoice) into a digital token on the blockchain.

**TTL (Time To Live)**
How long data is stored on-chain before being archived. Active invoices have longer TTL than completed ones.

## V

**Verified**
Invoice status after oracle approval (if oracle is configured). Indicates the invoice has been validated.

**Verification Hash**
An optional cryptographic hash linking an invoice to an off-chain document for verification purposes.

## W

**Whitelist**
The list of approved stablecoins that can be deposited into the pool. Managed by the admin.

**Withdraw**
The action of removing available funds from the pool back to your wallet.

## Y

**Yield**
The return earned by investors on their committed capital, typically expressed as APY.

---

## Acronyms Summary

| Acronym | Full Term |
|---------|-----------|
| APY | Annual Percentage Yield |
| BPS | Basis Points |
| RWA | Real World Asset |
| SAC | Stellar Asset Contract |
| SME | Small and Medium Enterprise |
| TTL | Time To Live |

---

**See also:**
- [SME Guide](./sme-guide.md)
- [Investor Guide](./investor-guide.md)
- [FAQ](./faq.md)
- [Troubleshooting](./troubleshooting.md)
