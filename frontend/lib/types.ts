export type InvoiceStatus =
  | 'Pending'
  | 'AwaitingVerification'
  | 'Verified'
  | 'Disputed'
  | 'Funded'
  | 'Paid'
  | 'Defaulted'
  | 'Cancelled'
  | 'Expired';

/** On-chain view from `get_metadata` (SEP-oriented display fields). */
export interface InvoiceMetadata {
  name: string;
  description: string;
  image: string;
  amount: bigint;
  debtor: string;
  dueDate: number;
  status: InvoiceStatus;
  symbol: string;
  decimals: number;
}

export interface Invoice {
  id: number;
  owner: string;
  debtor: string;
  amount: bigint;
  dueDate: number;
  description: string;
  status: InvoiceStatus;
  createdAt: number;
  fundedAt: number;
  paidAt: number;
  poolContract: string;
  verificationHash?: string;
  metadataUri?: string | null;
  oracleVerified?: boolean;
  disputeReason?: string;
  disputedAt?: number;
  gracePeriodOverride?: number | null;
}

export interface InvestorPosition {
  deposited: bigint;
  available: bigint;
  deployed: bigint;
  earned: bigint;
  depositCount: number;
}

export interface PoolConfig {
  invoiceContract: string;
  admin: string;
  yieldBps: number;
  factoringFeeBps: number;
  compoundInterest: boolean;
  // #227: yield timelock
  proposedYieldBps: number;
  yieldProposalAt: number;
  yieldTimelockSecs: number;
  // #233: max single-investor concentration
  maxSingleInvestorBps: number;
}

export interface PoolTokenTotals {
  totalDeposited: bigint;
  totalDeployed: bigint;
  totalPaidOut: bigint;
  totalFeeRevenue: bigint;
}

export type ProposalStatus = 'Active' | 'Passed' | 'Rejected' | 'Executed' | 'Cancelled';

export interface GovernanceProposal {
  id: number;
  proposer: string;
  description: string;
  targetContract: string;
  functionName: string;
  calldata: string;
  votesFor: bigint;
  votesAgainst: bigint;
  status: ProposalStatus;
  createdAt: number;
  votingEndsAt: number;
  executionDelay: number;
}

export interface InvoiceTtlWarning {
  id: number;
  status: InvoiceStatus;
  expiryLedger: number;
  remainingDays: number;
  severity: 'low' | 'medium' | 'high';
}

export interface FundedInvoice {
  invoiceId: number;
  sme: string;
  /** Stablecoin contract used for this invoice */
  token: string;
  principal: bigint;
  committed: bigint;
  fundedAt: number;
  factoringFee: bigint;
  dueDate: number;
  /** Total amount repaid so far (supports partial repayments) */
  repaidAmount: bigint;
}

export type WalletState = {
  address: string | null;
  connected: boolean;
  network: string;
};

export interface CollateralConfig {
  threshold: bigint;
  collateralBps: number;
}

export interface CollateralDeposit {
  invoiceId: number;
  depositor: string;
  token: string;
  amount: bigint;
  settled: boolean;
}
