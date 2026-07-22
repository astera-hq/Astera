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
  /** #860: set when this invoice was funded through a co-funding round. */
  coFundingRoundId?: bigint;
}

// #860: multi-investor co-funding rounds
export type CoFundingStatus = 'Open' | 'Filled' | 'Cancelled' | 'Expired';

export interface CoFundingRound {
  invoiceId: bigint;
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

// #868: credit_score v2 — external attestations + dispute mechanism
export type AttestorType = 'BusinessRegistry' | 'CreditBureau' | 'ExternalProtocol' | 'Manual';
export type AttestationStatus = 'Active' | 'Disputed' | 'Revoked' | 'Expired';

export interface AttestorInfo {
  address: string;
  attestorType: AttestorType;
  isActive: boolean;
  weightBps: number;
  registeredAt: number;
}

export interface Attestation {
  id: bigint;
  sme: string;
  attestor: string;
  attestationType: AttestorType;
  scoreContribution: number;
  evidenceHash: string;
  submittedAt: number;
  expiresAt: number;
  status: AttestationStatus;
}

export interface CreditScoreResponse {
  sme: string;
  score: number;
  totalInvoices: number;
  paidOnTime: number;
  paidLate: number;
  defaulted: number;
  totalVolume: bigint;
  averagePaymentDays: number;
  lastUpdated: number;
  scoreVersion: number;
  configVersion: number;
  isStale: boolean;
  /** Internal score blended with the SME's active external attestations. */
  blendedScore: number;
}
