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

// #1025/#1044: secondary market for pool positions and co-funding shares
// (contracts/secondary_market).

/** Whether a listing covers a co-funded share (bps) or a single-funded position slice (raw amount). */
export type ListingKind = 'CoFunding' | 'SingleFunded';

export type ListingStatus = 'Open' | 'Filled' | 'Cancelled';

/** A secondary-market listing created by `list_position`. */
export interface Listing {
  listingId: number;
  invoiceId: number;
  seller: string;
  token: string;
  kind: ListingKind;
  /** bps of the seller's co-fund share (CoFunding) or raw token amount of deployed principal (SingleFunded). */
  amountOrBps: bigint;
  /** Flat token amount the buyer must pay. */
  price: bigint;
  createdAt: number;
  status: ListingStatus;
}

// #1035: limit order book, sitting alongside the #1025 fixed-price listing
// flow above rather than replacing it.

export type OrderSide = 'Bid' | 'Ask';

export type OrderStatus = 'Open' | 'PartiallyFilled' | 'Filled' | 'Cancelled' | 'Expired';

/** A resting or partially-filled limit order created by `place_order`. */
export interface Order {
  orderId: number;
  invoiceId: number;
  owner: string;
  token: string;
  kind: ListingKind;
  side: OrderSide;
  /** Per-unit price, scaled by 1e7 — not a flat total like `Listing.price`. */
  price: bigint;
  amountOrBps: bigint;
  remaining: bigint;
  createdAt: number;
  /** Ledger timestamp after which the order can no longer match; 0 means no expiry. */
  expiresAt: number;
  status: OrderStatus;
}

// ── #863: utilization-driven kinked interest-rate model ─────────────────────

/** Curve parameters for one token, as returned by `get_rate_model_config`. */
export interface RateModelConfig {
  /** Rate (bps) at 0% utilization. */
  baseRateBps: number;
  /** The "kink" point in bps (e.g. 8000 = 80%). */
  optimalUtilizationBps: number;
  /** Rate increase (bps) spread across the 0..optimal span. */
  slope1Bps: number;
  /** Rate increase (bps) spread across the optimal..100% span (steeper). */
  slope2Bps: number;
  /** Hard ceiling on the computed rate. */
  maxRateBps: number;
}

/** One sample from the on-chain rate-history ring buffer. */
export interface RateSnapshot {
  timestamp: number;
  utilizationBps: number;
  rateBps: number;
}

export type ProposalStatus =
  'Active' | 'Passed' | 'Rejected' | 'Executed' | 'Cancelled' | 'Expired';

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
  postedAt: number;
  releasedAt: number;
  seizedAt: number;
  collateralBpsAtDeposit: number;
  thresholdAtDeposit: bigint;
}

