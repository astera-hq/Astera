'use client';

import type { ActionPayload } from '@/lib/types';
import { ALL_ROLES, ROLE_LABELS } from '@/lib/types';

export interface ActionFieldsProps {
  action: ActionPayload['tag'];
  fields: Record<string, string>;
  setFields: React.Dispatch<React.SetStateAction<Record<string, string>>>;
}

export default function ActionFields({ action, fields, setFields }: ActionFieldsProps) {
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
