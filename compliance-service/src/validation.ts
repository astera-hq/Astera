/**
 * Request-body validation for the admin REST API.
 *
 * `POST /screen` lets an operator override the screener's decision with an
 * explicit `status` / `riskTier`. Those values are encoded as Soroban
 * symbols and submitted to `submit_screening_result`, so anything outside
 * the contract's enums must be rejected here, at the API boundary, instead
 * of being pushed on-chain.
 */

import type { RiskTier, ScreenDecision } from './types';

/** Must mirror `ComplianceStatus` in contracts/compliance/src/lib.rs exactly. */
export const SCREEN_DECISIONS: readonly ScreenDecision[] = [
  'Unscreened',
  'Cleared',
  'Flagged',
  'Blocked',
  'PendingReview',
];

/** Must mirror `RiskTier` in contracts/compliance/src/lib.rs exactly. */
export const RISK_TIERS: readonly RiskTier[] = ['Low', 'Medium', 'High'];

const U32_MAX = 0xffffffff;

export function isScreenDecision(value: unknown): value is ScreenDecision {
  return typeof value === 'string' && (SCREEN_DECISIONS as readonly string[]).includes(value);
}

export function isRiskTier(value: unknown): value is RiskTier {
  return typeof value === 'string' && (RISK_TIERS as readonly string[]).includes(value);
}

export interface ScreenRequestBody {
  address?: string;
  name?: string;
  jurisdiction?: string;
  submitOnChain?: boolean;
  status?: ScreenDecision;
  reasonCode?: number;
  riskTier?: RiskTier;
  notesHash?: string;
}

/**
 * Validates the operator-supplied override fields of a `POST /screen` body.
 * Returns an error message for the first invalid field, or `null` when every
 * supplied field is acceptable. Absent fields are always accepted — they fall
 * back to the screener's own result.
 */
export function validateScreenOverride(body: Record<string, unknown>): string | null {
  if (body.status !== undefined && !isScreenDecision(body.status)) {
    return `invalid status: must be one of ${SCREEN_DECISIONS.join(', ')}`;
  }
  if (body.riskTier !== undefined && !isRiskTier(body.riskTier)) {
    return `invalid riskTier: must be one of ${RISK_TIERS.join(', ')}`;
  }
  if (
    body.reasonCode !== undefined &&
    !(
      typeof body.reasonCode === 'number' &&
      Number.isInteger(body.reasonCode) &&
      body.reasonCode >= 0 &&
      body.reasonCode <= U32_MAX
    )
  ) {
    return 'invalid reasonCode: must be an integer between 0 and 4294967295';
  }
  if (body.notesHash !== undefined && typeof body.notesHash !== 'string') {
    return 'invalid notesHash: must be a string';
  }
  return null;
}
