'use client';

import { useStore } from '@/lib/store';

export default function NetworkIndicator() {
  const { networkMismatch } = useStore();

  if (!networkMismatch.walletNetwork) {
    return null;
  }

  const walletNetworkName = networkMismatch.walletNetwork === 'PUBLIC' ? 'Mainnet' : 'Testnet';
  const isMismatched = networkMismatch.isMismatched;

  return (
    <div className="flex items-center gap-2">
      <div
        className={`flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg border text-xs font-medium transition-colors ${
          isMismatched
            ? 'bg-red-900/30 border-red-800/50 text-red-400'
            : networkMismatch.walletNetwork === 'PUBLIC'
              ? 'bg-blue-900/30 border-blue-800/50 text-blue-400'
              : 'bg-green-900/30 border-green-800/50 text-green-400'
        }`}
      >
        <span
          className={`w-1.5 h-1.5 rounded-full ${
            isMismatched
              ? 'bg-red-400'
              : networkMismatch.walletNetwork === 'PUBLIC'
                ? 'bg-blue-400'
                : 'bg-green-400'
          }`}
          aria-hidden="true"
        />
        <span>{walletNetworkName}</span>
      </div>
    </div>
  );
}
