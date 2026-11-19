// Target contract registry for the roles-admin UI. This module owns the only
// piece of the proposal-builder configuration that needs live contract IDs
// (from `@/lib/stellar`), keeping the pure encoder in `proposalPayload.ts`
// free of that dependency so it stays trivially unit-testable.

import {
  POOL_CONTRACT_ID,
  INVOICE_CONTRACT_ID,
  CREDIT_SCORE_CONTRACT_ID,
  ORACLE_REGISTRY_CONTRACT_ID,
  COMPLIANCE_CONTRACT_ID,
  GOVERNANCE_CONTRACT_ID,
  REFERRAL_CONTRACT_ID,
  ACCESS_CONTROL_CONTRACT_ID,
} from '@/lib/stellar';
import type { TargetKey } from './proposalPayload';

export type { TargetKey } from './proposalPayload';
export { ACTIONS_BY_TARGET, ACTION_LABELS } from './proposalPayload';

export const TARGET_CONTRACTS: Record<TargetKey, { label: string; id: string }> = {
  pool: { label: 'Pool', id: POOL_CONTRACT_ID },
  invoice: { label: 'Invoice', id: INVOICE_CONTRACT_ID },
  credit_score: { label: 'Credit Score', id: CREDIT_SCORE_CONTRACT_ID },
  oracle_registry: { label: 'Oracle Registry', id: ORACLE_REGISTRY_CONTRACT_ID },
  compliance: { label: 'Compliance', id: COMPLIANCE_CONTRACT_ID },
  governance: { label: 'Governance', id: GOVERNANCE_CONTRACT_ID },
  referral: { label: 'Referral', id: REFERRAL_CONTRACT_ID },
  self: { label: 'Access Control (self-management)', id: ACCESS_CONTROL_CONTRACT_ID },
};
