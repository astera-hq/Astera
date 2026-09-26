'use client';

import useSWR from 'swr';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import {
  buildClaimYieldTx,
  getAcceptedTokens,
  getInvestorPosition,
  getPoolConfig,
  submitTx,
} from '@/lib/contracts';
import { formatUSDC, stablecoinLabel } from '@/lib/stellar';

type Row = { token: string; deposited: bigint; value: bigint; accrued: bigint };

async function loadPortfolio(wallet: string): Promise<{ rows: Row[]; yieldBps: number }> {
  const [tokens, config] = await Promise.all([getAcceptedTokens(), getPoolConfig()]);
  const positions = await Promise.all(tokens.map((token) => getInvestorPosition(wallet, token)));
  const rows = tokens.flatMap((token, index) => {
    const position = positions[index];
    if (!position || position.deposited === 0n) return [];
    return [
      {
        token,
        deposited: position.deposited,
        value: position.available + position.deployed,
        accrued: position.earned,
      },
    ];
  });
  return { rows, yieldBps: config.yieldBps };
}

export default function LenderPortfolio() {
  const { wallet } = useStore();
  const { data, error, mutate } = useSWR(
    wallet.connected && wallet.address ? ['lender-portfolio', wallet.address] : null,
    ([, address]) => loadPortfolio(address),
    { refreshInterval: 15_000, revalidateOnFocus: true },
  );

  async function claim(token: string) {
    if (!wallet.address) return;
    try {
      const xdr = await buildClaimYieldTx(wallet.address, token);
      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
        networkPassphrase: 'Test SDF Network ; September 2015',
        address: wallet.address,
      });
      if (signError) throw new Error(signError.message);
      await submitTx(signedTxXdr);
      toast.success('Yield claimed successfully.');
      await mutate();
    } catch (claimError) {
      toast.error(claimError instanceof Error ? claimError.message : 'Unable to claim yield.');
    }
  }

  if (error)
    return (
      <div className="rounded-2xl border border-red-500/30 bg-brand-card p-5 text-sm text-red-300">
        Could not load lender portfolio.
      </div>
    );
  if (!data)
    return (
      <div className="rounded-2xl border border-brand-border bg-brand-card p-5 text-brand-muted">
        Loading lender portfolio…
      </div>
    );
  if (!data.rows.length)
    return (
      <div className="rounded-2xl border border-brand-border bg-brand-card p-8 text-center text-brand-muted">
        No lender deposits found for this wallet.
      </div>
    );

  return (
    <section className="rounded-2xl border border-brand-border bg-brand-card p-5">
      <div className="mb-5 flex items-start justify-between">
        <div>
          <h2 className="text-lg font-semibold">Lender portfolio</h2>
          <p className="text-sm text-brand-muted">Balances and yield refresh every 15 seconds.</p>
        </div>
        <span className="rounded-full bg-brand-gold/15 px-3 py-1 text-sm text-brand-gold">
          {(data.yieldBps / 100).toFixed(2)}% APY
        </span>
      </div>
      <div className="space-y-3">
        {data.rows.map((row) => {
          const annual = (row.value * BigInt(data.yieldBps)) / 10_000n;
          return (
            <article key={row.token} className="rounded-xl border border-brand-border p-4">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div>
                  <p className="font-medium">
                    {stablecoinLabel(row.token)}
                  </p>
                  <p className="text-sm text-brand-muted">
                    Deposited {formatUSDC(row.deposited)} · Current value {formatUSDC(row.value)}
                  </p>
                </div>
                <button
                  onClick={() => claim(row.token)}
                  disabled={row.accrued <= 0n}
                  className="rounded-lg bg-brand-gold px-3 py-2 text-sm font-semibold text-brand-dark disabled:opacity-50"
                >
                  Claim {formatUSDC(row.accrued)}
                </button>
              </div>
              <div className="mt-3 grid grid-cols-2 gap-3 text-sm">
                <p className="text-brand-muted">
                  Accrued yield{' '}
                  <span className="block font-medium text-white">{formatUSDC(row.accrued)}</span>
                </p>
                <p className="text-brand-muted">
                  Projected return{' '}
                  <span className="block font-medium text-white">
                    {formatUSDC(annual)}/yr · {formatUSDC(annual / 12n)}/mo
                  </span>
                </p>
              </div>
            </article>
          );
        })}
      </div>
    </section>
  );
}