// #1036: multi-asset, oracle-priced collateral risk response
export interface CollateralRiskConfig {
  dangerBps: number;
  gracePeriodSecs: number;
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

// #868: credit_score v2 — external attestations + dispute mechanism
export type AttestorType = 'BusinessRegistry' | 'CreditBureau' | 'ExternalProtocol' | 'Manual';
export type AttestationStatus = 'Active' | 'Disputed' | 'Revoked' | 'Expired';

export interface AttestorInfo {
  address: StellarAddress;
  attestorType: AttestorType;
  isActive: boolean;
  weightBps: number;
  registeredAt: number;
}

export interface Attestation {
  id: number;
  sme: StellarAddress;
  attestor: StellarAddress;
  attestationType: AttestorType;
  scoreContribution: number;
  evidenceHash: string;
  submittedAt: number;
  expiresAt: number;
  status: AttestationStatus;
}

/** Indexer risk signal data from GET /credit-score/:sme/risk-signals. */
export interface RiskSignal {
  sme: StellarAddress;
  debtorConcentrationBps: number;
  invoiceSizeRiskBps: number;
  totalVolume: string;
  updatedAt: number;
}

/** Full `get_credit_score` response, including the #868 internal/external blend. */
export interface FullCreditScore {
  sme: StellarAddress;
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
  blendedScore: number;
  /** #1041: point delta from debtor-concentration/invoice-size risk signals. */
  riskAdjustmentPts: number;
  /** #1041: point delta from the repayment-trend factor. */
  trendAdjustmentPts: number;
  /** #1041: blendedScore + riskAdjustmentPts + trendAdjustmentPts, clamped. */
  finalScore: number;
}

/** #799: referral program stats for a referrer, from `referral.get_stats()`. */
export interface ReferralStats {
  referrer: StellarAddress;
  referralCount: number;
}

// #864: role-based multisig access control
export type Role =
  'SuperAdmin' | 'RiskManager' | 'TreasuryManager' | 'ComplianceOfficer' | 'OracleManager';

export const ALL_ROLES: Role[] = [
  'SuperAdmin',
  'RiskManager',
  'TreasuryManager',
  'ComplianceOfficer',
  'OracleManager',
];

export const ROLE_LABELS: Record<Role, string> = {
  SuperAdmin: 'Super Admin',
  RiskManager: 'Risk Manager',
  TreasuryManager: 'Treasury Manager',
  ComplianceOfficer: 'Compliance Officer',
  OracleManager: 'Oracle Manager',
};

export interface MultiSigConfig {
  signers: StellarAddress[];
  threshold: number;
}

// Named distinctly from `ProposalStatus` above (governance module) since
// access-control proposals have a different status lifecycle.
export type AccessControlProposalStatus = 'Pending' | 'Approved' | 'Executed' | 'Rejected';

/** Mirrors contracts/access_control/src/lib.rs's `ActionPayload` enum. */
export type ActionPayload =
  | { tag: 'SetPaused'; values: [boolean] }
  | { tag: 'SetYield'; values: [number] }
  | { tag: 'SetTreasury'; values: [string] }
  | { tag: 'WithdrawRevenue'; values: [string, bigint] }
  | { tag: 'SetOracleContract'; values: [string] }
  | { tag: 'SetKycRequired'; values: [boolean] }
  | { tag: 'SetInvestorKyc'; values: [string, boolean] }
  | { tag: 'SetMaxUtilization'; values: [number] }
  | { tag: 'SetOracle'; values: [string] }
  | { tag: 'RegisterDebtor'; values: [string, string, bigint] }
  | { tag: 'DeactivateDebtor'; values: [string] }
  | { tag: 'AddKeeper'; values: [string] }
  | { tag: 'SetLateThreshold'; values: [bigint] }
  | { tag: 'SetScoreThresholds'; values: [number, number, number, number] }
  | { tag: 'RegisterAttestor'; values: [string, number, number] }
  // ── oracle_registry (#1042) ──
  | { tag: 'SetOracleRegistryInvoiceContract'; values: [string] }
  | { tag: 'SetOracleRegistryTreasury'; values: [string | undefined] }
  | {
      tag: 'SetOracleRegistryConfig';
      values: [bigint, number, number, bigint, bigint];
    }
  | { tag: 'SetOracleRegistryPaused'; values: [boolean] }
  | { tag: 'SlashOracle'; values: [string, number, bigint, string] }
  | { tag: 'AdminResolveRound'; values: [bigint, boolean, string] }
  // ── compliance (#1042) ──
  | { tag: 'SetCompliancePaused'; values: [boolean] }
  | { tag: 'RegisterScreener'; values: [string] }
  | { tag: 'ConfirmScreenerRegistration'; values: [string] }
  | { tag: 'DeregisterScreener'; values: [string] }
  | { tag: 'SetRescreeningInterval'; values: [bigint] }
  | { tag: 'SetScreenerTimelock'; values: [bigint] }
  // ── governance (#1042) ──
  | { tag: 'UpdateGovernanceConfig'; values: [number, number] }
  | { tag: 'SetCategoryQuorum'; values: [number, number] }
  // ── referral (#1042) ──
  | { tag: 'SetReferralPaused'; values: [boolean] }
  | { tag: 'SetReferralPool'; values: [string] }
  | { tag: 'SetBorrowRewardBps'; values: [number] }
  | { tag: 'SetDepositRewardBps'; values: [number] }
  // ── admin-key rotation (#1042) ──
  | { tag: 'SetInvoiceAccessControl'; values: [string] }
  | { tag: 'SetCreditScoreAccessControl'; values: [string] }
  | { tag: 'SetOracleRegistryAccessControl'; values: [string] }
  | { tag: 'SetComplianceAccessControl'; values: [string] }
  | { tag: 'SetGovernanceAccessControl'; values: [string] }
  | { tag: 'SetReferralAccessControl'; values: [string] }
  | { tag: 'AddSigner'; values: [Role, string] }
  | { tag: 'RemoveSigner'; values: [Role, string] }
  | { tag: 'SetThreshold'; values: [Role, number] };

export interface Proposal {
  role: Role;
  target: StellarAddress;
  action: ActionPayload;
  proposer: StellarAddress;
  approvals: StellarAddress[];
  createdAt: number;
  expiresAt: number;
  status: AccessControlProposalStatus;
}

// #1043: structured multi-party dispute arbitration

export interface DisputeRecord {
  evidenceHash: string;
  filedAt: number;
  resolvedAt: number;
  outcome: DisputeResolution;
}

export type DisputeResolution = 'Pending' | 'InFavorOfSME' | 'InFavorOfDebtor';

export type PartyRole = 'Claimant' | 'Respondent';

export type CaseStatus = 'EvidenceWindow' | 'CommitReveal' | 'Resolved' | 'NoQuorumEscalated';

export interface JurorInfo {
  address: StellarAddress;
  stakeAmount: bigint;
  stakeToken: string;
  isActive: boolean;
  casesServed: number;
  timesSlashed: number;
  nonRevealStrikes: number;
  registeredAt: number;
  deregisterRequestedAt: number | null;
}

export interface EvidenceEntry {
  submitter: StellarAddress;
  party: PartyRole;
  evidenceHash: string;
  submittedAt: number;
}

export interface DisputeCase {
  id: number;
  invoiceId: number;
  claimant: StellarAddress;
  respondent: StellarAddress;
  amount: bigint;
  openedAt: number;
  evidenceDeadline: number;
  commitDeadline: number;
  revealDeadline: number;
  jurors: StellarAddress[];
  status: CaseStatus;
  resolution: DisputeResolution;
  retryCount: number;
}

export interface JurorVoteStatus {
  hasCommitted: boolean;
  revealedVote: boolean | null;
}

export interface ArbitrationConfig {
  minStake: bigint;
  stakeToken: string;
  committeeSize: number;
  quorumFloor: number;
  evidenceWindowSecs: number;
  commitWindowSecs: number;
  revealWindowSecs: number;
  deregisterCooldownSecs: number;
  slashBps: number;
  nonRevealSlashBps: number;
  lopsidedConfidenceBps: number;
  maxRetries: number;
}
