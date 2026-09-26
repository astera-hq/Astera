import { rpc as StellarRpc, nativeToScVal, scValToNative, Address, xdr } from '@stellar/stellar-sdk';
import { BaseClient } from './base';
import { GovernanceError, Proposal, ProposalStatus, ProposalCategory, GovernanceAction, GovernanceConfig } from '../generated/governance';
import type { ClientConfig, Signer } from '../types';

const SIMULATION_SOURCE = 'GovernanceClient';

export class GovernanceClient extends BaseClient {
  protected override readonly errors = {
    [GovernanceError.NotInitialized]: { message: 'Governance contract not initialized' },
    [GovernanceError.ProposalNotFound]: { message: 'Proposal not found' },
    [GovernanceError.ProposalInactive]: { message: 'Proposal is not active' },
    [GovernanceError.AlreadyVoted]: { message: 'Already voted on this proposal' },
    [GovernanceError.InsufficientShareBalance]: { message: 'Insufficient share balance' },
    [GovernanceError.VotingPeriodActive]: { message: 'Voting period is still active' },
    [GovernanceError.TimelockActive]: { message: 'Timelock is still active' },
    [GovernanceError.QuorumNotMet]: { message: 'Quorum not met' },
    [GovernanceError.InvalidProposalState]: { message: 'Invalid proposal state' },
    [GovernanceError.Unauthorized]: { message: 'Unauthorized' },
    [GovernanceError.GovernanceNotConfigured]: { message: 'Governance not configured' },
  };

  constructor(config: ClientConfig) {
    super(config);
  }

  /**
   * Initialize the governance contract
   */
  async initialize(params: {
    admin: string;
    shareToken: string;
    minVotingPeriodSecs: number;
    defaultExecutionDelaySecs: number;
    parameterChangeQuorumBps: number;
    treasuryQuorumBps: number;
    criticalQuorumBps: number;
    parameterChangePassBps: number;
    treasuryPassBps: number;
    criticalPassBps: number;
    executionExpirySecs: number;
  }, caller: string, signer?: Signer): Promise<string> {
    const args = [
      new Address(params.admin).toScVal(),
      new Address(params.shareToken).toScVal(),
      nativeToScVal(params.minVotingPeriodSecs, { type: 'u64' }),
      nativeToScVal(params.defaultExecutionDelaySecs, { type: 'u64' }),
      nativeToScVal(params.parameterChangeQuorumBps, { type: 'u32' }),
      nativeToScVal(params.treasuryQuorumBps, { type: 'u32' }),
      nativeToScVal(params.criticalQuorumBps, { type: 'u32' }),
      nativeToScVal(params.parameterChangePassBps, { type: 'u32' }),
      nativeToScVal(params.treasuryPassBps, { type: 'u32' }),
      nativeToScVal(params.criticalPassBps, { type: 'u32' }),
      nativeToScVal(params.executionExpirySecs, { type: 'u64' }),
    ];

    return this.buildAndSendTx(caller, 'initialize', args);
  }

  /**
   * Create a governance proposal
   */
  async createProposal(params: {
    description: string;
    targetContract: string;
    action: GovernanceAction;
    category: ProposalCategory;
    votingPeriodSecs?: number;
    executionDelaySecs?: number;
  }, caller: string, signer?: Signer): Promise<{ proposalId: bigint; txHash: string }> {
    const actionScVal = this.governanceActionToScVal(params.action);
    
    const args = [
      nativeToScVal(params.description, { type: 'string' }),
      new Address(params.targetContract).toScVal(),
      actionScVal,
      nativeToScVal(params.category, { type: 'u32' }),
      nativeToScVal(params.votingPeriodSecs ?? 0, { type: 'u64' }),
      nativeToScVal(params.executionDelaySecs ?? 0, { type: 'u64' }),
    ];

    const txHash = await this.buildAndSendTx(caller, 'create_proposal', args);
    
    // Get the proposal ID from the result
    const sim = await this.simulate('get_last_proposal_id', []);
    const proposalId = scValToNative(sim.result!.retval) as bigint;
    
    return { proposalId, txHash };
  }

  /**
   * Vote on a proposal
   */
  async vote(params: {
    proposalId: bigint;
    voteFor: boolean;
    shares: bigint;
  }, caller: string, signer?: Signer): Promise<string> {
    const args = [
      nativeToScVal(params.proposalId, { type: 'u64' }),
      nativeToScVal(params.voteFor, { type: 'bool' }),
      nativeToScVal(params.shares, { type: 'i128' }),
    ];

    return this.buildAndSendTx(caller, 'vote', args);
  }

