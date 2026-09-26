'use client';

import { useState } from 'react';
import useSWR from 'swr';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import {
  getCoverageRecord,
  estimateInsurancePremium,
  buildPurchaseCoverageTx,
  buildFileClaimTx,
  getClaimHistory,
  submitTx,
} from '@/lib/contracts';
import { formatUSDC } from '@/lib/stellar';
import type { FundedInvoice } from '@/lib/types';

function tenorDaysFromDueDate(dueDate: number): number {
  const secs = dueDate - Math.floor(Date.now() / 1000);
  return Math.max(0, Math.floor(secs / 86_400));
}

export default function InsuranceCoverageBadge({
  fundedInvoice,
}: {
  fundedInvoice: FundedInvoice;
}) {
  const { wallet } = useStore();
  const [purchaseLoading, setPurchaseLoading] = useState(false);
  const [claimLoading, setClaimLoading] = useState(false);

  const { data: coverage, mutate: mutateCoverage } = useSWR(
    ['insurance-coverage', fundedInvoice.invoiceId],
    ([, id]) => getCoverageRecord(id),
  );
  const { data: premiumEstimate } = useSWR(
    !coverage ? ['insurance-premium-estimate', fundedInvoice.invoiceId, fundedInvoice.sme] : null,
    () =>
      estimateInsurancePremium(
        fundedInvoice.principal,
        fundedInvoice.sme,
        tenorDaysFromDueDate(fundedInvoice.dueDate),
        fundedInvoice.token,
      ),
  );
  const { data: claimHistory } = useSWR(
    coverage?.claimed ? ['insurance-claim-history', fundedInvoice.invoiceId] : null,
    ([, id]) => getClaimHistory(id),
  );

  async function purchase() {
    if (!wallet.address) {
      toast.error('Connect a wallet first.');
      return;
    }
    setPurchaseLoading(true);
    try {
      const txXdr = await buildPurchaseCoverageTx(
        wallet.address,
        fundedInvoice.invoiceId,
        fundedInvoice.principal,
        fundedInvoice.sme,
        fundedInvoice.dueDate,
        fundedInvoice.token,
      );
      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error: signError } = await freighter.signTransaction(txXdr, {
        networkPassphrase: 'Test SDF Network ; September 2015',
        address: wallet.address,
      });
      if (signError) throw new Error(signError.message || 'Signing rejected.');
      await submitTx(signedTxXdr);
      toast.success('Coverage purchased successfully!');
      await mutateCoverage();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : 'Unable to purchase coverage.');
    } finally {
      setPurchaseLoading(false);
    }
  }

  async function fileClaim() {
    if (!wallet.address) {
      toast.error('Connect a wallet first.');
      return;
    }
    setClaimLoading(true);
    try {
      const txXdr = await buildFileClaimTx(wallet.address, fundedInvoice.invoiceId);
      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error: signError } = await freighter.signTransaction(txXdr, {
        networkPassphrase: 'Test SDF Network ; September 2015',
        address: wallet.address,
      });
      if (signError) throw new Error(signError.message || 'Signing rejected.');
      await submitTx(signedTxXdr);
      toast.success('Claim filed successfully!');
      await mutateCoverage();
    } catch (e) {
      toast.error(
        e instanceof Error
          ? e.message
          : 'Unable to file claim — the invoice may not be in default, or there is no shortfall to claim.',
      );
    } finally {
      setClaimLoading(false);
    }
  }

  return (
    <div className="rounded-xl border border-brand-border bg-brand-card p-4">
      <div className="flex items-baseline justify-between">
        <h3 className="font-semibold">Insurance coverage</h3>
        {coverage && (
          <span
            className={`font-medium ${coverage.claimed ? 'text-yellow-400' : 'text-emerald-400'}`}
          >
            {coverage.claimed ? 'Claimed' : 'Active'}
          </span>
        )}
      </div>

      {!coverage ? (
        <>
          <p className="mt-1 text-sm text-brand-muted">
            Not covered yet. Purchase default-insurance coverage for this invoice.
          </p>
          {premiumEstimate != null && (
            <p className="mt-2 text-sm text-brand-muted">
              Estimated premium:{' '}
              <span className="font-medium text-white">{formatUSDC(premiumEstimate)}</span>
            </p>
          )}
          <button
            onClick={() => void purchase()}
            disabled={purchaseLoading || !wallet.address}
            className="mt-3 rounded-lg bg-brand-gold px-3 py-2 text-sm font-semibold text-brand-dark disabled:opacity-50"
          >
            {purchaseLoading ? 'Purchasing…' : 'Purchase coverage'}
          </button>
        </>
      ) : (
        <>
          <p className="mt-2 text-2xl font-bold">
            {(coverage.coverage_bps / 100).toFixed(0)}%
            <span className="text-sm text-brand-muted"> covered</span>
          </p>
          <p className="mt-1 text-sm text-brand-muted">
            Premium paid: {formatUSDC(coverage.premium_paid)}
          </p>
          {!coverage.claimed && (
            <button
              onClick={() => void fileClaim()}
              disabled={claimLoading || !wallet.address}
              className="mt-3 rounded-lg bg-brand-gold px-3 py-2 text-sm font-semibold text-brand-dark disabled:opacity-50"
            >
              {claimLoading ? 'Filing claim…' : 'File claim'}
            </button>
          )}
          {claimHistory && claimHistory.length > 0 && (
            <div className="mt-3 space-y-2 border-t border-brand-border pt-3">
              <p className="text-sm font-medium">Claim history</p>
              {claimHistory.map((item, i) => (
                <p key={i} className="text-sm text-brand-muted">
                  Paid out {formatUSDC(item.payout)} against a {formatUSDC(item.shortfalls)}{' '}
                  shortfall
                </p>
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}
