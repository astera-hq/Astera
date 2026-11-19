// Pure helpers for the roles-admin proposal builder. This module deliberately
// depends only on `@/lib/types` (types + the Stellar-address validator) and has
// no React / `@/lib/stellar` / `@/lib/contracts` imports, so the payload encoder
// can be unit-tested directly without rendering or mocking anything.

import type { ActionPayload, Role } from '@/lib/types';
import { parseStellarAddress, ROLE_LABELS } from '@/lib/types';

export type TargetKey =
  | 'pool'
  | 'invoice'
  | 'credit_score'
  | 'oracle_registry'
  | 'compliance'
  | 'governance'
  | 'referral'
  | 'self';

// Which ActionPayload variants make sense for each target — mirrors the
// `execute_cross_contract`/`execute_self_management` match arms in
// contracts/access_control/src/lib.rs. Every target except `pool` also
// accepts its own `Set*AccessControl` rotation action (#1042) — proposed
// under `Role::SuperAdmin`, same as the self-management actions. `pool`
// has no wasm size budget left for a new entrypoint (see
// contracts/.wasm-size-baseline.json), so its rotation action was
// dropped from this PR.
export const ACTIONS_BY_TARGET: Record<TargetKey, ActionPayload['tag'][]> = {
  pool: [
    'SetPaused',
    'SetYield',
    'SetTreasury',
    'WithdrawRevenue',
    'SetOracleContract',
    'SetKycRequired',
    'SetInvestorKyc',
    'SetMaxUtilization',
  ],
  invoice: [
    'SetPaused',
    'SetOracle',
    'RegisterDebtor',
    'DeactivateDebtor',
    'AddKeeper',
    'SetInvoiceAccessControl',
  ],
  credit_score: [
    'SetPaused',
    'SetLateThreshold',
    'SetScoreThresholds',
    'RegisterAttestor',
    'SetCreditScoreAccessControl',
  ],
  oracle_registry: [
    'SetOracleRegistryPaused',
    'SetOracleRegistryInvoiceContract',
    'SetOracleRegistryTreasury',
    'SetOracleRegistryConfig',
    'SlashOracle',
    'AdminResolveRound',
    'SetOracleRegistryAccessControl',
  ],
  compliance: [
    'SetCompliancePaused',
    'RegisterScreener',
    'ConfirmScreenerRegistration',
    'DeregisterScreener',
    'SetRescreeningInterval',
    'SetScreenerTimelock',
    'SetComplianceAccessControl',
  ],
  governance: ['UpdateGovernanceConfig', 'SetCategoryQuorum', 'SetGovernanceAccessControl'],
  referral: [
    'SetReferralPaused',
    'SetReferralPool',
    'SetBorrowRewardBps',
    'SetDepositRewardBps',
    'SetReferralAccessControl',
  ],
  self: ['AddSigner', 'RemoveSigner', 'SetThreshold'],
};

export const ACTION_LABELS: Record<ActionPayload['tag'], string> = {
  SetPaused: 'Set Paused',
  SetYield: 'Set Yield (bps)',
  SetTreasury: 'Set Treasury Address',
  WithdrawRevenue: 'Withdraw Revenue',
  SetOracleContract: 'Set Oracle Contract',
  SetKycRequired: 'Set KYC Required',
  SetInvestorKyc: 'Set Investor KYC Status',
  SetMaxUtilization: 'Set Max Utilization (bps)',
  SetOracle: 'Set Oracle Address',
  RegisterDebtor: 'Register Debtor',
  DeactivateDebtor: 'Deactivate Debtor',
  AddKeeper: 'Add Keeper',
  SetLateThreshold: 'Set Late Threshold (days)',
  SetScoreThresholds: 'Set Score Thresholds',
  RegisterAttestor: 'Register Attestor',
  SetOracleRegistryInvoiceContract: 'Set Invoice Contract',
  SetOracleRegistryTreasury: 'Set Treasury Address',
  SetOracleRegistryConfig: 'Set Registry Config',
  SetOracleRegistryPaused: 'Set Paused',
  SlashOracle: 'Slash Oracle',
  AdminResolveRound: 'Admin Resolve Round',
  SetCompliancePaused: 'Set Paused',
  RegisterScreener: 'Register Screener',
  ConfirmScreenerRegistration: 'Confirm Screener Registration',
  DeregisterScreener: 'Deregister Screener',
  SetRescreeningInterval: 'Set Rescreening Interval (secs)',
  SetScreenerTimelock: 'Set Screener Timelock (secs)',
  UpdateGovernanceConfig: 'Update Config (quorum/pass bps)',
  SetCategoryQuorum: 'Set Category Quorum',
  SetReferralPaused: 'Set Paused',
  SetReferralPool: 'Set Pool Address',
  SetBorrowRewardBps: 'Set Borrow Reward (bps)',
  SetDepositRewardBps: 'Set Deposit Reward (bps)',
  SetInvoiceAccessControl: 'Rotate Access Control',
  SetCreditScoreAccessControl: 'Rotate Access Control',
  SetOracleRegistryAccessControl: 'Rotate Access Control',
  SetComplianceAccessControl: 'Rotate Access Control',
  SetGovernanceAccessControl: 'Rotate Access Control',
  SetReferralAccessControl: 'Rotate Access Control',
  AddSigner: 'Add Signer',
  RemoveSigner: 'Remove Signer',
  SetThreshold: 'Set Threshold',
};