  /**
   * Execute a proposal
   */
  async executeProposal(params: { proposalId: bigint }, caller: string, signer?: Signer): Promise<string> {
    const args = [nativeToScVal(params.proposalId, { type: 'u64' })];
    return this.buildAndSendTx(caller, 'execute_proposal', args);
  }

  /**
   * Cancel a proposal
   */
  async cancelProposal(params: { proposalId: bigint }, caller: string, signer?: Signer): Promise<string> {
    const args = [nativeToScVal(params.proposalId, { type: 'u64' })];
    return this.buildAndSendTx(caller, 'cancel_proposal', args);
  }

  /**
   * Get a proposal by ID
   */
  async getProposal(proposalId: bigint): Promise<Proposal> {
    const sim = await this.simulate('get_proposal', [nativeToScVal(proposalId, { type: 'u64' })]);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    return this.proposalFromScVal(scValToNative(sim.result!.retval));
  }

  /**
   * Get all proposals
   */
  async getAllProposals(): Promise<Proposal[]> {
    const sim = await this.simulate('get_all_proposals', []);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    return scValToNative(sim.result!.retval) as Proposal[];
  }

  /**
   * Get governance configuration
   */
  async getConfig(): Promise<GovernanceConfig> {
    const sim = await this.simulate('get_config', []);
    if (StellarRpc.Api.isSimulationError(sim)) {
      throw new Error(`Simulation failed: ${sim.error}`);
    }
    return this.configFromScVal(scValToNative(sim.result!.retval));
  }

  /**
   * Get the governance contract address from a target contract
   */
  async getGovernanceAddress(): Promise<string | null> {
    try {
      const sim = await this.simulate('get_governance_address', []);
      if (StellarRpc.Api.isSimulationError(sim)) {
        return null;
      }
      const address = scValToNative(sim.result!.retval);
      return address as string | null;
    } catch (e) {
      return null;
    }
  }

