/** Alert Thresholds and Rules */

/**
 * Large Transaction Threshold (in USDC/EURC unit, e.g., $5,000).
 * On-chain values are in stroops (7 decimals), so this will be multiplied by 10^7.
 */
export const LARGE_TX_THRESHOLD = 5000;

/**
 * Unusual Activity - Frequency Threshold
 * Trigger if more than X events of the same type occur within the window.
 */
export const ACTIVITY_THRESHOLD_COUNT = 3;

/**
 * Unusual Activity - Time Window (in seconds)
 * 10 minutes = 600 seconds.
 */
export const ACTIVITY_WINDOW_SECONDS = 600;

/** Priority Levels for Alerts */
export type AlertPriority = 'LOW' | 'MEDIUM' | 'HIGH' | 'CRITICAL';

/**
 * Liquidity threshold: alert when available liquidity drops below this fraction
 * of total pool deposits (0.10 = 10%).
 */
export const LOW_LIQUIDITY_THRESHOLD_RATIO = 0.1;

/**
 * Default rate threshold: alert when more than this fraction of invoices
 * in a rolling 7-day window are defaulted (0.05 = 5%).
 */
export const DEFAULT_RATE_THRESHOLD_RATIO = 0.05;

/** Rolling window for default-rate calculation (seconds). */
export const DEFAULT_RATE_WINDOW_SECONDS = 7 * 24 * 60 * 60;

/** Alert Types */
export type AlertType =
  | 'LARGE_TRANSACTION'
  | 'UNUSUAL_ACTIVITY'
  | 'CONTRACT_DEFAULT'
  | 'SYSTEM_ERROR'
  | 'INVOICE_FUNDED'
  | 'INVOICE_PAID'
  | 'INVOICE_DEFAULTED'
  | 'DISPUTE_RAISED'
  | 'DISPUTE_RESOLVED'
  | 'YIELD_RATE_CHANGED'
  | 'COLLATERAL_REQUIRED'
  | 'LOW_LIQUIDITY'
  | 'HIGH_DEFAULT_RATE'
  | 'CONTRACT_PAUSED'
  | 'RPC_SLOW';

/** Rule Definition */
export interface AlertRule {
  id: string;
  name: string;
  type: AlertType;
  priority: AlertPriority;
  description: string;
}

export const ALERT_RULES: AlertRule[] = [
  {
    id: 'rule-large-tx',
    name: 'Large Transaction Alert',
    type: 'LARGE_TRANSACTION',
    priority: 'HIGH',
    description: `Triggered when a transaction exceeds ${LARGE_TX_THRESHOLD} units.`,
  },
  {
    id: 'rule-unusual-activity',
    name: 'Unusual Activity Detection',
    type: 'UNUSUAL_ACTIVITY',
    priority: 'MEDIUM',
    description: 'Triggered when multiple events occur from the same address in a short window.',
  },
  {
    id: 'rule-contract-default',
    name: 'Invoice Default Alert',
    type: 'CONTRACT_DEFAULT',
    priority: 'CRITICAL',
    description: 'Triggered when an invoice is marked as defaulted.',
  },
  {
    id: 'rule-invoice-funded',
    name: 'Invoice Funded',
    type: 'INVOICE_FUNDED',
    priority: 'MEDIUM',
    description: 'Triggered when an invoice transitions to the Funded status.',
  },
  {
    id: 'rule-invoice-paid',
    name: 'Invoice Paid',
    type: 'INVOICE_PAID',
    priority: 'MEDIUM',
    description: 'Triggered when an invoice is fully repaid.',
  },
  {
    id: 'rule-invoice-defaulted',
    name: 'Invoice Defaulted',
    type: 'INVOICE_DEFAULTED',
    priority: 'CRITICAL',
    description: 'Triggered when an invoice is marked as defaulted via the SME/investor flow.',
  },
  {
    id: 'rule-low-liquidity',
    name: 'Low Pool Liquidity',
    type: 'LOW_LIQUIDITY',
    priority: 'HIGH',
    description: `Triggered when available pool liquidity drops below ${LOW_LIQUIDITY_THRESHOLD_RATIO * 100}% of total deposits.`,
  },
  {
    id: 'rule-high-default-rate',
    name: 'High Invoice Default Rate',
    type: 'HIGH_DEFAULT_RATE',
    priority: 'CRITICAL',
    description: `Triggered when more than ${DEFAULT_RATE_THRESHOLD_RATIO * 100}% of invoices in a 7-day rolling window are defaulted.`,
  },
  {
    id: 'rule-contract-paused',
    name: 'Contract Paused',
    type: 'CONTRACT_PAUSED',
    priority: 'CRITICAL',
    description: 'Triggered when any core contract is paused unexpectedly.',
  },
  {
    id: 'rule-rpc-slow',
    name: 'Stellar RPC Slow',
    type: 'RPC_SLOW',
    priority: 'HIGH',
    description: 'Triggered when the Stellar RPC endpoint response time exceeds 5 seconds.',
  },
];
