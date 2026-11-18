'use client';

import { useCallback, useEffect, useMemo, useState } from 'react';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { Skeleton } from '@/components/Skeleton';
import { parseStellarAddress } from '@/lib/types';
import type { Role, MultiSigConfig, ActionPayload, Proposal } from '@/lib/types';
import { ALL_ROLES, ROLE_LABELS } from '@/lib/types';
import {
  NETWORK,
  POOL_CONTRACT_ID,
  INVOICE_CONTRACT_ID,
  CREDIT_SCORE_CONTRACT_ID,
  ORACLE_REGISTRY_CONTRACT_ID,
  COMPLIANCE_CONTRACT_ID,
  GOVERNANCE_CONTRACT_ID,
  REFERRAL_CONTRACT_ID,
  ACCESS_CONTROL_CONTRACT_ID,
} from '@/lib/stellar';
import {
  listAllRoleConfigs,
  listProposals,
  buildProposeActionTx,
  buildApproveActionTx,
  buildRejectActionTx,
  buildRevokeApprovalTx,
  buildExecuteActionTx,
  submitTx,
  getContractErrorMessage,
} from '@/lib/contracts';

type TargetKey =
  | 'pool'
  | 'invoice'
  | 'credit_score'
  | 'oracle_registry'
  | 'compliance'
  | 'governance'
  | 'referral'
  | 'self';

const TARGET_CONTRACTS: Record<TargetKey, { label: string; id: string }> = {
  pool: { label: 'Pool', id: POOL_CONTRACT_ID },
  invoice: { label: 'Invoice', id: INVOICE_CONTRACT_ID },
  credit_score: { label: 'Credit Score', id: CREDIT_SCORE_CONTRACT_ID },
  oracle_registry: { label: 'Oracle Registry', id: ORACLE_REGISTRY_CONTRACT_ID },
  compliance: { label: 'Compliance', id: COMPLIANCE_CONTRACT_ID },
  governance: { label: 'Governance', id: GOVERNANCE_CONTRACT_ID },
  referral: { label: 'Referral', id: REFERRAL_CONTRACT_ID },
  self: { label: 'Access Control (self-management)', id: ACCESS_CONTROL_CONTRACT_ID },
};

