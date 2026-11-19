'use client';

import type { Dispatch, FormEvent, SetStateAction } from 'react';
import { ALL_ROLES, ROLE_LABELS } from '@/lib/types';
import type { ActionPayload, Role } from '@/lib/types';
import ActionFields from './ActionFields';
import { ACTIONS_BY_TARGET, ACTION_LABELS, TARGET_CONTRACTS } from './constants';
import type { TargetKey } from './constants';

export interface ProposalBuilderProps {
  proposeRole: Role;
  setProposeRole: Dispatch<SetStateAction<Role>>;
  proposeTarget: TargetKey;
  setProposeTarget: Dispatch<SetStateAction<TargetKey>>;
  proposeAction: ActionPayload['tag'];
  setProposeAction: Dispatch<SetStateAction<ActionPayload['tag']>>;
  fields: Record<string, string>;
  setFields: Dispatch<SetStateAction<Record<string, string>>>;
  txLoading: boolean;
  walletAddress: string | null;
  onSubmit: (e: FormEvent) => void;
}

/** The "Propose an Action" form: role/target/action selectors + dynamic fields. */
export default function ProposalBuilder({
  proposeRole,
  setProposeRole,
  proposeTarget,
  setProposeTarget,
  proposeAction,
  setProposeAction,
  fields,
  setFields,
  txLoading,
  walletAddress,
  onSubmit,
}: ProposalBuilderProps) {
  const availableActions = ACTIONS_BY_TARGET[proposeTarget];

  return (
    <div className="p-6 bg-brand-card border border-brand-border rounded-2xl space-y-4">
      <h2 className="font-semibold">Propose an Action</h2>
      <form onSubmit={onSubmit} className="space-y-3">
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
          disabled={txLoading || !walletAddress}
          className="py-2.5 px-6 bg-brand-gold text-brand-dark rounded-xl text-sm font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50"
        >
          {txLoading ? 'Processing…' : 'Submit Proposal'}
        </button>
      </form>
    </div>
  );
}
