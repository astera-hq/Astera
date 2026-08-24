import { InvoiceClient } from './clients/invoice';
import { PoolClient } from './clients/pool';
import { SecondaryMarketClient } from './clients/secondary_market';
import { AuctionClient } from './clients/auction';
import { CreditScoreClient } from './clients/credit_score';
import { OracleRegistryClient } from './clients/oracle_registry';
import { ComplianceClient } from './clients/compliance';
import { TrancheClient, type TrancheInvestorPosition } from './clients/tranche';
import { AccessControlClient } from './clients/access_control';
import { InsuranceClient } from './clients/insurance';
import { ShareClient } from './clients/share';
import type {
  TranchePool,
  TrancheConfig,
  TrancheAccounting,
  InvoiceTrancheExposure,
  WaterfallSimulation,
  TrancheClass,
} from './generated/tranche';
import type {
  PremiumConfig,
  ReserveFund,
  CoverageRecord,
  ClaimHistoryItem,
  ReserveHealth,
} from './generated/insurance';
import type {
  AsteraConfig,
  Invoice,
  InvoiceMetadata,
  InvestorPosition,
  PoolConfig,
  PoolTokenTotals,
  FundedInvoice,
  TransactionProgress,
  WithdrawalRequest,
  WaitEstimate,
  LiquidityForecastPoint,
  CoFundingRound,
  OracleInfo,
  VerificationRound,
  RegistryConfig,
  RateModelConfig,
  RateSnapshot,
  ComplianceStatus,
  RiskTier,
  ComplianceRecord,
  ScreeningHistoryEntry,
  Role,
  GovernanceProposal,
  GovernanceProposalStatus,
  ProposalCategory,
  GovernanceConfig,
  MultiSigConfig,
  Proposal,
  ActionPayload,
  CollateralConfig,
  CollateralDeposit,
  CollateralRiskConfig,
  CollateralSale,
  Listing,
  ListingKind,
} from './types';

export class AsteraClient {
  private invoiceClient: InvoiceClient;
  private poolClient: PoolClient;
  private secondaryMarketClient: SecondaryMarketClient;
  private auctionClient: AuctionClient;
  private creditScoreClient: CreditScoreClient;
  private oracleRegistryClient: OracleRegistryClient;
  private complianceClient: ComplianceClient;
  private trancheClient: TrancheClient;
  private accessControlClient: AccessControlClient;
  private insuranceClient: InsuranceClient;
  private shareClient: ShareClient | null;

  constructor(config: AsteraConfig) {
    this.invoiceClient = new InvoiceClient({
      rpcUrl: config.rpcUrl,
      network: config.network,
      contractId: config.invoiceContractId,
    });
    this.poolClient = new PoolClient({
      rpcUrl: config.rpcUrl,
      network: config.network,
      contractId: config.poolContractId,
    });
    this.secondaryMarketClient = new SecondaryMarketClient({
      rpcUrl: config.rpcUrl,
      network: config.network,
      contractId: config.secondaryMarketContractId ?? '',
    });
    this.auctionClient = new AuctionClient({
      rpcUrl: config.rpcUrl,
      network: config.network,
      contractId: config.auctionContractId ?? '',
    });
    this.creditScoreClient = new CreditScoreClient({
      rpcUrl: config.rpcUrl,
      network: config.network,
      contractId: config.creditScoreContractId ?? '',
    });
    this.oracleRegistryClient = new OracleRegistryClient({
      rpcUrl: config.rpcUrl,
      network: config.network,
      contractId: config.oracleRegistryContractId ?? '',
    });
    this.complianceClient = new ComplianceClient({
      rpcUrl: config.rpcUrl,
      network: config.network,
      contractId: config.complianceContractId ?? '',
    });
    this.trancheClient = new TrancheClient({
      rpcUrl: config.rpcUrl,
      network: config.network,
      contractId: config.trancheContractId ?? '',
    });
    this.accessControlClient = new AccessControlClient({
      rpcUrl: config.rpcUrl,
      network: config.network,
      contractId: config.accessControlContractId ?? '',
    });
    this.insuranceClient = new InsuranceClient({
      rpcUrl: config.rpcUrl,
      network: config.network,
      contractId: config.insuranceContractId ?? '',
    });
    this.shareClient = config.shareContractId
      ? new ShareClient({
          rpcUrl: config.rpcUrl,
          network: config.network,
          contractId: config.shareContractId,
        })
      : null;
  }

