'use client';

import { useState } from 'react';
import { getEnvConfig } from '@/lib/env';
import toast from 'react-hot-toast';

interface Props {
  address: string;
}

export default function TestnetFaucet({ address }: Props) {
  const { NEXT_PUBLIC_NETWORK, NEXT_PUBLIC_USDC_TOKEN_ID } = getEnvConfig();
  const [fundingXlm, setFundingXlm] = useState(false);
  const [fundingUsdc, setFundingUsdc] = useState(false);

  // Only render on testnet
  if (NEXT_PUBLIC_NETWORK !== 'testnet') return null;

  async function fundWithFriendbot() {
    setFundingXlm(true);
    try {
      const res = await fetch(`https://friendbot.stellar.org/?addr=${encodeURIComponent(address)}`);
      if (!res.ok) throw new Error(`Friendbot error: ${res.status}`);
      toast.success('Funded with 10,000 XLM via Friendbot!');
    } catch (e) {
      toast.error('Friendbot request failed. Try again.');
      console.error('[TestnetFaucet] Friendbot error:', e);
    } finally {
      setFundingXlm(false);
    }
  }

  async function fundWithUsdc() {
    setFundingUsdc(true);
    try {
      // Call the mock service faucet endpoint if available, otherwise guide user
      const mockUrl = process.env.NEXT_PUBLIC_MOCK_API_URL ?? 'http://localhost:4000';
      const res = await fetch(`${mockUrl}/faucet/usdc`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ address, amount: 1_000_000_000_000 }),
      });
      if (!res.ok) throw new Error(`USDC faucet error: ${res.status}`);
      toast.success('Minted 100,000 test USDC to your wallet!');
    } catch {
      toast.error(
        'USDC faucet unavailable. Run the mock service locally or mint manually via CLI.',
      );
    } finally {
      setFundingUsdc(false);
    }
  }

  return (
    <div className="rounded-2xl border border-brand-gold/30 bg-brand-gold/5 p-4 mb-6">
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
        <div>
          <p className="text-sm font-semibold text-brand-gold">Testnet Wallet</p>
          <p className="text-xs text-brand-muted mt-0.5">
            Get free testnet tokens to try the protocol.
          </p>
        </div>
        <div className="flex gap-2 flex-wrap">
          <button
            onClick={fundWithFriendbot}
            disabled={fundingXlm}
            className="px-3 py-1.5 rounded-lg bg-brand-gold text-brand-dark text-xs font-bold disabled:opacity-50 hover:bg-brand-gold-light transition-colors"
          >
            {fundingXlm ? 'Funding…' : 'Fund with Friendbot (XLM)'}
          </button>
          <button
            onClick={fundWithUsdc}
            disabled={fundingUsdc}
            className="px-3 py-1.5 rounded-lg border border-brand-gold/40 text-brand-gold text-xs font-bold disabled:opacity-50 hover:bg-brand-gold/10 transition-colors"
          >
            {fundingUsdc ? 'Minting…' : 'Get Test USDC'}
          </button>
        </div>
      </div>
    </div>
  );
}
