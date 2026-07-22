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
  admin: StellarAddress;
  yieldBps: number;
  factoringFeeBps: number;
  compoundInterest: boolean;
  // #227: yield timelock
  proposedYieldBps: number;
  yieldProposalAt: number;
  yieldTimelockSecs: number;
  // #233: max single-investor concentration
  maxSingleInvestorBps: number;
  maxWithdrawalQueueAgeDays: number;
  // #865: global cap on outstanding withdrawal-queue entries per token (0 = unlimited)
  maxWithdrawalQueueDepth: number;
}

export interface PoolTokenTotals {
  totalDeposited: bigint;
  totalDeployed: bigint;
  totalPaidOut: bigint;
  totalFeeRevenue: bigint;
}

export interface WaitEstimate {
  queuePosition: number;
  capitalAhead: bigint;
  nearestInvoiceDueDate: number;
  /** #865: predicted seconds until this request likely clears. An estimate, not a guarantee. */
  estimatedWaitSecs: number;
}

/** #865: a single pending entry returned by `get_withdrawal_queue`. */
export interface WithdrawalRequest {
  investor: string;
  token: string;
  shares: bigint;
  requestedAt: number;
  requestId: number;
}

/** #865: one projected point returned by `get_liquidity_forecast`. */
export interface LiquidityForecastPoint {
  /** Days from now (1-indexed). */
  day: number;
  projectedAvailable: bigint;
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
  /** #860: set when this invoice was funded through a co-funding round. */
  coFundingRoundId?: number;
}

// #860: multi-investor co-funding rounds
export type CoFundingStatus = 'Open' | 'Filled' | 'Cancelled' | 'Expired';

export interface CoFundingRound {
  invoiceId: number;
  token: string;
  sme: string;
  dueDate: number;
  targetPrincipal: bigint;
  committedPrincipal: bigint;
  fundingDeadline: number;
  status: CoFundingStatus;
  minCommitment: bigint;
  maxInvestorBps: number;
  participants: string[];
}

export type WalletState = {
  address: string | null;
  connected: boolean;
  network: string;
};

export type StellarAddress = string & { readonly _brand: 'StellarAddress' };

export const STELLAR_ADDRESS_REGEX = /^[GC][A-Z2-7]{55}$/;

export function parseStellarAddress(value: string): StellarAddress {
  if (!STELLAR_ADDRESS_REGEX.test(value)) {
    throw new Error(`Invalid Stellar address: ${value}`);
  }
  return value as StellarAddress;
}

export function isStellarAddress(value: string): value is StellarAddress {
  return STELLAR_ADDRESS_REGEX.test(value);
}

export interface GovernanceConfig {
  admin: StellarAddress;
  shareToken: string;
  votingPeriodSecs: number;
  quorumBps: number;
  passBps: number;
  executionDelaySecs: number;
  minShareBalance: bigint;
}

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

// #861: N-of-M staked oracle consensus network
export type RoundStatus = 'Open' | 'ConsensusApproved' | 'ConsensusRejected' | 'Expired';

export interface OracleInfo {
  address: StellarAddress;
  stakeAmount: bigint;
  stakeToken: string;
  isActive: boolean;
  totalVerifications: number;
  totalSlashes: number;
  registeredAt: number;
  deregisterRequestedAt: number | null;
}

export interface VerificationRound {
  invoiceId: number;
  requiredVotes: number;
  totalRegisteredOracles: number;
  weightFor: bigint;
  weightAgainst: bigint;
  totalStakeSnapshot: bigint;
  quorumBps: number;
  status: RoundStatus;
  openedAt: number;
  deadline: number;
  oracleHash: string;
}

export interface OracleRegistryConfig {
  minStake: bigint;
  stakeToken: string;
  requiredVotes: number;
  quorumBps: number;
  roundDurationSecs: number;
  deregisterCooldownSecs: number;
  treasury: string | null;
}
