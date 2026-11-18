import { Networks } from '@stellar/stellar-sdk';

const NETWORK_LABELS: Record<string, string> = {
  PUBLIC: 'Mainnet',
  mainnet: 'Mainnet',
  [Networks.PUBLIC]: 'Mainnet',
  TESTNET: 'Testnet',
  testnet: 'Testnet',
  [Networks.TESTNET]: 'Testnet',
  STANDALONE: 'Local',
  standalone: 'Local',
  local: 'Local',
  [Networks.STANDALONE]: 'Local',
};

export function formatNetworkName(network: string | null | undefined): string {
  const value = network?.trim();
  if (!value) return 'Unknown';

  const knownLabel = NETWORK_LABELS[value] ?? NETWORK_LABELS[value.toLowerCase()];
  if (knownLabel) return knownLabel;

  if (value.length <= 24) return value;

  return `${value.slice(0, 12)}...${value.slice(-8)}`;
}
