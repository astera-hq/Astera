'use client';

import { useStore } from '@/lib/store';

export default function NetworkMismatchBanner() {
  const { networkMismatch, setNetworkMismatch } = useStore();

  if (!networkMismatch.isMismatched) {
    return null;
  }

  const dismissBanner = () => {
    setNetworkMismatch({
      isMismatched: false,
      walletNetwork: null,
      appNetwork: null,
    });
  };

  const walletNetworkName = networkMismatch.walletNetwork === 'PUBLIC' ? 'Mainnet' : 'Testnet';
  const appNetworkName = networkMismatch.appNetwork === 'mainnet' ? 'Mainnet' : 'Testnet';

  return (
    <div className="fixed top-16 left-0 right-0 z-40 bg-red-900/95 border-b border-red-800 backdrop-blur-sm">
      <div className="max-w-7xl mx-auto px-6 py-3">
        <div className="flex items-start gap-3">
          <div className="flex-1 min-w-0">
            <p className="text-red-100 text-sm font-medium">Network Mismatch Detected</p>
            <p className="text-red-200 text-xs mt-1">
              Your Freighter wallet is connected to{' '}
              <span className="font-semibold text-red-100">{walletNetworkName}</span> but this app
              uses <span className="font-semibold text-red-100">{appNetworkName}</span>. Please
              switch networks in Freighter to continue.
            </p>
            <a
              href="https://www.freighter.app/help#how-do-i-switch-networks"
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1 text-red-300 hover:text-red-100 text-xs mt-2 transition-colors"
            >
              How to switch networks
              <svg
                width="12"
                height="12"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
                aria-hidden="true"
              >
                <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
                <polyline points="15,3 21,3 21,9" />
                <line x1="10" y1="14" x2="21" y2="3" />
              </svg>
            </a>
          </div>
          <button
            onClick={dismissBanner}
            className="flex-shrink-0 p-1 text-red-300 hover:text-red-100 transition-colors"
            aria-label="Dismiss network mismatch warning"
          >
            <svg
              width="16"
              height="16"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden="true"
            >
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>
      </div>
    </div>
  );
}
