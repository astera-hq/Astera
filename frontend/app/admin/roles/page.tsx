'use client';

import { useCallback, useEffect, useMemo, useState } from 'react';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { parseStellarAddress } from '@/lib/types';
import type { Role, MultiSigConfig, ActionPayload, Proposal } from '@/lib/types';
import { ALL_ROLES } from '@/lib/types';
import { ACCESS_CONTROL_CONTRACT_ID } from '@/lib/stellar';
import { sdk, sdkSigner } from '@/lib/sdk';
import { getContractErrorMessage } from '@/lib/contracts';
import ProposalBuilder from '@/components/admin/roles/ProposalBuilder';
import ProposalList from '@/components/admin/roles/ProposalList';
import SignerManagement from '@/components/admin/roles/SignerManagement';
import { TARGET_CONTRACTS } from '@/components/admin/roles/constants';
import { buildActionPayload } from '@/components/admin/roles/proposalPayload';
import type { TargetKey } from '@/components/admin/roles/proposalPayload';

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
      const [configsList, props] = await Promise.all([
        Promise.all(ALL_ROLES.map((role) => sdk.accessControl.getRoleConfig(role))),
        sdk.accessControl.listProposals(),
      ]);
      setRoleConfigs(
        Object.fromEntries(
          ALL_ROLES.map((role, i) => [role, configsList[i] as MultiSigConfig | null]),
        ) as Record<Role, MultiSigConfig | null>,
      );
      setProposals(
        props
          .map(({ id, proposal }) => ({
            id: Number(id),
            proposal: {
              ...proposal,
              createdAt: Number(proposal.createdAt),
              expiresAt: Number(proposal.expiresAt),
              action: proposal.action as unknown as ActionPayload,
            } as unknown as Proposal,
          }))
          .reverse(),
      );
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

  const isSigner = useMemo(() => {
    if (!wallet.address || !roleConfigs) return {} as Record<Role, boolean>;
    const address = wallet.address;
    const result = {} as Record<Role, boolean>;
    for (const role of ALL_ROLES) {
      result[role] = roleConfigs[role]?.signers.some((s) => s === address) ?? false;
    }
    return result;
  }, [wallet.address, roleConfigs]);

  async function handlePropose(e: React.FormEvent) {
    e.preventDefault();
    if (!wallet.address) return;
    setTxLoading(true);
    try {
      const action = buildActionPayload(proposeAction, fields);
      if (!action) throw new Error('Fill in the action fields.');
      const target =
        proposeTarget === 'self'
          ? parseStellarAddress(ACCESS_CONTROL_CONTRACT_ID)
          : parseStellarAddress(TARGET_CONTRACTS[proposeTarget].id);
      await sdk.accessControl.proposeAction({
        signer: await sdkSigner(wallet.address),
        role: proposeRole,
        proposer: parseStellarAddress(wallet.address),
        target,
        action: action as Parameters<typeof sdk.accessControl.proposeAction>[0]['action'],
      });
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

  type SignerAction = (params: {
    signer: Awaited<ReturnType<typeof sdkSigner>>;
    signerAddress: string;
    proposalId: number;
  }) => Promise<string>;

  async function handleSignerAction(
    action: SignerAction,
    proposalId: number,
    successMessage: string,
  ) {
    if (!wallet.address) return;
    setTxLoading(true);
    try {
      await action({
        signer: await sdkSigner(wallet.address),
        signerAddress: wallet.address,
        proposalId,
      });
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
      await sdk.accessControl.executeAction({
        signer: await sdkSigner(wallet.address),
        caller: wallet.address,
        proposalId,
      });
      toast.success(`Proposal #${proposalId} executed.`);
      await load();
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : 'Transaction failed.';
      toast.error(getContractErrorMessage(message));
    } finally {
      setTxLoading(false);
    }
  }

  const handleApprove = (id: number) =>
    handleSignerAction((p) => sdk.accessControl.approveAction(p), id, `Approved proposal #${id}.`);
  const handleReject = (id: number) =>
    handleSignerAction((p) => sdk.accessControl.rejectAction(p), id, `Rejected proposal #${id}.`);
  const handleRevoke = (id: number) =>
    handleSignerAction((p) => sdk.accessControl.revokeApproval(p), id, `Revoked approval on #${id}.`);

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

      <SignerManagement loading={loading} roleConfigs={roleConfigs} isSigner={isSigner} />

      <ProposalBuilder
        proposeRole={proposeRole}
        setProposeRole={setProposeRole}
        proposeTarget={proposeTarget}
        setProposeTarget={setProposeTarget}
        proposeAction={proposeAction}
        setProposeAction={setProposeAction}
        fields={fields}
        setFields={setFields}
        txLoading={txLoading}
        walletAddress={wallet.address}
        onSubmit={handlePropose}
      />

      <ProposalList
        proposals={proposals}
        loading={loading}
        walletAddress={wallet.address}
        isSigner={isSigner}
        roleConfigs={roleConfigs}
        txLoading={txLoading}
        onApprove={handleApprove}
        onReject={handleReject}
        onRevoke={handleRevoke}
        onExecute={handleExecute}
      />
    </div>
  );
}
