type EnvConfig = {
  NEXT_PUBLIC_INVOICE_CONTRACT_ID: string;
  NEXT_PUBLIC_POOL_CONTRACT_ID: string;
  NEXT_PUBLIC_USDC_TOKEN_ID: string;
  NEXT_PUBLIC_EURC_TOKEN_ID?: string;
  NEXT_PUBLIC_NETWORK: 'testnet' | 'mainnet' | 'standalone';
  NEXT_PUBLIC_HORIZON_URL?: string;
  NEXT_PUBLIC_SOROBAN_RPC_URL?: string;
};

type EnvValidationError = {
  variable: string;
  message: string;
};

const REQUIRED_VARS = [
  'NEXT_PUBLIC_INVOICE_CONTRACT_ID',
  'NEXT_PUBLIC_POOL_CONTRACT_ID',
  'NEXT_PUBLIC_USDC_TOKEN_ID',
] as const;

const NETWORK_CONFIGS = {
  testnet: {
    horizonUrl: 'https://horizon-testnet.stellar.org',
    sorobanRpcUrl: 'https://soroban-testnet.stellar.org',
    networkPassphrase: 'Test SDF Network ; September 2015',
  },
  mainnet: {
    horizonUrl: 'https://horizon.stellar.org',
    sorobanRpcUrl: 'https://soroban.stellar.org',
    networkPassphrase: 'Public Global Stellar Network ; September 2015',
  },
  standalone: {
    horizonUrl: 'http://localhost:8000',
    sorobanRpcUrl: 'http://localhost:8000/soroban/rpc',
    networkPassphrase: 'Standalone Network ; February 2017',
  },
} as const;

function validateContractId(value: string, name: string): EnvValidationError | null {
  if (!value || value.trim() === '') {
    return { variable: name, message: 'is required' };
  }
  if (value.length !== 56 || !value.startsWith('C')) {
    return {
      variable: name,
      message: 'must be a valid Stellar contract ID (56 characters starting with C)',
    };
  }
  return null;
}

function validateNetwork(value: string): EnvValidationError | null {
  const validNetworks = ['testnet', 'mainnet', 'standalone'];
  if (value && !validNetworks.includes(value)) {
    return {
      variable: 'NEXT_PUBLIC_NETWORK',
      message: `must be one of: ${validNetworks.join(', ')}`,
    };
  }
  return null;
}

function validateUrl(value: string | undefined, name: string): EnvValidationError | null {
  if (!value) return null;
  try {
    new URL(value);
    return null;
  } catch {
    return { variable: name, message: 'must be a valid URL' };
  }
}

export function validateEnv(): { valid: boolean; errors: EnvValidationError[] } {
  const errors: EnvValidationError[] = [];

  const invoiceId = process.env.NEXT_PUBLIC_INVOICE_CONTRACT_ID || '';
  const poolId = process.env.NEXT_PUBLIC_POOL_CONTRACT_ID || '';
  const usdcId = process.env.NEXT_PUBLIC_USDC_TOKEN_ID || '';
  const eurcId = process.env.NEXT_PUBLIC_EURC_TOKEN_ID;
  const network = process.env.NEXT_PUBLIC_NETWORK || 'testnet';
  const horizonUrl = process.env.NEXT_PUBLIC_HORIZON_URL;
  const sorobanUrl = process.env.NEXT_PUBLIC_SOROBAN_RPC_URL;

  const invoiceError = validateContractId(invoiceId, 'NEXT_PUBLIC_INVOICE_CONTRACT_ID');
  if (invoiceError) errors.push(invoiceError);

  const poolError = validateContractId(poolId, 'NEXT_PUBLIC_POOL_CONTRACT_ID');
  if (poolError) errors.push(poolError);

  const usdcError = validateContractId(usdcId, 'NEXT_PUBLIC_USDC_TOKEN_ID');
  if (usdcError) errors.push(usdcError);

  if (eurcId) {
    const eurcError = validateContractId(eurcId, 'NEXT_PUBLIC_EURC_TOKEN_ID');
    if (eurcError) errors.push(eurcError);
  }

  const networkError = validateNetwork(network);
  if (networkError) errors.push(networkError);

  const horizonError = validateUrl(horizonUrl, 'NEXT_PUBLIC_HORIZON_URL');
  if (horizonError) errors.push(horizonError);

  const sorobanError = validateUrl(sorobanUrl, 'NEXT_PUBLIC_SOROBAN_RPC_URL');
  if (sorobanError) errors.push(sorobanError);

  return { valid: errors.length === 0, errors };
}

export function getEnvConfig(): EnvConfig {
  const network = (process.env.NEXT_PUBLIC_NETWORK ||
    'testnet') as EnvConfig['NEXT_PUBLIC_NETWORK'];

  return {
    NEXT_PUBLIC_INVOICE_CONTRACT_ID: process.env.NEXT_PUBLIC_INVOICE_CONTRACT_ID || '',
    NEXT_PUBLIC_POOL_CONTRACT_ID: process.env.NEXT_PUBLIC_POOL_CONTRACT_ID || '',
    NEXT_PUBLIC_USDC_TOKEN_ID: process.env.NEXT_PUBLIC_USDC_TOKEN_ID || '',
    NEXT_PUBLIC_EURC_TOKEN_ID: process.env.NEXT_PUBLIC_EURC_TOKEN_ID,
    NEXT_PUBLIC_NETWORK: network,
    NEXT_PUBLIC_HORIZON_URL: process.env.NEXT_PUBLIC_HORIZON_URL,
    NEXT_PUBLIC_SOROBAN_RPC_URL: process.env.NEXT_PUBLIC_SOROBAN_RPC_URL,
  };
}

export function getNetworkConfig(network?: EnvConfig['NEXT_PUBLIC_NETWORK']) {
  const targetNetwork = network || getEnvConfig().NEXT_PUBLIC_NETWORK;
  const config = NETWORK_CONFIGS[targetNetwork];
  const envConfig = getEnvConfig();

  return {
    ...config,
    horizonUrl: envConfig.NEXT_PUBLIC_HORIZON_URL || config.horizonUrl,
    sorobanRpcUrl: envConfig.NEXT_PUBLIC_SOROBAN_RPC_URL || config.sorobanRpcUrl,
  };
}

export function formatEnvErrors(errors: EnvValidationError[]): string {
  return errors.map((e) => `  - ${e.variable}: ${e.message}`).join('\n');
}

export function assertEnvValid(): void {
  const { valid, errors } = validateEnv();
  if (!valid) {
    const errorMessage = `Environment configuration errors:\n${formatEnvErrors(errors)}`;
    console.error(errorMessage);
    // CI/build environments (especially for PRs from forks) typically don't have
    // deployed contract IDs. Only hard-fail when explicitly requested.
    const strict = process.env.NEXT_PUBLIC_STRICT_ENV === 'true';
    if (strict && typeof window === 'undefined') {
      throw new Error(errorMessage);
    }
  }
}