  public readonly invoice = {
    get: (id: bigint | number): Promise<Invoice> =>
      this.invoiceClient.getInvoice(id),

    getMetadata: (id: bigint | number): Promise<InvoiceMetadata> =>
      this.invoiceClient.getMetadata(id),

    create: (params: {
      signer: (txXdr: string) => Promise<string>;
      owner: string;
      debtor: string;
      amount: bigint;
      dueDate: number;
      description: string;
      verificationHash?: string;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> => {
      const { signer, owner, debtor, amount, dueDate, description, verificationHash, onProgress } = params;
      return this.invoiceClient.createInvoice({
        signer, owner, debtor, amount, dueDate, description, verificationHash, onProgress,
      });
    },

    verify: (params: {
      signer: (txXdr: string) => Promise<string>;
      oracle: string;
      id: bigint | number;
      approved: boolean;
      reason: string;
      oracleHash: string;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> => {
      const { signer, oracle, id, approved, reason, oracleHash, onProgress } = params;
      return this.invoiceClient.verifyInvoice({ signer, oracle, id, approved, reason, oracleHash, onProgress });
    },
  };

  public readonly pool = {
    getConfig: (): Promise<PoolConfig> =>
      this.poolClient.getConfig(),

    getPosition: (investor: string, token: string): Promise<InvestorPosition | null> =>
      this.poolClient.getPosition(investor, token),

    deposit: (params: {
      signer: (txXdr: string) => Promise<string>;
      investor: string;
      token: string;
      amount: bigint;
      minRate?: number;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.poolClient.deposit(params),

    requestWithdrawal: (params: {
      signer: (txXdr: string) => Promise<string>;
      investor: string;
      token: string;
      shares: bigint;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.poolClient.requestWithdrawal(params),

    cancelWithdrawalRequest: (params: {
      signer: (txXdr: string) => Promise<string>;
      investor: string;
      token: string;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.poolClient.cancelWithdrawalRequest(params),

    drainWithdrawalQueue: (params: {
      signer: (txXdr: string) => Promise<string>;
      caller: string;
      token: string;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.poolClient.drainWithdrawalQueue(params),

    getWithdrawalQueue: (token: string): Promise<WithdrawalRequest[]> =>
      this.poolClient.getWithdrawalQueue(token),

    repay: (params: {
      signer: (txXdr: string) => Promise<string>;
      payer: string;
      invoiceId: bigint | number;
      amount: bigint;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.poolClient.repay(params),

    fundInvoice: (params: {
      signer: (txXdr: string) => Promise<string>;
      admin: string;
      invoiceId: bigint | number;
      principal: bigint;
      sme: string;
      dueDate: number;
      token: string;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.poolClient.fundInvoice(params),

    getFundedInvoice: (invoiceId: bigint | number): Promise<FundedInvoice | null> =>
      this.poolClient.getFundedInvoice(invoiceId),

    getPoolTokenTotals: (token: string): Promise<PoolTokenTotals> =>
      this.poolClient.getPoolTokenTotals(token),

    openCoFunding: (params: {
      signer: (txXdr: string) => Promise<string>;
      admin: string;
      invoiceId: bigint | number;
      token: string;
      targetPrincipal: bigint;
      sme: string;
      dueDate: number;
      fundingDeadline: number;
      minCommitment: bigint;
      maxInvestorBps: number;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.poolClient.openCoFunding(params),

    commitToInvoice: (params: {
      signer: (txXdr: string) => Promise<string>;
      investor: string;
      invoiceId: bigint | number;
      amount: bigint;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.poolClient.commitToInvoice(params),

    finalizeCoFunding: (params: {
      signer: (txXdr: string) => Promise<string>;
      caller: string;
      invoiceId: bigint | number;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.poolClient.finalizeCoFunding(params),

    withdrawCoFundingCommitment: (params: {
      signer: (txXdr: string) => Promise<string>;
      investor: string;
      invoiceId: bigint | number;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.poolClient.withdrawCommitment(params),

    transferCoFundShare: (params: {
      signer: (txXdr: string) => Promise<string>;
      from: string;
      invoiceId: bigint | number;
      token: string;
      to: string;
      bps: number;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.poolClient.transferCoFundShare(params),

    getCoFundingRound: (invoiceId: bigint | number): Promise<CoFundingRound | null> =>
      this.poolClient.getCoFundingRound(invoiceId),

    listCoFundingRounds: (): Promise<bigint[]> =>
      this.poolClient.listCoFundingRounds(),

    getInvestorCoFundPositions: (
      investor: string,
    ): Promise<Array<{ invoiceId: bigint; bps: number }>> =>
      this.poolClient.getInvestorCoFundPositions(investor),

    getCoFundShare: (invoiceId: bigint | number, investor: string): Promise<number> =>
      this.poolClient.getCoFundShare(invoiceId, investor),

    getCurrentRate: (token: string): Promise<number> =>
      this.poolClient.getCurrentRate(token),

    getRateModelConfig: (token: string): Promise<RateModelConfig | null> =>
      this.poolClient.getRateModelConfig(token),

    getRateHistory: (token: string, limit: number): Promise<RateSnapshot[]> =>
      this.poolClient.getRateHistory(token, limit),

    previewRateAtUtilization: (token: string, utilizationBps: number): Promise<number> =>
      this.poolClient.previewRateAtUtilization(token, utilizationBps),

    // #1036: multi-asset, oracle-priced collateral risk response

    depositCollateral: (params: {
      signer: (txXdr: string) => Promise<string>;
      invoiceId: bigint | number;
      depositor: string;
      token: string;
      amount: bigint;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.poolClient.depositCollateral(params),

    topUpCollateral: (params: {
      signer: (txXdr: string) => Promise<string>;
      invoiceId: bigint | number;
      depositor: string;
      token: string;
      amount: bigint;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.poolClient.topUpCollateral(params),

    getCollateralConfig: (): Promise<CollateralConfig> =>
      this.poolClient.getCollateralConfig(),

    getCollateralDeposit: (invoiceId: bigint | number): Promise<CollateralDeposit | null> =>
      this.poolClient.getCollateralDeposit(invoiceId),

    // get_live_collateral_ratio/check_collateral_risk/liquidateCollateral and
    // the risk-config get/set now live on the auction satellite (see
    // `this.auction` below) — pool keeps only the trusted mutation
    // entrypoint that satellite calls.
    setRiskContract: (params: {
      signer: (txXdr: string) => Promise<string>;
      admin: string;
      riskContract: string;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.poolClient.setRiskContract(params),
  };

  // #1036: collateral-liquidation Dutch auction + oracle-priced,
  // multi-asset collateral risk-response satellite (contracts/auction).
  public readonly auction = {
    getPoolContract: (): Promise<string | null> =>
      this.auctionClient.getPoolContract(),

    setCollateralRiskConfig: (params: {
      signer: (txXdr: string) => Promise<string>;
      admin: string;
      dangerBps: number;
      gracePeriodSecs: number;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.auctionClient.setCollateralRiskConfig(params),

    getCollateralRiskConfig: (): Promise<CollateralRiskConfig> =>
      this.auctionClient.getCollateralRiskConfig(),

    getLiveCollateralRatio: (invoiceId: bigint | number): Promise<number> =>
      this.auctionClient.getLiveCollateralRatio(invoiceId),

    getAtRiskSince: (invoiceId: bigint | number): Promise<number | null> =>
      this.auctionClient.getAtRiskSince(invoiceId),

    checkCollateralRisk: (params: {
      signer: (txXdr: string) => Promise<string>;
      caller: string;
      invoiceId: bigint | number;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.auctionClient.checkCollateralRisk(params),

    liquidateCollateral: (params: {
      signer: (txXdr: string) => Promise<string>;
      caller: string;
      invoiceId: bigint | number;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.auctionClient.liquidateCollateral(params),

    getSale: (saleId: bigint | number): Promise<CollateralSale | null> =>
      this.auctionClient.getSale(saleId),

    currentSalePrice: (saleId: bigint | number): Promise<bigint> =>
      this.auctionClient.currentSalePrice(saleId),

    takeCollateralSale: (params: {
      signer: (txXdr: string) => Promise<string>;
      taker: string;
      saleId: bigint | number;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.auctionClient.takeCollateralSale(params),

    reclaimExpiredSale: (params: {
      signer: (txXdr: string) => Promise<string>;
      caller: string;
      saleId: bigint | number;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.auctionClient.reclaimExpiredSale(params),
  };

  /** #1044: secondary-market listings and withdrawal-wait/liquidity-forecast analytics. */
  public readonly secondaryMarket = {
    listPosition: (params: {
      signer: (txXdr: string) => Promise<string>;
      seller: string;
      invoiceId: bigint | number;
      kind: ListingKind;
      amountOrBps: bigint;
      price: bigint;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.secondaryMarketClient.listPosition(params),

    cancelListing: (params: {
      signer: (txXdr: string) => Promise<string>;
      seller: string;
      listingId: bigint | number;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.secondaryMarketClient.cancelListing(params),

    buyListing: (params: {
      signer: (txXdr: string) => Promise<string>;
      buyer: string;
      listingId: bigint | number;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.secondaryMarketClient.buyListing(params),

    getListing: (listingId: bigint | number): Promise<Listing | null> =>
      this.secondaryMarketClient.getListing(listingId),

    listListingsForInvoice: (invoiceId: bigint | number): Promise<bigint[]> =>
      this.secondaryMarketClient.listListingsForInvoice(invoiceId),

    listListingsForInvestor: (seller: string): Promise<bigint[]> =>
      this.secondaryMarketClient.listListingsForInvestor(seller),

    estimateWithdrawalWait: (investor: string, token: string): Promise<WaitEstimate> =>
      this.secondaryMarketClient.estimateWithdrawalWait(investor, token),

    getLiquidityForecast: (token: string, horizonDays: number): Promise<LiquidityForecastPoint[]> =>
      this.secondaryMarketClient.getLiquidityForecast(token, horizonDays),
  };

  public readonly creditScore = {
    getCreditScore: (sme: string) =>
      this.creditScoreClient.getCreditScore(sme),
  };

  public readonly oracleRegistry = {
    getRound: (invoiceId: bigint | number): Promise<VerificationRound | null> =>
      this.oracleRegistryClient.getRound(invoiceId),

    getOracleInfo: (operator: string): Promise<OracleInfo | null> =>
      this.oracleRegistryClient.getOracleInfo(operator),

    getRegistryConfig: (): Promise<RegistryConfig> =>
      this.oracleRegistryClient.getRegistryConfig(),

    openRound: (params: {
      signer: (txXdr: string) => Promise<string>;
      caller: string;
      invoiceId: bigint | number;
      oracleHash: string;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.oracleRegistryClient.openRound(params),

    register: (params: {
      signer: (txXdr: string) => Promise<string>;
      operator: string;
      stakeAmount: bigint;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.oracleRegistryClient.register(params),

    vote: (params: {
      signer: (txXdr: string) => Promise<string>;
      oracle: string;
      invoiceId: bigint | number;
      approved: boolean;
      evidenceHash: string;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.oracleRegistryClient.vote(params),
  };

  public readonly compliance = {
    isCleared: (address: string): Promise<boolean> =>
      this.complianceClient.isCleared(address),

    getRecord: (address: string): Promise<ComplianceRecord | null> =>
      this.complianceClient.getRecord(address),

    getHistory: (address: string): Promise<ScreeningHistoryEntry[]> =>
      this.complianceClient.getHistory(address),

    listFlagged: (): Promise<string[]> =>
      this.complianceClient.listFlagged(),

    listPendingReview: (): Promise<string[]> =>
      this.complianceClient.listPendingReview(),

    submitScreeningResult: (params: {
      signer: (txXdr: string) => Promise<string>;
      screener: string;
      address: string;
      status: ComplianceStatus;
      reasonCode: number;
      riskTier: RiskTier;
      expiresAt: number;
      notesHash: string;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.complianceClient.submitScreeningResult(params),

    requestReview: (params: {
      signer: (txXdr: string) => Promise<string>;
      caller: string;
      address: string;
      reason: string;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.complianceClient.requestReview(params),
  };

  public readonly tranche = {
    getPool: (token: string): Promise<TranchePool> =>
      this.trancheClient.getPool(token),

    getConfig: (token: string): Promise<TrancheConfig> =>
      this.trancheClient.getTrancheConfig(token),

    getTotals: (token: string, trancheClass: TrancheClass): Promise<TrancheAccounting> =>
      this.trancheClient.getTotals(token, trancheClass),

    getPosition: (
      investor: string,
      token: string,
      trancheClass: TrancheClass,
    ): Promise<TrancheInvestorPosition> =>
      this.trancheClient.getInvestorPosition(investor, token, trancheClass),

    getEffectiveApy: (token: string, trancheClass: TrancheClass): Promise<number> =>
      this.trancheClient.getEffectiveApy(token, trancheClass),

    getInvoiceExposure: (invoiceId: number): Promise<InvoiceTrancheExposure | null> =>
      this.trancheClient.getInvoiceExposure(invoiceId),

    simulateWaterfall: (
      token: string,
      invoiceId: number,
      hypotheticalRepayment: bigint,
      elapsedSecs: number,
    ): Promise<WaterfallSimulation> =>
      this.trancheClient.simulateWaterfall(token, invoiceId, hypotheticalRepayment, elapsedSecs),

    deposit: (params: {
      signer: (txXdr: string) => Promise<string>;
      investor: string;
      token: string;
      trancheClass: TrancheClass;
      amount: bigint;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.trancheClient.deposit(params),

    withdraw: (params: {
      signer: (txXdr: string) => Promise<string>;
      investor: string;
      token: string;
      trancheClass: TrancheClass;
      amount: bigint;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.trancheClient.withdraw(params),

    setConfig: (params: {
      signer: (txXdr: string) => Promise<string>;
      admin: string;
      token: string;
      seniorTargetYieldBps: number;
      seniorAdvanceRateBps: number;
      juniorFirstLossBps: number;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.trancheClient.setTrancheConfig(params),
  };

  /** #864: role-based multisig access control. */
  public readonly accessControl = {
    getRoleConfig: (role: Role): Promise<MultiSigConfig | null> =>
      this.accessControlClient.getRoleConfig(role),

    isSigner: (role: Role, address: string): Promise<boolean> =>
      this.accessControlClient.isSigner(role, address),

    getProposal: (proposalId: bigint | number): Promise<Proposal | null> =>
      this.accessControlClient.getProposal(proposalId),

    listProposals: (): Promise<Array<{ id: bigint; proposal: Proposal }>> =>
      this.accessControlClient.listProposals(),

    proposeAction: (params: {
      signer: (txXdr: string) => Promise<string>;
      role: Role;
      proposer: string;
      target: string;
      action: ActionPayload;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.accessControlClient.proposeAction(params),

    approveAction: (params: {
      signer: (txXdr: string) => Promise<string>;
      signerAddress: string;
      proposalId: bigint | number;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.accessControlClient.approveAction(params),

    revokeApproval: (params: {
      signer: (txXdr: string) => Promise<string>;
      signerAddress: string;
      proposalId: bigint | number;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.accessControlClient.revokeApproval(params),

    rejectAction: (params: {
      signer: (txXdr: string) => Promise<string>;
      signerAddress: string;
      proposalId: bigint | number;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.accessControlClient.rejectAction(params),

    executeAction: (params: {
      signer: (txXdr: string) => Promise<string>;
      caller: string;
      proposalId: bigint | number;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.accessControlClient.executeAction(params),
  };

  /** #1055: default-insurance reserve — coverage purchase, claims, reserve health. */
  public readonly insurance = {
    estimatePremium: (
      principal: bigint,
      sme: string,
      tenorDays: number,
      token: string,
    ): Promise<bigint> =>
      this.insuranceClient.estimatePremium(principal, sme, tenorDays, token),

    purchaseCoverage: (params: {
      signer: (txXdr: string) => Promise<string>;
      payer: string;
      invoiceId: bigint | number;
      principal: bigint;
      sme: string;
      dueDate: number;
      token: string;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.insuranceClient.purchaseCoverage(params),

    fileClaim: (params: {
      signer: (txXdr: string) => Promise<string>;
      caller: string;
      invoiceId: bigint | number;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.insuranceClient.fileClaim(params),

    getReserveStatus: (token: string): Promise<ReserveFund> =>
      this.insuranceClient.getReserveStatus(token),

    getCoverageRecord: (invoiceId: bigint | number): Promise<CoverageRecord | null> =>
      this.insuranceClient.getCoverageRecord(invoiceId),

    getClaimHistory: (invoiceId: bigint | number): Promise<ClaimHistoryItem[]> =>
      this.insuranceClient.getClaimHistory(invoiceId),

    checkReserveHealth: (token: string): Promise<ReserveHealth> =>
      this.insuranceClient.checkReserveHealth(token),

    getPremiumConfig: (): Promise<PremiumConfig | null> =>
      this.insuranceClient.getPremiumConfig(),

    getMinReserveAmount: (token: string): Promise<bigint> =>
      this.insuranceClient.getMinReserveAmount(token),

    setPremiumConfig: (params: {
      signer: (txXdr: string) => Promise<string>;
      admin: string;
      config: PremiumConfig;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.insuranceClient.setPremiumConfig(params),

    setMinCoverageRatio: (params: {
      signer: (txXdr: string) => Promise<string>;
      admin: string;
      token: string;
      minRatioBps: number;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.insuranceClient.setMinCoverageRatio(params),

    setMinReserveAmount: (params: {
      signer: (txXdr: string) => Promise<string>;
      admin: string;
      token: string;
      minAmount: bigint;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.insuranceClient.setMinReserveAmount(params),

    fundReserveFromTreasury: (params: {
      signer: (txXdr: string) => Promise<string>;
      admin: string;
      token: string;
      amount: bigint;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.insuranceClient.fundReserveFromTreasury(params),

    setCreditScoreContract: (params: {
      signer: (txXdr: string) => Promise<string>;
      admin: string;
      creditScoreContract: string;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.insuranceClient.setCreditScoreContract(params),
  };

  public readonly share = {
    balance: (id: string): Promise<bigint> =>
      this.shareClient!.balance(id),

    balanceAt: (id: string, timestamp: number): Promise<bigint> =>
      this.shareClient!.balanceAt(id, timestamp),

    totalSupply: (): Promise<bigint> =>
      this.shareClient!.totalSupply(),

    allowance: (owner: string, spender: string): Promise<bigint> =>
      this.shareClient!.allowance(owner, spender),

    decimals: (): Promise<number> =>
      this.shareClient!.decimals(),

    name: (): Promise<string> =>
      this.shareClient!.name(),

    symbol: (): Promise<string> =>
      this.shareClient!.symbol(),

    mint: (params: {
      signer: (txXdr: string) => Promise<string>;
      admin: string;
      to: string;
      amount: bigint;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.shareClient!.mint(params),

    burn: (params: {
      signer: (txXdr: string) => Promise<string>;
      admin: string;
      from: string;
      amount: bigint;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.shareClient!.burn(params),

    transfer: (params: {
      signer: (txXdr: string) => Promise<string>;
      from: string;
      to: string;
      amount: bigint;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.shareClient!.transfer(params),

    approve: (params: {
      signer: (txXdr: string) => Promise<string>;
      owner: string;
      spender: string;
      amount: bigint;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.shareClient!.approve(params),

    transferFrom: (params: {
      signer: (txXdr: string) => Promise<string>;
      spender: string;
      from: string;
      to: string;
      amount: bigint;
      onProgress?: (progress: TransactionProgress) => void;
    }): Promise<string> =>
      this.shareClient!.transferFrom(params),
  };
}