  private governanceActionToScVal(action: GovernanceAction): xdr.ScVal {
    const entry = (key: string, val: xdr.ScVal) =>
      new xdr.ScMapEntry({ key: nativeToScVal(key, { type: 'symbol' }), val });

    switch (action.type) {
      case 'SetPoolYield':
        return xdr.ScVal.scvMap([
          entry('SetPoolYield', nativeToScVal(action.value, { type: 'u32' })),
        ]);
      case 'SetPoolTreasury':
        return xdr.ScVal.scvMap([
          entry('SetPoolTreasury', new Address(action.value).toScVal()),
        ]);
      case 'SetPoolMaxUtilization':
        return xdr.ScVal.scvMap([
          entry('SetPoolMaxUtilization', nativeToScVal(action.value, { type: 'u32' })),
        ]);
      case 'SetPoolOracleContract':
        return xdr.ScVal.scvMap([
          entry('SetPoolOracleContract', new Address(action.value).toScVal()),
        ]);
      case 'SetPoolKycRequired':
        return xdr.ScVal.scvMap([
          entry('SetPoolKycRequired', nativeToScVal(action.value, { type: 'bool' })),
        ]);
      case 'SetPoolComplianceRegistry':
        return xdr.ScVal.scvMap([
          entry('SetPoolComplianceRegistry', new Address(action.value).toScVal()),
        ]);
      case 'SetPoolRequireComplianceCheck':
        return xdr.ScVal.scvMap([
          entry('SetPoolRequireComplianceCheck', nativeToScVal(action.value, { type: 'bool' })),
        ]);
      case 'SetPoolReferralRegistry':
        return xdr.ScVal.scvMap([
          entry('SetPoolReferralRegistry', new Address(action.value).toScVal()),
        ]);
      case 'SetPoolCreditScoreContract':
        return xdr.ScVal.scvMap([
          entry('SetPoolCreditScoreContract', new Address(action.value).toScVal()),
        ]);
      case 'SetPoolInsuranceContract':
        return xdr.ScVal.scvMap([
          entry('SetPoolInsuranceContract', new Address(action.value).toScVal()),
        ]);
      case 'SetPoolCompoundInterest':
        return xdr.ScVal.scvMap([
          entry('SetPoolCompoundInterest', nativeToScVal(action.value, { type: 'bool' })),
        ]);
      case 'SetPoolSecondaryMarketContract':
        return xdr.ScVal.scvMap([
          entry('SetPoolSecondaryMarketContract', new Address(action.value).toScVal()),
        ]);
      case 'SetPoolRiskContract':
        return xdr.ScVal.scvMap([
          entry('SetPoolRiskContract', new Address(action.value).toScVal()),
        ]);
      case 'SetPoolMinDeposit':
        return xdr.ScVal.scvMap([
          entry('SetPoolMinDeposit', nativeToScVal(action.value, { type: 'i128' })),
        ]);
      case 'SetPoolMaxInvestorConcentration':
        return xdr.ScVal.scvMap([
          entry('SetPoolMaxInvestorConcentration', nativeToScVal(action.value, { type: 'u32' })),
        ]);
      case 'SetPoolUpgradeTimelock':
        return xdr.ScVal.scvMap([
          entry('SetPoolUpgradeTimelock', nativeToScVal(action.value, { type: 'u64' })),
        ]);
      case 'SetPoolOperationDelay':
        return xdr.ScVal.scvMap([
          entry('SetPoolOperationDelay', nativeToScVal(action.value, { type: 'u64' })),
        ]);
      case 'SetPoolYieldChangePolicy':
        return xdr.ScVal.scvMap([
          entry('SetPoolYieldChangePolicy', nativeToScVal(action.value, { type: 'u64' })),
        ]);
      case 'SetPoolFactoringFee':
        return xdr.ScVal.scvMap([
          entry('SetPoolFactoringFee', nativeToScVal(action.value, { type: 'u32' })),
        ]);
      case 'SetPoolWithdrawalLimits':
        return xdr.ScVal.scvMap([
          entry('SetPoolWithdrawalLimits', nativeToScVal(action.value, { type: 'u32' })),
        ]);
      case 'SetPoolMaxWithdrawalQueueAge':
        return xdr.ScVal.scvMap([
          entry('SetPoolMaxWithdrawalQueueAge', nativeToScVal(action.value, { type: 'u32' })),
        ]);
      case 'SetPoolMaxWithdrawalQueueDepth':
        return xdr.ScVal.scvMap([
          entry('SetPoolMaxWithdrawalQueueDepth', nativeToScVal(action.value, { type: 'u32' })),
        ]);
      case 'SetPoolOracleStaleThreshold':
        return xdr.ScVal.scvMap([
          entry('SetPoolOracleStaleThreshold', nativeToScVal(action.value, { type: 'u64' })),
        ]);
      case 'SetPoolFeeTier':
        return xdr.ScVal.scvMap([
          entry('SetPoolFeeTier', xdr.ScVal.scvMap([
            entry('tier_id', nativeToScVal(action.tier_id, { type: 'u32' })),
            entry('tier', xdr.ScVal.scvMap([
              entry('min_amount', nativeToScVal(action.tier.min_amount, { type: 'i128' })),
              entry('max_amount', nativeToScVal(action.tier.max_amount, { type: 'i128' })),
              entry('min_credit_score', nativeToScVal(action.tier.min_credit_score, { type: 'u32' })),
              entry('fee_bps', nativeToScVal(action.tier.fee_bps, { type: 'u32' })),
            ])),
          ])),
        ]);
      case 'SetPoolLoyaltyTiers':
        const tiersScVal = action.tiers.map(tier =>
          xdr.ScVal.scvMap([
            entry('days_threshold', nativeToScVal(tier.days_threshold, { type: 'u32' })),
            entry('bonus_bps', nativeToScVal(tier.bonus_bps, { type: 'u32' })),
          ])
        );
        return xdr.ScVal.scvMap([
          entry('SetPoolLoyaltyTiers', nativeToScVal(tiersScVal, { type: 'vec' })),
        ]);
      case 'SetPoolFallbackPrice':
        return xdr.ScVal.scvMap([
          entry('SetPoolFallbackPrice', xdr.ScVal.scvMap([
            entry('token', new Address(action.token).toScVal()),
            entry('price', nativeToScVal(action.price, { type: 'i128' })),
          ])),
        ]);
      case 'SetPoolRateBounds':
        return xdr.ScVal.scvMap([
          entry('SetPoolRateBounds', xdr.ScVal.scvMap([
            entry('token', new Address(action.token).toScVal()),
            entry('min_rate', nativeToScVal(action.min_rate, { type: 'i128' })),
            entry('max_rate', nativeToScVal(action.max_rate, { type: 'i128' })),
          ])),
        ]);
      case 'SetPoolExchangeRate':
        return xdr.ScVal.scvMap([
          entry('SetPoolExchangeRate', xdr.ScVal.scvMap([
            entry('token', new Address(action.token).toScVal()),
            entry('rate', nativeToScVal(action.rate, { type: 'i128' })),
          ])),
        ]);
      case 'SetPoolCollateralConfig':
        return xdr.ScVal.scvMap([
          entry('SetPoolCollateralConfig', xdr.ScVal.scvMap([
            entry('threshold', nativeToScVal(action.config.threshold, { type: 'i128' })),
            entry('collateral_bps', nativeToScVal(action.config.collateral_bps, { type: 'u32' })),
          ])),
        ]);
      case 'SetInvoiceGracePeriod':
        return xdr.ScVal.scvMap([
          entry('SetInvoiceGracePeriod', nativeToScVal(action.value, { type: 'u32' })),
        ]);
      case 'SetInvoiceMaxAmount':
        return xdr.ScVal.scvMap([
          entry('SetInvoiceMaxAmount', nativeToScVal(action.value, { type: 'i128' })),
        ]);
      case 'SetInvoiceMaxSmeOutstanding':
        return xdr.ScVal.scvMap([
          entry('SetInvoiceMaxSmeOutstanding', nativeToScVal(action.value, { type: 'i128' })),
        ]);
      case 'SetInvoiceExpirationDuration':
        return xdr.ScVal.scvMap([
          entry('SetInvoiceExpirationDuration', nativeToScVal(action.value, { type: 'u64' })),
        ]);
      case 'SetInvoiceCompletedTtl':
        return xdr.ScVal.scvMap([
          entry('SetInvoiceCompletedTtl', nativeToScVal(action.value, { type: 'u32' })),
        ]);
      case 'SetInvoiceDailyLimit':
        return xdr.ScVal.scvMap([
          entry('SetInvoiceDailyLimit', nativeToScVal(action.value, { type: 'u32' })),
        ]);
      case 'SetInvoiceDisputeWindow':
        return xdr.ScVal.scvMap([
          entry('SetInvoiceDisputeWindow', nativeToScVal(action.value, { type: 'u64' })),
        ]);
      case 'SetInvoiceOracle':
        return xdr.ScVal.scvMap([
          entry('SetInvoiceOracle', new Address(action.value).toScVal()),
        ]);
      case 'SetInvoiceSecondaryOracle':
        return xdr.ScVal.scvMap([
          entry('SetInvoiceSecondaryOracle', action.value ? new Address(action.value).toScVal() : xdr.ScVal.scvVoid()),
        ]);
      case 'SetInvoiceOracleRegistry':
        return xdr.ScVal.scvMap([
          entry('SetInvoiceOracleRegistry', new Address(action.value).toScVal()),
        ]);
      case 'SetInvoiceConsensusRequired':
        return xdr.ScVal.scvMap([
          entry('SetInvoiceConsensusRequired', nativeToScVal(action.value, { type: 'bool' })),
        ]);
      case 'SetInvoiceComplianceRegistry':
        return xdr.ScVal.scvMap([
          entry('SetInvoiceComplianceRegistry', new Address(action.value).toScVal()),
        ]);
      case 'SetInvoiceRequireComplianceCheck':
        return xdr.ScVal.scvMap([
          entry('SetInvoiceRequireComplianceCheck', nativeToScVal(action.value, { type: 'bool' })),
        ]);
      case 'SetInvoiceRequireRegisteredDebtor':
        return xdr.ScVal.scvMap([
          entry('SetInvoiceRequireRegisteredDebtor', nativeToScVal(action.value, { type: 'bool' })),
        ]);
      case 'SetInvoiceOracleVerifiedFundingOnly':
        return xdr.ScVal.scvMap([
          entry('SetInvoiceOracleVerifiedFundingOnly', nativeToScVal(action.value, { type: 'bool' })),
        ]);
      case 'SetInvoiceArbitrationContract':
        return xdr.ScVal.scvMap([
          entry('SetInvoiceArbitrationContract', new Address(action.value).toScVal()),
        ]);
      case 'SetInvoiceDisputeValueThreshold':
        return xdr.ScVal.scvMap([
          entry('SetInvoiceDisputeValueThreshold', nativeToScVal(action.value, { type: 'i128' })),
        ]);
      case 'SetInvoiceMetadataImageUri':
        return xdr.ScVal.scvMap([
          entry('SetInvoiceMetadataImageUri', nativeToScVal(action.value, { type: 'string' })),
        ]);
      case 'SetInvoiceMinDueDateWindow':
        return xdr.ScVal.scvMap([
          entry('SetInvoiceMinDueDateWindow', nativeToScVal(action.value, { type: 'u64' })),
        ]);
      case 'SetOracleRegistryInvoiceContract':
        return xdr.ScVal.scvMap([
          entry('SetOracleRegistryInvoiceContract', new Address(action.value).toScVal()),
        ]);
      case 'SetOracleRegistryTreasury':
        return xdr.ScVal.scvMap([
          entry('SetOracleRegistryTreasury', action.value ? new Address(action.value).toScVal() : xdr.ScVal.scvVoid()),
        ]);
      case 'SetOracleRegistryConfig':
        return xdr.ScVal.scvMap([
          entry('SetOracleRegistryConfig', xdr.ScVal.scvMap([
            entry('min_stake', nativeToScVal(action.min_stake, { type: 'i128' })),
            entry('required_votes', nativeToScVal(action.required_votes, { type: 'u32' })),
            entry('quorum_bps', nativeToScVal(action.quorum_bps, { type: 'u32' })),
            entry('round_duration_secs', nativeToScVal(action.round_duration_secs, { type: 'u64' })),
            entry('deregister_cooldown_secs', nativeToScVal(action.deregister_cooldown_secs, { type: 'u64' })),
          ])),
        ]);
      case 'SetOracleRegistryQuorumTiers':
        const quorumTiersScVal = action.tiers.map(tier =>
          xdr.ScVal.scvMap([
            entry('min_invoice_amount', nativeToScVal(tier.min_invoice_amount, { type: 'i128' })),
            entry('quorum_bps', nativeToScVal(tier.quorum_bps, { type: 'u32' })),
          ])
        );
        return xdr.ScVal.scvMap([
          entry('SetOracleRegistryQuorumTiers', nativeToScVal(quorumTiersScVal, { type: 'vec' })),
        ]);
      case 'SetComplianceRescreeningInterval':
        return xdr.ScVal.scvMap([
          entry('SetComplianceRescreeningInterval', nativeToScVal(action.value, { type: 'u64' })),
        ]);
      case 'SetComplianceScreenerTimelock':
        return xdr.ScVal.scvMap([
          entry('SetComplianceScreenerTimelock', nativeToScVal(action.value, { type: 'u64' })),
        ]);
      default:
        throw new Error(`Unknown governance action type: ${(action as any).type}`);
    }
  }

