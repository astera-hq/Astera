"use client";

import { useEffect, useState } from "react";
import { useStore } from "@/lib/store";
import { truncateAddress } from "@/lib/stellar";

export default function WalletConnect() {
  const { wallet, setWallet, disconnect } = useStore();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function connect() {
    setLoading(true);
    setError(null);
    try {
      const { isConnected, getAddress, isAllowed, setAllowed } = await import(
        "@stellar/freighter-api"
      );

      const connected = await isConnected();
      if (!connected) {
        setError("Freighter not detected. Install the extension first.");
        return;
      }

      const allowed = await isAllowed();
      if (!allowed) {
        await setAllowed();
      }

      const { address } = await getAddress();
      setWallet({ address, connected: true, network: "testnet" });
    } catch (e) {
      setError("Failed to connect wallet.");
      console.error(e);
    } finally {
      setLoading(false);
    }
  }

  if (wallet.connected && wallet.address) {
    return (
      <div className="flex items-center gap-3">
        <div className="hidden sm:flex items-center gap-2 px-3 py-1.5 rounded-lg bg-green-900/30 border border-green-800/50 text-green-400 text-sm">
          <span className="w-1.5 h-1.5 rounded-full bg-green-400" />
          {truncateAddress(wallet.address)}
        </div>
        <button
          onClick={disconnect}
          className="text-sm text-brand-muted hover:text-white transition-colors"
        >
          Disconnect
        </button>
      </div>
    );
  }

  return (
    <div className="flex flex-col items-end gap-1">
      <button
        onClick={connect}
        disabled={loading}
        className="px-4 py-2 bg-brand-gold text-brand-dark font-semibold rounded-lg hover:bg-brand-amber transition-colors text-sm disabled:opacity-60"
      >
        {loading ? "Connecting..." : "Connect Wallet"}
      </button>
      {error && <p className="text-red-400 text-xs">{error}</p>}
    </div>
  );
}
