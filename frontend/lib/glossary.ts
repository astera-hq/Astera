/**
 * Single source of truth for Stellar/Soroban terminology shown across the
 * app (inline tooltips + the /glossary page). Add a new term by adding an
 * entry here — no other file needs to change.
 */
export interface GlossaryEntry {
  /** Stable identifier used for lookups and #anchors on /glossary. */
  id: string;
  term: string;
  definition: string;
}

export const glossary: GlossaryEntry[] = [
  {
    id: 'stroops',
    term: 'Stroops',
    definition:
      'The smallest unit of XLM (like satoshis for Bitcoin). 1 XLM = 10,000,000 stroops. Token amounts on Stellar are stored and moved in stroops under the hood, even when the UI shows a rounded amount.',
  },
  {
    id: 'ledger',
    term: 'Ledger',
    definition:
      "A single block in Stellar's blockchain. A new ledger closes roughly every 5 seconds, and every transaction is recorded in exactly one ledger.",
  },
  {
    id: 'horizon',
    term: 'Horizon',
    definition:
      "Stellar's API server for querying blockchain data — account balances, transaction history, and ledger info. Astera's frontend talks to Horizon (or a compatible RPC) to read on-chain state.",
  },
  {
    id: 'soroban',
    term: 'Soroban',
    definition:
      "Stellar's smart contract platform. Astera's pool, invoice, and governance logic run as Soroban contracts, similar to how Ethereum uses the EVM.",
  },
  {
    id: 'ttl',
    term: 'TTL',
    definition:
      'Time-to-live: how long a piece of data stays stored on-chain before it needs to be renewed ("bumped") to avoid archival. Soroban contract data can expire if its TTL runs out.',
  },
  {
    id: 'collateral-ratio',
    term: 'Collateral Ratio',
    definition:
      'The collateral required for a funded invoice, shown as a percentage of its principal. This is separate from insurance coverage: collateral is recovered first if an invoice defaults.',
  },
  {
    id: 'credit-score',
    term: 'Credit Score',
    definition:
      'An SME payment-history score from 200 to 850. The default bands are Excellent (800–850), Very Good (740–799), Good (670–739), Fair (580–669), and below Fair (200–579). Higher scores can qualify an SME for more favorable risk pricing.',
  },
  {
    id: 'risk-tier',
    term: 'Risk Tier',
    definition:
      'An inclusive credit-score range used by the insurance reserve to set a premium multiplier. Lower-score tiers carry a higher multiplier; if a score is unavailable or outside every configured tier, the reserve uses its conservative default multiplier.',
  },
  {
    id: 'coverage-bps',
    term: 'Coverage Percentage',
    definition:
      'The portion of an invoice principal that insurance covers, expressed in basis points: 10,000 bps equals 100%. A coverage record stores this percentage and its maximum payout is still limited by the remaining shortfall and the reserve balance.',
  },
  {
    id: 'coverage-ratio',
    term: 'Reserve Coverage Ratio',
    definition:
      'The insurance reserve’s total reserves divided by its total outstanding covered exposure, expressed in basis points. 10,000 bps means the reserve is backed 1:1; it is not the collateral ratio or the percentage of an individual invoice insured.',
  },
  {
    id: 'factoring-fee',
    term: 'Factoring Fee',
    definition:
      "The protocol's fee for facilitating invoice financing — the cost of getting paid early on an invoice, deducted from the amount funded.",
  },
];

export function getGlossaryEntry(id: string): GlossaryEntry | undefined {
  return glossary.find((entry) => entry.id === id);
}