/** Encode the selected action + its form fields into a typed ActionPayload. */
export function buildActionPayload(
  action: ActionPayload['tag'],
  fields: Record<string, string>,
): ActionPayload | null {
  const num = (key: string) => Number(fields[key] ?? '0');
  const str = (key: string) => (fields[key] ?? '').trim();
  switch (action) {
    case 'SetPaused':
    case 'SetKycRequired':
      return { tag: action, values: [fields[`${action}_value`] === 'true'] };
    case 'SetYield':
    case 'SetMaxUtilization':
      return { tag: action, values: [num('bps')] };
    case 'SetTreasury':
    case 'SetOracleContract':
    case 'SetOracle':
    case 'AddKeeper':
      return { tag: action, values: [parseStellarAddress(str('address'))] };
    case 'WithdrawRevenue':
      return {
        tag: 'WithdrawRevenue',
        values: [parseStellarAddress(str('token')), BigInt(str('amount') || '0')],
      };
    case 'SetInvestorKyc':
      return {
        tag: 'SetInvestorKyc',
        values: [parseStellarAddress(str('investor')), fields.approved === 'true'],
      };
    case 'RegisterDebtor':
      return {
        tag: 'RegisterDebtor',
        values: [str('debtorId'), str('debtorName'), BigInt(str('maxExposure') || '0')],
      };
    case 'DeactivateDebtor':
      return { tag: 'DeactivateDebtor', values: [str('debtorId')] };
    case 'SetLateThreshold':
      return { tag: 'SetLateThreshold', values: [BigInt(str('days') || '0')] };
    case 'SetScoreThresholds':
      return {
        tag: 'SetScoreThresholds',
        values: [num('excellent'), num('veryGood'), num('good'), num('fair')],
      };
    case 'RegisterAttestor':
      return {
        tag: 'RegisterAttestor',
        values: [parseStellarAddress(str('address')), num('attestorType'), num('weightBps')],
      };
    case 'AddSigner':
    case 'RemoveSigner':
      return {
        tag: action,
        values: [str('role') as Role, parseStellarAddress(str('address'))],
      };
    case 'SetThreshold':
      return { tag: 'SetThreshold', values: [str('role') as Role, num('threshold')] };
    case 'SetOracleRegistryInvoiceContract':
    case 'SetReferralPool':
      return { tag: action, values: [parseStellarAddress(str('address'))] };
    case 'SetOracleRegistryTreasury':
      return {
        tag: 'SetOracleRegistryTreasury',
        values: [str('treasury') ? parseStellarAddress(str('treasury')) : undefined],
      };
    case 'SetOracleRegistryConfig':
      return {
        tag: 'SetOracleRegistryConfig',
        values: [
          BigInt(str('minStake') || '0'),
          num('requiredVotes'),
          num('quorumBps'),
          BigInt(str('roundDurationSecs') || '0'),
          BigInt(str('deregisterCooldownSecs') || '0'),
        ],
      };
    case 'SetOracleRegistryPaused':
    case 'SetCompliancePaused':
    case 'SetReferralPaused':
      return { tag: action, values: [fields[`${action}_value`] === 'true'] };
    case 'SlashOracle':
      return {
        tag: 'SlashOracle',
        values: [
          parseStellarAddress(str('operator')),
          num('bps'),
          BigInt(str('roundId') || '0'),
          str('evidence'),
        ],
      };
    case 'AdminResolveRound':
      return {
        tag: 'AdminResolveRound',
        values: [BigInt(str('invoiceId') || '0'), fields.approved === 'true', str('reason')],
      };
    case 'RegisterScreener':
    case 'ConfirmScreenerRegistration':
    case 'DeregisterScreener':
      return { tag: action, values: [parseStellarAddress(str('screener'))] };
    case 'SetRescreeningInterval':
    case 'SetScreenerTimelock':
      return { tag: action, values: [BigInt(str('secs') || '0')] };
    case 'UpdateGovernanceConfig':
      // ABI: UpdateGovernanceConfig(u32, u32, u64) — THREE values
      // (quorum_bps, pass_bps, execution_delay_secs). lib/types.ts correctly
      // types this as [number, number, number]; the SDK type is currently
      // wrong and must NOT be used to derive this encoder.
      return {
        tag: 'UpdateGovernanceConfig',
        values: [num('quorumBps'), num('passBps'), num('executionDelaySecs')],
      };
    case 'SetCategoryQuorum':
      return {
        tag: 'SetCategoryQuorum',
        values: [num('category'), num('quorumBps')],
      };
    case 'SetBorrowRewardBps':
    case 'SetDepositRewardBps':
      return { tag: action, values: [num('bps')] };
    case 'SetInvoiceAccessControl':
    case 'SetCreditScoreAccessControl':
    case 'SetOracleRegistryAccessControl':
    case 'SetComplianceAccessControl':
    case 'SetGovernanceAccessControl':
    case 'SetReferralAccessControl':
      return { tag: action, values: [parseStellarAddress(str('address'))] };
  }
  return null;
}