  private proposalFromScVal(raw: unknown): Proposal {
    const r = raw as Record<string, unknown>;
    return {
      id: BigInt(String(r.id)),
      proposer: r.proposer as string,
      description: r.description as string,
      target_contract: r.target_contract as string,
      action: r.action as GovernanceAction,
      votes_for: BigInt(String(r.votes_for)),
      votes_against: BigInt(String(r.votes_against)),
      status: r.status as ProposalStatus,
      created_at: Number(r.created_at),
      voting_ends_at: Number(r.voting_ends_at),
      execution_delay: Number(r.execution_delay),
      snapshot_supply: BigInt(String(r.snapshot_supply)),
      passed_at: Number(r.passed_at),
      category: r.category as ProposalCategory,
      quorum_bps: Number(r.quorum_bps),
      pass_bps: Number(r.pass_bps),
    };
  }

  private configFromScVal(raw: unknown): GovernanceConfig {
    const r = raw as Record<string, unknown>;
    return {
      admin: r.admin as string,
      share_token: r.share_token as string,
      min_voting_period_secs: Number(r.min_voting_period_secs),
      default_execution_delay_secs: Number(r.default_execution_delay_secs),
      parameter_change_quorum_bps: Number(r.parameter_change_quorum_bps),
      treasury_quorum_bps: Number(r.treasury_quorum_bps),
      critical_quorum_bps: Number(r.critical_quorum_bps),
      parameter_change_pass_bps: Number(r.parameter_change_pass_bps),
      treasury_pass_bps: Number(r.treasury_pass_bps),
      critical_pass_bps: Number(r.critical_pass_bps),
      execution_expiry_secs: Number(r.execution_expiry_secs),
    };
  }
}