// Which ActionPayload variants make sense for each target — mirrors the
// `execute_cross_contract`/`execute_self_management` match arms in
// contracts/access_control/src/lib.rs. Every target except `pool` also
// accepts its own `Set*AccessControl` rotation action (#1042) — proposed
// under `Role::SuperAdmin`, same as the self-management actions. `pool`
// has no wasm size budget left for a new entrypoint (see
// contracts/.wasm-size-baseline.json), so its rotation action was
// dropped from this PR.
const ACTIONS_BY_TARGET: Record<TargetKey, ActionPayload['tag'][]> = {
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

const ACTION_LABELS: Record<ActionPayload['tag'], string> = {
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

function summarizeAction(action: ActionPayload): string {
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

export default function RolesAdminPage() {
  const { wallet } = useStore();
  const [roleConfigs, setRoleConfigs] = useState<Record<Role, MultiSigConfig | null> | null>(null);
  const [proposals, setProposals] = useState<Array<{ id: number; proposal: Proposal }>>([]);
  const [loading, setLoading] = useState(true);
  const [txLoading, setTxLoading] = useState(false);

  const [proposeRole, setProposeRole] = useState<Role>('RiskManager');
  const [proposeTarget, setProposeTarget] = useState<TargetKey>('pool');
  const [proposeAction, setProposeAction] = useState<ActionPayload['tag']>('SetYield');
  const [fields, setFields] = useState<Record<string, string>>({});

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [configs, props] = await Promise.all([listAllRoleConfigs(), listProposals()]);
      setRoleConfigs(configs);
      setProposals(props.reverse());
    } catch (e) {
      console.error(e);
      toast.error('Failed to load access-control state.');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  async function signAndSubmit(xdr: string) {
    const freighter = await import('@stellar/freighter-api');
    const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
      networkPassphrase: NETWORK,
      address: wallet.address!,
    });
    if (signError) throw new Error(signError.message || 'Signing rejected.');
    await submitTx(signedTxXdr);
  }

  const isSigner = useMemo(() => {
    if (!wallet.address || !roleConfigs) return {} as Record<Role, boolean>;
    const address = wallet.address;
    const result = {} as Record<Role, boolean>;
    for (const role of ALL_ROLES) {
      result[role] = roleConfigs[role]?.signers.some((s) => s === address) ?? false;
    }
    return result;
  }, [wallet.address, roleConfigs]);

  function buildActionPayload(): ActionPayload | null {
    const num = (key: string) => Number(fields[key] ?? '0');
    const str = (key: string) => (fields[key] ?? '').trim();
    switch (proposeAction) {
      case 'SetPaused':
      case 'SetKycRequired':
        return { tag: proposeAction, values: [fields[`${proposeAction}_value`] === 'true'] };
      case 'SetYield':
      case 'SetMaxUtilization':
        return { tag: proposeAction, values: [num('bps')] };
      case 'SetTreasury':
      case 'SetOracleContract':
      case 'SetOracle':
      case 'AddKeeper':
        return { tag: proposeAction, values: [parseStellarAddress(str('address'))] };
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
          tag: proposeAction,
          values: [str('role') as Role, parseStellarAddress(str('address'))],
        };
      case 'SetThreshold':
        return { tag: 'SetThreshold', values: [str('role') as Role, num('threshold')] };
      case 'SetOracleRegistryInvoiceContract':
      case 'SetReferralPool':
        return { tag: proposeAction, values: [parseStellarAddress(str('address'))] };
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
        return { tag: proposeAction, values: [fields[`${proposeAction}_value`] === 'true'] };
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
        return { tag: proposeAction, values: [parseStellarAddress(str('screener'))] };
      case 'SetRescreeningInterval':
      case 'SetScreenerTimelock':
        return { tag: proposeAction, values: [BigInt(str('secs') || '0')] };
      case 'UpdateGovernanceConfig':
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
        return { tag: proposeAction, values: [num('bps')] };
      case 'SetInvoiceAccessControl':
      case 'SetCreditScoreAccessControl':
      case 'SetOracleRegistryAccessControl':
      case 'SetComplianceAccessControl':
      case 'SetGovernanceAccessControl':
      case 'SetReferralAccessControl':
        return { tag: proposeAction, values: [parseStellarAddress(str('address'))] };
    }
  }

  async function handlePropose(e: React.FormEvent) {
    e.preventDefault();
    if (!wallet.address) return;
    setTxLoading(true);
    try {
      const action = buildActionPayload();
      if (!action) throw new Error('Fill in the action fields.');
      const target =
        proposeTarget === 'self'
          ? parseStellarAddress(ACCESS_CONTROL_CONTRACT_ID)
          : parseStellarAddress(TARGET_CONTRACTS[proposeTarget].id);
      const xdr = await buildProposeActionTx({
        role: proposeRole,
        proposer: parseStellarAddress(wallet.address),
        target,
        action,
      });
      await signAndSubmit(xdr);
      toast.success('Proposal submitted.');
      setFields({});
      await load();
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : 'Transaction failed.';
      toast.error(getContractErrorMessage(message));
    } finally {
      setTxLoading(false);
    }
  }

  async function handleSignerAction(
    builder: typeof buildApproveActionTx,
    proposalId: number,
    successMessage: string,
  ) {
    if (!wallet.address) return;
    setTxLoading(true);
    try {
      const xdr = await builder({ signer: parseStellarAddress(wallet.address), proposalId });
      await signAndSubmit(xdr);
      toast.success(successMessage);
      await load();
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : 'Transaction failed.';
      toast.error(getContractErrorMessage(message));
    } finally {
      setTxLoading(false);
    }
  }

  async function handleExecute(proposalId: number) {
    if (!wallet.address) return;
    setTxLoading(true);
    try {
      const xdr = await buildExecuteActionTx({
        caller: parseStellarAddress(wallet.address),
        proposalId,
      });
      await signAndSubmit(xdr);
      toast.success(`Proposal #${proposalId} executed.`);
      await load();
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : 'Transaction failed.';
      toast.error(getContractErrorMessage(message));
    } finally {
      setTxLoading(false);
    }
  }

  const availableActions = ACTIONS_BY_TARGET[proposeTarget];

  return (
    <div className="max-w-5xl space-y-8">
      <div>
        <h1 className="text-3xl font-bold mb-2">Roles & Multisig Access Control</h1>
        <p className="text-brand-muted text-sm">
          Sensitive admin actions across Pool, Invoice, Credit Score, Oracle Registry, Compliance,
          Governance, and Referral now route through role-based multisig proposals: propose, gather
          approvals up to each role&apos;s threshold, then execute.
        </p>
      </div>

      {/* Role configs */}
      <div className="space-y-4">
        <h2 className="text-xl font-bold">Roles</h2>
        {loading ? (
          <Skeleton className="h-40 w-full rounded-2xl" />
        ) : (
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            {ALL_ROLES.map((role) => {
              const config = roleConfigs?.[role] ?? null;
              return (
                <div
                  key={role}
                  className="p-5 bg-brand-card border border-brand-border rounded-2xl space-y-2"
                >
                  <div className="flex items-center justify-between">
                    <h3 className="font-semibold">{ROLE_LABELS[role]}</h3>
                    {isSigner[role] && (
                      <span className="text-xs px-2 py-0.5 rounded-full bg-brand-gold/10 text-brand-gold">
                        You are a signer
                      </span>
                    )}
                  </div>
                  {!config ? (
                    <p className="text-brand-muted text-sm">Not configured yet.</p>
                  ) : (
                    <>
                      <p className="text-sm text-brand-muted">
                        Threshold: {config.threshold} of {config.signers.length}
                      </p>
                      <ul className="text-xs font-mono space-y-1">
                        {config.signers.map((s) => (
                          <li key={s} className="text-brand-muted">
                            {s.slice(0, 8)}…{s.slice(-6)}
                          </li>
                        ))}
                      </ul>
                    </>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Propose new action */}
      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl space-y-4">
        <h2 className="font-semibold">Propose an Action</h2>
        <form onSubmit={handlePropose} className="space-y-3">
          <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
            <select
              value={proposeRole}
              onChange={(e) => setProposeRole(e.target.value as Role)}
              className="bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white focus:outline-none focus:border-brand-gold text-sm"
            >
              {ALL_ROLES.map((r) => (
                <option key={r} value={r}>
                  {ROLE_LABELS[r]}
                </option>
              ))}
            </select>
            <select
              value={proposeTarget}
              onChange={(e) => {
                const target = e.target.value as TargetKey;
                setProposeTarget(target);
                setProposeAction(ACTIONS_BY_TARGET[target][0]!);
                setFields({});
              }}
              className="bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white focus:outline-none focus:border-brand-gold text-sm"
            >
              {(Object.keys(TARGET_CONTRACTS) as TargetKey[]).map((t) => (
                <option key={t} value={t}>
                  {TARGET_CONTRACTS[t].label}
                </option>
              ))}
            </select>
            <select
              value={proposeAction}
              onChange={(e) => {
                setProposeAction(e.target.value as ActionPayload['tag']);
                setFields({});
              }}
              className="bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white focus:outline-none focus:border-brand-gold text-sm"
            >
              {availableActions.map((a) => (
                <option key={a} value={a}>
                  {ACTION_LABELS[a]}
                </option>
              ))}
            </select>
          </div>

          <ActionFields action={proposeAction} fields={fields} setFields={setFields} />

          <button
            type="submit"
            disabled={txLoading || !wallet.address}
            className="py-2.5 px-6 bg-brand-gold text-brand-dark rounded-xl text-sm font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50"
          >
            {txLoading ? 'Processing…' : 'Submit Proposal'}
          </button>
        </form>
      </div>

      {/* Proposals queue */}
      <div className="space-y-4">
        <h2 className="text-xl font-bold">Proposals</h2>
        {loading ? (
          <Skeleton className="h-32 w-full rounded-2xl" />
        ) : proposals.length === 0 ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">
            No proposals yet.
          </div>
        ) : (
          <div className="space-y-3">
            {proposals.map(({ id, proposal }) => {
              const address = wallet.address;
              const canApprove =
                isSigner[proposal.role] &&
                proposal.status === 'Pending' &&
                !!address &&
                !proposal.approvals.some((a) => a === address);
              const canRevoke =
                !!address &&
                proposal.approvals.some((a) => a === address) &&
                (proposal.status === 'Pending' || proposal.status === 'Approved');
              const canReject = isSigner[proposal.role] && proposal.status === 'Pending';
              const canExecute = proposal.status === 'Approved';
              const config = roleConfigs?.[proposal.role] ?? null;

              return (
                <div
                  key={id}
                  className="p-5 bg-brand-card border border-brand-border rounded-2xl space-y-2"
                >
                  <div className="flex items-center justify-between flex-wrap gap-2">
                    <div>
                      <span className="text-xs text-brand-muted">
                        #{id} · {ROLE_LABELS[proposal.role]}
                      </span>
                      <p className="font-medium text-sm">{summarizeAction(proposal.action)}</p>
                    </div>
                    <span
                      className={`text-xs px-2 py-0.5 rounded-full ${
                        proposal.status === 'Executed'
                          ? 'bg-green-500/20 text-green-400'
                          : proposal.status === 'Rejected'
                            ? 'bg-red-500/20 text-red-400'
                            : proposal.status === 'Approved'
                              ? 'bg-brand-gold/20 text-brand-gold'
                              : 'bg-brand-border text-brand-muted'
                      }`}
                    >
                      {proposal.status}
                    </span>
                  </div>
                  <p className="text-xs text-brand-muted">
                    Approvals: {proposal.approvals.length} / {config?.threshold ?? '?'} · Proposer{' '}
                    {proposal.proposer.slice(0, 8)}… · Expires{' '}
                    {new Date(proposal.expiresAt * 1000).toLocaleString()}
                  </p>
                  <div className="flex gap-2 pt-1">
                    {canApprove && (
                      <button
                        onClick={() =>
                          handleSignerAction(buildApproveActionTx, id, `Approved proposal #${id}.`)
                        }
                        disabled={txLoading}
                        className="px-3 py-1.5 bg-green-500/20 text-green-400 border border-green-500/30 rounded-lg text-xs font-semibold hover:bg-green-500/30 transition-colors disabled:opacity-50"
                      >
                        Approve
                      </button>
                    )}
                    {canExecute && (
                      <button
                        onClick={() => handleExecute(id)}
                        disabled={txLoading}
                        className="px-3 py-1.5 bg-brand-gold/20 text-brand-gold border border-brand-gold/30 rounded-lg text-xs font-semibold hover:bg-brand-gold/30 transition-colors disabled:opacity-50"
                      >
                        Execute
                      </button>
                    )}
                    {canRevoke && (
                      <button
                        onClick={() =>
                          handleSignerAction(
                            buildRevokeApprovalTx,
                            id,
                            `Revoked approval on #${id}.`,
                          )
                        }
                        disabled={txLoading}
                        className="px-3 py-1.5 bg-brand-dark border border-brand-border rounded-lg text-xs font-semibold hover:bg-brand-border transition-colors disabled:opacity-50"
                      >
                        Revoke Approval
                      </button>
                    )}
                    {canReject && (
                      <button
                        onClick={() =>
                          handleSignerAction(buildRejectActionTx, id, `Rejected proposal #${id}.`)
                        }
                        disabled={txLoading}
                        className="px-3 py-1.5 bg-red-500/20 text-red-400 border border-red-500/30 rounded-lg text-xs font-semibold hover:bg-red-500/30 transition-colors disabled:opacity-50"
                      >
                        Reject
                      </button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

function ActionFields({
  action,
  fields,
  setFields,
}: {
  action: ActionPayload['tag'];
  fields: Record<string, string>;
  setFields: React.Dispatch<React.SetStateAction<Record<string, string>>>;
}) {
  const set = (key: string) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
    setFields((f) => ({ ...f, [key]: e.target.value }));

  const inputClass =
    'bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm';

  switch (action) {
    case 'SetPaused':
    case 'SetKycRequired':
      return (
        <select
          value={fields[`${action}_value`] ?? 'true'}
          onChange={set(`${action}_value`)}
          className={inputClass}
        >
          <option value="true">true</option>
          <option value="false">false</option>
        </select>
      );
    case 'SetYield':
    case 'SetMaxUtilization':
      return (
        <input
          type="number"
          placeholder="Basis points"
          value={fields.bps ?? ''}
          onChange={set('bps')}
          className={inputClass}
        />
      );
    case 'SetTreasury':
    case 'SetOracleContract':
    case 'SetOracle':
    case 'AddKeeper':
      return (
        <input
          type="text"
          placeholder="Address (G... or C...)"
          value={fields.address ?? ''}
          onChange={set('address')}
          className={`${inputClass} font-mono w-full`}
        />
      );
    case 'WithdrawRevenue':
      return (
        <div className="grid grid-cols-2 gap-3">
          <input
            type="text"
            placeholder="Token contract"
            value={fields.token ?? ''}
            onChange={set('token')}
            className={`${inputClass} font-mono`}
          />
          <input
            type="text"
            placeholder="Amount (stroops)"
            value={fields.amount ?? ''}
            onChange={set('amount')}
            className={inputClass}
          />
        </div>
      );
    case 'SetInvestorKyc':
      return (
        <div className="grid grid-cols-2 gap-3">
          <input
            type="text"
            placeholder="Investor address"
            value={fields.investor ?? ''}
            onChange={set('investor')}
            className={`${inputClass} font-mono`}
          />
          <select
            value={fields.approved ?? 'true'}
            onChange={set('approved')}
            className={inputClass}
          >
            <option value="true">Approved</option>
            <option value="false">Not approved</option>
          </select>
        </div>
      );
    case 'RegisterDebtor':
      return (
        <div className="grid grid-cols-3 gap-3">
          <input
            type="text"
            placeholder="Debtor ID"
            value={fields.debtorId ?? ''}
            onChange={set('debtorId')}
            className={inputClass}
          />
          <input
            type="text"
            placeholder="Debtor name"
            value={fields.debtorName ?? ''}
            onChange={set('debtorName')}
            className={inputClass}
          />
          <input
            type="text"
            placeholder="Max exposure"
            value={fields.maxExposure ?? ''}
            onChange={set('maxExposure')}
            className={inputClass}
          />
        </div>
      );
    case 'DeactivateDebtor':
      return (
        <input
          type="text"
          placeholder="Debtor ID"
          value={fields.debtorId ?? ''}
          onChange={set('debtorId')}
          className={inputClass}
        />
      );
    case 'SetLateThreshold':
      return (
        <input
          type="number"
          placeholder="Days"
          value={fields.days ?? ''}
          onChange={set('days')}
          className={inputClass}
        />
      );
    case 'SetScoreThresholds':
      return (
        <div className="grid grid-cols-4 gap-3">
          <input
            placeholder="Excellent"
            value={fields.excellent ?? ''}
            onChange={set('excellent')}
            className={inputClass}
          />
          <input
            placeholder="Very good"
            value={fields.veryGood ?? ''}
            onChange={set('veryGood')}
            className={inputClass}
          />
          <input
            placeholder="Good"
            value={fields.good ?? ''}
            onChange={set('good')}
            className={inputClass}
          />
          <input
            placeholder="Fair"
            value={fields.fair ?? ''}
            onChange={set('fair')}
            className={inputClass}
          />
        </div>
      );
    case 'RegisterAttestor':
      return (
        <div className="grid grid-cols-3 gap-3">
          <input
            type="text"
            placeholder="Attestor address"
            value={fields.address ?? ''}
            onChange={set('address')}
            className={`${inputClass} font-mono`}
          />
          <select
            value={fields.attestorType ?? '0'}
            onChange={set('attestorType')}
            className={inputClass}
          >
            <option value="0">Business Registry</option>
            <option value="1">Credit Bureau</option>
            <option value="2">External Protocol</option>
            <option value="3">Manual</option>
          </select>
          <input
            type="number"
            placeholder="Weight (bps)"
            value={fields.weightBps ?? ''}
            onChange={set('weightBps')}
            className={inputClass}
          />
        </div>
      );
    case 'AddSigner':
    case 'RemoveSigner':
      return (
        <div className="grid grid-cols-2 gap-3">
          <select value={fields.role ?? 'SuperAdmin'} onChange={set('role')} className={inputClass}>
            {ALL_ROLES.map((r) => (
              <option key={r} value={r}>
                {ROLE_LABELS[r]}
              </option>
            ))}
          </select>
          <input
            type="text"
            placeholder="Signer address"
            value={fields.address ?? ''}
            onChange={set('address')}
            className={`${inputClass} font-mono`}
          />
        </div>
      );
    case 'SetThreshold':
      return (
        <div className="grid grid-cols-2 gap-3">
          <select value={fields.role ?? 'SuperAdmin'} onChange={set('role')} className={inputClass}>
            {ALL_ROLES.map((r) => (
              <option key={r} value={r}>
                {ROLE_LABELS[r]}
              </option>
            ))}
          </select>
          <input
            type="number"
            placeholder="New threshold"
            value={fields.threshold ?? ''}
            onChange={set('threshold')}
            className={inputClass}
          />
        </div>
      );
    case 'SetOracleRegistryInvoiceContract':
    case 'SetReferralPool':
    case 'SetInvoiceAccessControl':
    case 'SetCreditScoreAccessControl':
    case 'SetOracleRegistryAccessControl':
    case 'SetComplianceAccessControl':
    case 'SetGovernanceAccessControl':
    case 'SetReferralAccessControl':
      return (
        <input
          type="text"
          placeholder="Address (G... or C...)"
          value={fields.address ?? ''}
          onChange={set('address')}
          className={`${inputClass} font-mono w-full`}
        />
      );
    case 'SetOracleRegistryTreasury':
      return (
        <input
          type="text"
          placeholder="Treasury address (blank to clear)"
          value={fields.treasury ?? ''}
          onChange={set('treasury')}
          className={`${inputClass} font-mono w-full`}
        />
      );
    case 'SetOracleRegistryConfig':
      return (
        <div className="grid grid-cols-5 gap-3">
          <input
            placeholder="Min stake"
            value={fields.minStake ?? ''}
            onChange={set('minStake')}
            className={inputClass}
          />
          <input
            placeholder="Required votes"
            value={fields.requiredVotes ?? ''}
            onChange={set('requiredVotes')}
            className={inputClass}
          />
          <input
            placeholder="Quorum bps"
            value={fields.quorumBps ?? ''}
            onChange={set('quorumBps')}
            className={inputClass}
          />
          <input
            placeholder="Round duration (secs)"
            value={fields.roundDurationSecs ?? ''}
            onChange={set('roundDurationSecs')}
            className={inputClass}
          />
          <input
            placeholder="Deregister cooldown (secs)"
            value={fields.deregisterCooldownSecs ?? ''}
            onChange={set('deregisterCooldownSecs')}
            className={inputClass}
          />
        </div>
      );
    case 'SetOracleRegistryPaused':
    case 'SetCompliancePaused':
    case 'SetReferralPaused':
      return (
        <select
          value={fields[`${action}_value`] ?? 'true'}
          onChange={set(`${action}_value`)}
          className={inputClass}
        >
          <option value="true">true</option>
          <option value="false">false</option>
        </select>
      );
    case 'SlashOracle':
      return (
        <div className="grid grid-cols-4 gap-3">
          <input
            type="text"
            placeholder="Operator address"
            value={fields.operator ?? ''}
            onChange={set('operator')}
            className={`${inputClass} font-mono`}
          />
          <input
            type="number"
            placeholder="Bps"
            value={fields.bps ?? ''}
            onChange={set('bps')}
            className={inputClass}
          />
          <input
            type="text"
            placeholder="Round ID"
            value={fields.roundId ?? ''}
            onChange={set('roundId')}
            className={inputClass}
          />
          <input
            type="text"
            placeholder="Evidence"
            value={fields.evidence ?? ''}
            onChange={set('evidence')}
            className={inputClass}
          />
        </div>
      );
    case 'AdminResolveRound':
      return (
        <div className="grid grid-cols-3 gap-3">
          <input
            type="text"
            placeholder="Invoice ID"
            value={fields.invoiceId ?? ''}
            onChange={set('invoiceId')}
            className={inputClass}
          />
          <select
            value={fields.approved ?? 'true'}
            onChange={set('approved')}
            className={inputClass}
          >
            <option value="true">Approved</option>
            <option value="false">Rejected</option>
          </select>
          <input
            type="text"
            placeholder="Reason"
            value={fields.reason ?? ''}
            onChange={set('reason')}
            className={inputClass}
          />
        </div>
      );
    case 'RegisterScreener':
    case 'ConfirmScreenerRegistration':
    case 'DeregisterScreener':
      return (
        <input
          type="text"
          placeholder="Screener address"
          value={fields.screener ?? ''}
          onChange={set('screener')}
          className={`${inputClass} font-mono w-full`}
        />
      );
    case 'SetRescreeningInterval':
    case 'SetScreenerTimelock':
      return (
        <input
          type="number"
          placeholder="Seconds"
          value={fields.secs ?? ''}
          onChange={set('secs')}
          className={inputClass}
        />
      );
    case 'UpdateGovernanceConfig':
      return (
        <div className="grid grid-cols-3 gap-3">
          <input
            type="number"
            placeholder="Quorum bps"
            value={fields.quorumBps ?? ''}
            onChange={set('quorumBps')}
            className={inputClass}
          />
          <input
            type="number"
            placeholder="Pass bps"
            value={fields.passBps ?? ''}
            onChange={set('passBps')}
            className={inputClass}
          />
          <input
            type="number"
            placeholder="Execution delay (secs)"
            value={fields.executionDelaySecs ?? ''}
            onChange={set('executionDelaySecs')}
            className={inputClass}
          />
        </div>
      );
    case 'SetCategoryQuorum':
      return (
        <div className="grid grid-cols-2 gap-3">
          <select value={fields.category ?? '0'} onChange={set('category')} className={inputClass}>
            <option value="0">Parameter Change</option>
            <option value="1">Treasury</option>
            <option value="2">Critical</option>
          </select>
          <input
            type="number"
            placeholder="Quorum bps"
            value={fields.quorumBps ?? ''}
            onChange={set('quorumBps')}
            className={inputClass}
          />
        </div>
      );
    case 'SetBorrowRewardBps':
    case 'SetDepositRewardBps':
      return (
        <input
          type="number"
          placeholder="Basis points"
          value={fields.bps ?? ''}
          onChange={set('bps')}
          className={inputClass}
        />
      );
  }
}