/** Human-readable one-line summary shown in the proposals queue. */
export function summarizeAction(action: ActionPayload): string {
  switch (action.tag) {
    case 'SetPaused':
      return `Set Paused → ${action.values[0]}`;
    case 'SetYield':
      return `Set Yield → ${action.values[0]} bps`;
    case 'SetTreasury':
      return `Set Treasury → ${action.values[0]}`;
    case 'WithdrawRevenue':
      return `Withdraw ${action.values[1]} from ${action.values[0]}`;
    case 'SetOracleContract':
      return `Set Oracle Contract → ${action.values[0]}`;
    case 'SetKycRequired':
      return `Set KYC Required → ${action.values[0]}`;
    case 'SetInvestorKyc':
      return `Set Investor KYC → ${action.values[0]}: ${action.values[1]}`;
    case 'SetMaxUtilization':
      return `Set Max Utilization → ${action.values[0]} bps`;
    case 'SetOracle':
      return `Set Oracle → ${action.values[0]}`;
    case 'RegisterDebtor':
      return `Register Debtor "${action.values[0]}" (${action.values[1]}), max exposure ${action.values[2]}`;
    case 'DeactivateDebtor':
      return `Deactivate Debtor ${action.values[0]}`;
    case 'AddKeeper':
      return `Add Keeper ${action.values[0]}`;
    case 'SetLateThreshold':
      return `Set Late Threshold → ${action.values[0]} days`;
    case 'SetScoreThresholds':
      return `Set Score Thresholds → ${action.values.join(' / ')}`;
    case 'RegisterAttestor':
      return `Register Attestor ${action.values[0]} (type ${action.values[1]}, weight ${action.values[2]}bps)`;
    case 'AddSigner':
      return `Add Signer ${action.values[1]} to ${ROLE_LABELS[action.values[0]]}`;
    case 'RemoveSigner':
      return `Remove Signer ${action.values[1]} from ${ROLE_LABELS[action.values[0]]}`;
    case 'SetThreshold':
      return `Set ${ROLE_LABELS[action.values[0]]} Threshold → ${action.values[1]}`;
    case 'SetOracleRegistryInvoiceContract':
      return `Set Invoice Contract → ${action.values[0]}`;
    case 'SetOracleRegistryTreasury':
      return `Set Treasury → ${action.values[0] ?? '(none)'}`;
    case 'SetOracleRegistryConfig':
      return `Set Registry Config → min_stake ${action.values[0]}, required_votes ${action.values[1]}, quorum_bps ${action.values[2]}, round_duration_secs ${action.values[3]}, deregister_cooldown_secs ${action.values[4]}`;
    case 'SetOracleRegistryPaused':
      return `Set Paused → ${action.values[0]}`;
    case 'SlashOracle':
      return `Slash Oracle ${action.values[0]} by ${action.values[1]}bps (round ${action.values[2]}): ${action.values[3]}`;
    case 'AdminResolveRound':
      return `Admin Resolve Round for invoice ${action.values[0]} → ${action.values[1]} (${action.values[2]})`;
    case 'SetCompliancePaused':
      return `Set Paused → ${action.values[0]}`;
    case 'RegisterScreener':
      return `Register Screener ${action.values[0]}`;
    case 'ConfirmScreenerRegistration':
      return `Confirm Screener Registration ${action.values[0]}`;
    case 'DeregisterScreener':
      return `Deregister Screener ${action.values[0]}`;
    case 'SetRescreeningInterval':
      return `Set Rescreening Interval → ${action.values[0]}s`;
    case 'SetScreenerTimelock':
      return `Set Screener Timelock → ${action.values[0]}s`;
    case 'UpdateGovernanceConfig':
      return `Update Config → quorum ${action.values[0]}bps, pass ${action.values[1]}bps, exec delay ${action.values[2]}s`;
    case 'SetCategoryQuorum':
      return `Set Category Quorum (category ${action.values[0]}) → ${action.values[1]}bps`;
    case 'SetReferralPaused':
      return `Set Paused → ${action.values[0]}`;
    case 'SetReferralPool':
      return `Set Pool → ${action.values[0]}`;
    case 'SetBorrowRewardBps':
      return `Set Borrow Reward → ${action.values[0]}bps`;
    case 'SetDepositRewardBps':
      return `Set Deposit Reward → ${action.values[0]}bps`;
    case 'SetInvoiceAccessControl':
    case 'SetCreditScoreAccessControl':
    case 'SetOracleRegistryAccessControl':
    case 'SetComplianceAccessControl':
    case 'SetGovernanceAccessControl':
    case 'SetReferralAccessControl':
      return `Rotate Access Control → ${action.values[0]}`;
  }
}
