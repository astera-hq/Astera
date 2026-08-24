import type { xdr } from '@stellar/stellar-sdk';

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

export interface RateModelConfig {
  baseRateBps: number;
  optimalUtilizationBps: number;
  slope1Bps: number;
  slope2Bps: number;
  maxRateBps: number;
}

export interface RateSnapshot {
  timestamp: number;
  utilizationBps: number;
  rateBps: number;
}

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
  estimatedWaitSecs: number;
}

export interface LiquidityForecastPoint {
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
  coFundingRoundId?: bigint;
}

export type CoFundingStatus = 'Open' | 'Filled' | 'Cancelled' | 'Expired';

// #1036: multi-asset, oracle-priced collateral risk response
export interface CollateralConfig {
  threshold: bigint;
  collateralBps: number;
}

export interface CollateralDeposit {
  invoiceId: bigint;
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

export interface CollateralRiskConfig {
  /** Live collateral ratio (bps) below which a position is flagged at-risk. */
  dangerBps: number;
  /** Seconds a depositor has to top up before liquidateCollateral is callable. */
  gracePeriodSecs: number;
}

export type CollateralSaleStatus = 'Open' | 'Settled' | 'Expired';

/** #1036: a seized collateral asset up for sale on the auction contract's
 * Dutch/declining-price liquidation mechanism. */
export interface CollateralSale {
  saleId: bigint;
  seller: string;
  token: string;
  amount: bigint;
  proceedsToken: string;
  proceedsRecipient: string;
  startPrice: bigint;
  floorPrice: bigint;
  openedAt: number;
  durationSecs: number;
  status: CollateralSaleStatus;
  taker?: string;
  settledPrice?: bigint;
}

// #1025: secondary market
export type ListingStatus = 'Open' | 'Filled' | 'Cancelled';
export type ListingKind = 'CoFunding' | 'SingleFunded';

export interface Listing {
  listingId: bigint;
  invoiceId: bigint;
  seller: string;
  token: string;
  kind: ListingKind;
  /** bps of CoFundShare (CoFunding) or raw token amount (SingleFunded) */
  amountOrBps: bigint;
  price: bigint;
  createdAt: number;
  status: ListingStatus;
}

// #1035: order-book, sitting alongside the #1025 fixed-price listing flow above.
export type OrderSide = 'Bid' | 'Ask';
export type OrderStatus = 'Open' | 'PartiallyFilled' | 'Filled' | 'Cancelled' | 'Expired';

export interface Order {
  orderId: bigint;
  invoiceId: bigint;
  owner: string;
  token: string;
  kind: ListingKind;
  side: OrderSide;
  /** Per-unit price, scaled by 1e7 (`PRICE_SCALE` on-chain) — not a flat total like `Listing.price`. */
  price: bigint;
  amountOrBps: bigint;
  remaining: bigint;
  createdAt: number;
  /** Ledger timestamp after which the order can no longer match; 0 means no expiry. */
  expiresAt: number;
  status: OrderStatus;
}

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

export interface ClientConfig {
  rpcUrl: string;
  network: string;
  contractId: string;
  signer?: Signer;
}

export type Signer = (txXdr: string) => Promise<string>;

export interface AsteraConfig {
  rpcUrl: string;
  network: string;
  invoiceContractId: string;
  poolContractId: string;
  /** #1044: secondary-market listing + withdrawal-wait/liquidity-forecast satellite contract. */
  secondaryMarketContractId?: string;
  /** #1036: collateral-liquidation Dutch auction + risk-response satellite contract. */
  auctionContractId?: string;
  creditScoreContractId?: string;
  oracleRegistryContractId?: string;
  complianceContractId?: string;
  trancheContractId?: string;
  /** #864: role-based multisig access-control contract, if deployed. */
  accessControlContractId?: string;
  /** #1055: default-insurance reserve contract, if deployed. */
  insuranceContractId?: string;
  /** Share-token contract, if deployed. */
  shareContractId?: string;
}

// ─── #864: role-based multisig access control ──────────────────────────────
// Mirrors contracts/access_control/src/lib.rs's public types.

export type Role =
  | 'SuperAdmin'
  | 'RiskManager'
  | 'TreasuryManager'
  | 'ComplianceOfficer'
  | 'OracleManager';

export const ALL_ROLES: Role[] = [
  'SuperAdmin',
  'RiskManager',
  'TreasuryManager',
  'ComplianceOfficer',
  'OracleManager',
];

/** Human-readable label for one of the five fixed roles, for admin UI. */
export const ROLE_LABELS: Record<Role, string> = {
  SuperAdmin: 'Super Admin',
  RiskManager: 'Risk Manager',
  TreasuryManager: 'Treasury Manager',
  ComplianceOfficer: 'Compliance Officer',
  OracleManager: 'Oracle Manager',
};

export interface MultiSigConfig {
  signers: string[];
  threshold: number;
}

export type ProposalStatus = 'Pending' | 'Approved' | 'Executed' | 'Rejected';

// Mirrors contracts/access_control/src/lib.rs's `ActionPayload` enum. Each
// variant's `values` tuple matches that Rust variant's fields in order.
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
  target: string;
  action: ActionPayload;
  proposer: string;
  approvals: string[];
  createdAt: bigint;
  expiresAt: bigint;
  status: ProposalStatus;
}

