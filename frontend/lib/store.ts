import { create } from "zustand";
import type { WalletState, PoolConfig, InvestorPosition } from "./types";

interface AsteraStore {
  wallet: WalletState;
  poolConfig: PoolConfig | null;
  position: InvestorPosition | null;

  setWallet: (wallet: WalletState) => void;
  setPoolConfig: (config: PoolConfig) => void;
  setPosition: (position: InvestorPosition | null) => void;
  disconnect: () => void;
}

export const useStore = create<AsteraStore>((set) => ({
  wallet: { address: null, connected: false, network: "testnet" },
  poolConfig: null,
  position: null,

  setWallet: (wallet) => set({ wallet }),
  setPoolConfig: (poolConfig) => set({ poolConfig }),
  setPosition: (position) => set({ position }),
  disconnect: () =>
    set({ wallet: { address: null, connected: false, network: "testnet" }, position: null }),
}));
