export type InvoiceStatus =
  | 'Pending'
  | 'AwaitingVerification'
  | 'Verified'
  | 'Disputed'
  | 'Funded'
  | 'Paid'
  | 'Defaulted';

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
  id: bigint;
  owner: string;
  debtor: string;
  amount: bigint;
  due_date: number;
  description: string;
  status: InvoiceStatus;
  created_at: number;
  funded_at: number;
  paid_at: number;
  pool_contract: string;
  verification_hash?: string;
  metadata_uri?: string;
  oracle_verified?: boolean;
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
}

// #865: withdrawal-queue completion + liquidity forecasting

export interface WithdrawalRequest {
  investor: string;
  token: string;
  shares: bigint;
  requestedAt: number;
  requestId: bigint;
}

export interface WaitEstimate {
  queuePosition: number;
  capitalAhead: bigint;
  nearestInvoiceDueDate: number;
  /** Predicted seconds until this request is likely to clear. An estimate, not a guarantee. */
  estimatedWaitSecs: number;
}

export interface LiquidityForecastPoint {
  /** Days from now (1-indexed). */
  day: number;
  projectedAvailable: bigint;
}

export interface PoolTokenTotals {
  totalDeposited: bigint;
  totalDeployed: bigint;
  totalPaidOut: bigint;
  totalFeeRevenue: bigint;
}

export interface FundedInvoice {
  invoiceId: bigint;
  sme: string;
  token: string;
  principal: bigint;
  committed: bigint;
  fundedAt: number;
  factoringFee: bigint;
  dueDate: number;
  repaidAmount: bigint;
}

export interface AsteraConfig {
  rpcUrl: string;
  network: string;
  invoiceContractId: string;
  poolContractId: string;
  creditScoreContractId?: string;
  // #861: N-of-M staked oracle consensus network
  oracleRegistryContractId?: string;
}

// #861: N-of-M staked oracle consensus network
export type RoundStatus = 'Open' | 'ConsensusApproved' | 'ConsensusRejected' | 'Expired';

export interface OracleInfo {
  address: string;
  stakeAmount: bigint;
  stakeToken: string;
  isActive: boolean;
  totalVerifications: number;
  totalSlashes: number;
  registeredAt: number;
  deregisterRequestedAt?: number;
}

export interface VerificationRound {
  invoiceId: bigint;
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

export interface TransactionProgress {
  status: 'pending' | 'confirmed' | 'failed';
  hash: string;
  error?: string;
}
