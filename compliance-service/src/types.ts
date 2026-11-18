export interface ComplianceConfig {
  rpcUrl: string;
  horizonUrl: string;
  networkPassphrase: string;
  screenerSecretKey: string;
  complianceContractId: string;
  poolContractId: string;
  invoiceContractId: string;
  healthPort: number;
  /** Admin token required for POST /screen and GET /flags. Empty = open (dev only). */
  adminToken: string;
  /** Deposit amount (raw) at/above which structuring heuristics fire. */
  structuringThreshold: bigint;
  /** Window (ms) for counting near-threshold deposits. */
  structuringWindowMs: number;
  /** Max near-threshold deposits in the window before request_review. */
  structuringMaxCount: number;
  /** Base URL of a real upstream KYC/screening provider; unset = use bundled MockScreener. */
  screeningProviderUrl: string;
  /** API key for the upstream screening provider, if required. */
  screeningProviderApiKey: string;
  /** Hard timeout (ms) on upstream screening provider calls. */
  screeningProviderTimeoutMs: number;
  /** How often (ms) the monitor checks tracked subjects against get_rescreening_interval. */
  rescreenCheckIntervalMs: number;
}

export type RiskTier = 'Low' | 'Medium' | 'High';

/** Must mirror `ComplianceStatus` in contracts/compliance/src/lib.rs exactly. */
export type ScreenDecision = 'Unscreened' | 'Cleared' | 'Flagged' | 'Blocked' | 'PendingReview';

export interface ScreenResult {
  address: string;
  status: ScreenDecision;
  reasonCode: number;
  riskTier: RiskTier;
  matchedList?: string;
  notes: string;
}

export interface MonitorAlert {
  id: string;
  address: string;
  reason: string;
  at: string;
  pattern: string;
}
