declare module '@stellar/freighter-api' {
  export type FreighterError = { message?: string } | null | undefined;

  export function isConnected(): Promise<{ isConnected: boolean; error?: FreighterError }>;
  export function isAllowed(): Promise<{ isAllowed: boolean; error?: FreighterError }>;
  export function setAllowed(): Promise<unknown>;

  export function getAddress(): Promise<{ address?: string; error?: FreighterError }>;
  export function getNetwork(): Promise<{ network: string }>;

  export function signTransaction(
    xdr: string,
    opts: { networkPassphrase: string; address: string },
  ): Promise<{ signedTxXdr: string; error?: FreighterError }>;
}
