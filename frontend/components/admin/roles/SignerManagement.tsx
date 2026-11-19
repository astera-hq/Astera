'use client';

import { Skeleton } from '@/components/Skeleton';
import { ALL_ROLES, ROLE_LABELS } from '@/lib/types';
import type { MultiSigConfig, Role } from '@/lib/types';

export interface SignerManagementProps {
  loading: boolean;
  roleConfigs: Record<Role, MultiSigConfig | null> | null;
  isSigner: Record<Role, boolean>;
}

/**
 * Role configuration overview. Adding/removing signers and changing
 * thresholds are performed through the multisig proposal flow (the
 * `AddSigner` / `RemoveSigner` / `SetThreshold` actions in ProposalBuilder),
 * so this component only renders the current on-chain configuration.
 */
export default function SignerManagement({
  loading,
  roleConfigs,
  isSigner,
}: SignerManagementProps) {
  return (
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
  );
}
