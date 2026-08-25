export enum GovernanceError {
  NotInitialized = 1,
  ProposalNotFound = 2,
  ProposalInactive = 3,
  AlreadyVoted = 4,
  InsufficientShareBalance = 5,
  VotingPeriodActive = 6,
  TimelockActive = 7,
  QuorumNotMet = 8,
  InvalidProposalState = 9,
  Unauthorized = 10,
  GovernanceNotConfigured = 11,
}

export enum ProposalStatus {
  Active = 0,
  Passed = 1,
  Rejected = 2,
  Executed = 3,
  Cancelled = 4,
  Expired = 5,
}

export enum ProposalCategory {
  ParameterChange = 0,
  Treasury = 1,
  Critical = 2,
}

export interface LoyaltyTier {
  days_threshold: number;
  bonus_bps: number;
}

export interface FeeTier {
  min_amount: bigint;
  max_amount: bigint;
  min_credit_score: number;
  fee_bps: number;
}

export interface CollateralConfig {
  threshold: bigint;
  collateral_bps: number;
}

export interface QuorumTier {
  min_invoice_amount: bigint;
  quorum_bps: number;
}

export type GovernanceAction =
  | { type: 'SetPoolYield'; value: number }
  | { type: 'SetPoolTreasury'; value: string }
  | { type: 'SetPoolMaxUtilization'; value: number }
  | { type: 'SetPoolOracleContract'; value: string }
  | { type: 'SetPoolKycRequired'; value: boolean }
  | { type: 'SetPoolComplianceRegistry'; value: string }
  | { type: 'SetPoolRequireComplianceCheck'; value: boolean }
  | { type: 'SetPoolReferralRegistry'; value: string }
  | { type: 'SetPoolCreditScoreContract'; value: string }
  | { type: 'SetPoolInsuranceContract'; value: string }
  | { type: 'SetPoolCompoundInterest'; value: boolean }
  | { type: 'SetPoolSecondaryMarketContract'; value: string }
  | { type: 'SetPoolRiskContract'; value: string }
  | { type: 'SetPoolMinDeposit'; value: bigint }
  | { type: 'SetPoolMaxInvestorConcentration'; value: number }
  | { type: 'SetPoolUpgradeTimelock'; value: number }
  | { type: 'SetPoolOperationDelay'; value: number }
  | { type: 'SetPoolYieldChangePolicy'; value: number }
  | { type: 'SetPoolFactoringFee'; value: number }
  | { type: 'SetPoolWithdrawalLimits'; value: number }
  | { type: 'SetPoolMaxWithdrawalQueueAge'; value: number }
  | { type: 'SetPoolMaxWithdrawalQueueDepth'; value: number }
  | { type: 'SetPoolOracleStaleThreshold'; value: number }
  | { type: 'SetPoolFeeTier'; tier_id: number; tier: FeeTier }
  | { type: 'SetPoolLoyaltyTiers'; tiers: LoyaltyTier[] }
  | { type: 'SetPoolFallbackPrice'; token: string; price: bigint }
  | { type: 'SetPoolRateBounds'; token: string; min_rate: bigint; max_rate: bigint }
  | { type: 'SetPoolExchangeRate'; token: string; rate: bigint }
  | { type: 'SetPoolCollateralConfig'; config: CollateralConfig }
  | { type: 'SetInvoiceGracePeriod'; value: number }
  | { type: 'SetInvoiceMaxAmount'; value: bigint }
  | { type: 'SetInvoiceMaxSmeOutstanding'; value: bigint }
  | { type: 'SetInvoiceExpirationDuration'; value: number }
  | { type: 'SetInvoiceCompletedTtl'; value: number }
  | { type: 'SetInvoiceDailyLimit'; value: number }
  | { type: 'SetInvoiceDisputeWindow'; value: number }
  | { type: 'SetInvoiceOracle'; value: string }
  | { type: 'SetInvoiceSecondaryOracle'; value: string | null }
  | { type: 'SetInvoiceOracleRegistry'; value: string }
  | { type: 'SetInvoiceConsensusRequired'; value: boolean }
  | { type: 'SetInvoiceComplianceRegistry'; value: string }
  | { type: 'SetInvoiceRequireComplianceCheck'; value: boolean }
  | { type: 'SetInvoiceRequireRegisteredDebtor'; value: boolean }
  | { type: 'SetInvoiceOracleVerifiedFundingOnly'; value: boolean }
  | { type: 'SetInvoiceArbitrationContract'; value: string }
  | { type: 'SetInvoiceDisputeValueThreshold'; value: bigint }
  | { type: 'SetInvoiceMetadataImageUri'; value: string }
  | { type: 'SetInvoiceMinDueDateWindow'; value: number }
  | { type: 'SetOracleRegistryInvoiceContract'; value: string }
  | { type: 'SetOracleRegistryTreasury'; value: string | null }
  | { type: 'SetOracleRegistryConfig'; min_stake: bigint; required_votes: number; quorum_bps: number; round_duration_secs: number; deregister_cooldown_secs: number }
  | { type: 'SetOracleRegistryQuorumTiers'; tiers: QuorumTier[] }
  | { type: 'SetComplianceRescreeningInterval'; value: number }
  | { type: 'SetComplianceScreenerTimelock'; value: number };

export interface Proposal {
  id: bigint;
  proposer: string;
  description: string;
  target_contract: string;
  action: GovernanceAction;
  votes_for: bigint;
  votes_against: bigint;
  status: ProposalStatus;
  created_at: number;
  voting_ends_at: number;
  execution_delay: number;
  snapshot_supply: bigint;
  passed_at: number;
  category: ProposalCategory;
  quorum_bps: number;
  pass_bps: number;
}

export interface GovernanceConfig {
  admin: string;
  share_token: string;
  min_voting_period_secs: number;
  default_execution_delay_secs: number;
  parameter_change_quorum_bps: number;
  treasury_quorum_bps: number;
  critical_quorum_bps: number;
  parameter_change_pass_bps: number;
  treasury_pass_bps: number;
  critical_pass_bps: number;
  execution_expiry_secs: number;
}
