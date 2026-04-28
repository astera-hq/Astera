import { create } from 'zustand';
import type { WalletState, PoolConfig, InvestorPosition } from './types';
import type { StoreEvent } from './sse-events';

// NOTE: Contract-derived objects (PoolConfig, InvestorPosition, etc.) are
// stored as-is in memory — including any `bigint` values needed for math.
// Do NOT `JSON.stringify` these objects directly. Use `safeStringify` from
// `lib/stellar.ts` for any logging, network or persistence serialization.
// See `safeSerialize` doc block in `lib/stellar.ts` for details.

const WALLET_KEY = 'astera_wallet_address';

export function getStoredWalletAddress(): string | null {
  if (typeof window === 'undefined') return null;
  return localStorage.getItem(WALLET_KEY);
}

interface AsteraStore {
  wallet: WalletState;
  poolConfig: PoolConfig | null;
  position: InvestorPosition | null;

  // SSE Events state
  recentEvents: StoreEvent[];
  lastPollTime: number | null;
  pollingInterval: number;

  // Network mismatch state
  networkMismatch: {
    isMismatched: boolean;
    walletNetwork: string | null;
    appNetwork: string | null;
  };

  setWallet: (wallet: WalletState) => void;
  setPoolConfig: (config: PoolConfig) => void;
  setPosition: (position: InvestorPosition | null) => void;
  setRecentEvents: (events: StoreEvent[]) => void;
  setLastPollTime: (time: number) => void;
  setPollingInterval: (interval: number) => void;
  setNetworkMismatch: (mismatch: {
    isMismatched: boolean;
    walletNetwork: string | null;
    appNetwork: string | null;
  }) => void;
  disconnect: () => void;
  refreshPosition: () => void;
}

export const useStore = create<AsteraStore>((set, get) => ({
  wallet: { address: null, connected: false, network: 'testnet' },
  poolConfig: null,
  position: null,

  // SSE Events state
  recentEvents: [],
  lastPollTime: null,
  pollingInterval: 15_000,

  // Network mismatch state
  networkMismatch: {
    isMismatched: false,
    walletNetwork: null,
    appNetwork: null,
  },

  setWallet: (wallet) => {
    if (typeof window !== 'undefined') {
      if (wallet.connected && wallet.address) {
        localStorage.setItem(WALLET_KEY, wallet.address);
      } else {
        localStorage.removeItem(WALLET_KEY);
      }
    }
    set({ wallet });
  },
  setPoolConfig: (poolConfig) => set({ poolConfig }),
  setPosition: (position) => set({ position }),
  setRecentEvents: (recentEvents) => set({ recentEvents }),
  setLastPollTime: (lastPollTime) => set({ lastPollTime }),
  setPollingInterval: (pollingInterval) => set({ pollingInterval }),
  setNetworkMismatch: (networkMismatch: {
    isMismatched: boolean;
    walletNetwork: string | null;
    appNetwork: string | null;
  }) => set({ networkMismatch }),
  disconnect: () => {
    if (typeof window !== 'undefined') {
      localStorage.removeItem(WALLET_KEY);
    }
    set({
      wallet: { address: null, connected: false, network: 'testnet' },
      position: null,
      poolConfig: null,
      recentEvents: [],
      networkMismatch: {
        isMismatched: false,
        walletNetwork: null,
        appNetwork: null,
      },
    });
  },
  refreshPosition: async () => {
    // Placeholder for actual on-chain position refresh
    // In production, this would call the pool contract's get_position method
    const { wallet } = get();
    if (!wallet.address) return;

    try {
      // Dynamic import to avoid circular deps
      const { fetchInvestorPosition } = await import('./contracts');
      const position = await fetchInvestorPosition(wallet.address);
      set({ position });
    } catch (error) {
      console.error('[Store] Failed to refresh position:', error);
    }
  },
}));