export type ComplianceStatus =
  | 'Unscreened'
  | 'Cleared'
  | 'Flagged'
  | 'Blocked'
  | 'PendingReview';

export type RiskTier = 'Low' | 'Medium' | 'High';

export interface ComplianceRecord {
  address: string;
  status: ComplianceStatus;
  reasonCode: number;
  riskTier: RiskTier;
  screenedAt: number;
  screenedBy: string;
  expiresAt: number;
  notesHash: string;
}

export interface ScreeningHistoryEntry {
  status: ComplianceStatus;
  reasonCode: number;
  riskTier: RiskTier;
  screenedAt: number;
  screenedBy: string;
  expiresAt: number;
  notesHash: string;
}

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

export interface RegistryConfig {
  minStake: bigint;
  stakeToken: string;
  requiredVotes: number;
  quorumBps: number;
  roundDurationSecs: number;
  deregisterCooldownSecs: number;
  treasury?: string;
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

export interface ContractCallParams {
  signer: Signer;
  caller: string;
  onProgress?: (progress: TransactionProgress) => void;
}

export interface PoolDepositEvent {
  depositor: string;
  token: string;
  amount: bigint;
  sharesMinted: bigint;
  timestamp: number;
}

export interface PoolWithdrawEvent {
  withdrawer: string;
  token: string;
  amount: bigint;
  sharesBurned: bigint;
  timestamp: number;
}

export interface PoolRepaidEvent {
  invoiceId: bigint;
  payer: string;
  principal: bigint;
  interest: bigint;
  timestamp: number;
}

export interface PoolPartPayEvent {
  invoiceId: bigint;
  payer: string;
  amount: bigint;
  totalRepaid: bigint;
  timestamp: number;
}

export interface InvoiceFundedEvent {
  invoiceId: bigint;
  funder: string;
  timestamp: number;
}

export interface InvoicePaidEvent {
  invoiceId: bigint;
  caller: string;
  timestamp: number;
}

export interface CreditPaymentEvent {
  caller: string;
  sme: string;
  invoiceId: bigint;
  status: string;
  score: number;
  timestamp: number;
}

export interface CreditDefaultEvent {
  caller: string;
  sme: string;
  invoiceId: bigint;
  score: number;
  timestamp: number;
}

// ─── #1043: structured multi-party dispute arbitration ─────────────────────
// Mirrors contracts/arbitration/src/lib.rs's public types. `DisputeResolution`
// intentionally mirrors invoice's own dispute-outcome type by string value —
// `true`/favor-debtor votes resolve to `'InFavorOfDebtor'`.

export type DisputeResolution = 'Pending' | 'InFavorOfSME' | 'InFavorOfDebtor';

export type PartyRole = 'Claimant' | 'Respondent';

export type CaseStatus = 'EvidenceWindow' | 'CommitReveal' | 'Resolved' | 'NoQuorumEscalated';

export interface JurorInfo {
  address: string;
  stakeAmount: bigint;
  stakeToken: string;
  isActive: boolean;
  casesServed: number;
  timesSlashed: number;
  nonRevealStrikes: number;
  registeredAt: number;
  deregisterRequestedAt?: number;
}

export interface EvidenceEntry {
  submitter: string;
  party: PartyRole;
  evidenceHash: string;
  submittedAt: number;
}

export interface DisputeCase {
  id: bigint;
  invoiceId: bigint;
  claimant: string;
  respondent: string;
  amount: bigint;
  openedAt: number;
  evidenceDeadline: number;
  commitDeadline: number;
  revealDeadline: number;
  jurors: string[];
  status: CaseStatus;
  resolution: DisputeResolution;
  retryCount: number;
}

export interface JurorVote {
  commitHash?: string;
  revealedVote?: boolean;
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

// ─── #1196: governance contract ─────────────────────────────────────────────
// Mirrors contracts/governance/src/lib.rs's public types.

export type GovernanceProposalStatus =
  | 'Active'
  | 'Passed'
  | 'Rejected'
  | 'Executed'
  | 'Cancelled'
  | 'Expired';

export type ProposalCategory = 'ParameterChange' | 'Treasury' | 'Critical';

export interface GovernanceProposal {
  id: bigint;
  proposer: string;
  description: string;
  targetContract: string;
  functionName: string;
  calldata: string;
  votesFor: bigint;
  votesAgainst: bigint;
  status: GovernanceProposalStatus;
  createdAt: bigint;
  votingEndsAt: bigint;
  executionDelay: bigint;
  snapshotSupply: bigint;
  passedAt: bigint;
  category: ProposalCategory;
  quorumBps: number;
  passBps: number;
}

export interface GovernanceConfig {
  admin: string;
  shareToken: string;
  votingPeriodSecs: bigint;
  quorumBps: number;
  passBps: number;
  executionDelaySecs: bigint;
  minShareBalance: bigint;
  treasuryQuorumBps: number;
  criticalQuorumBps: number;
}
