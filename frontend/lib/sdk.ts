/**
 * Configured `@astera/sdk` clients and the single import surface for contract
 * interactions in the frontend.
 *
 * SDK clients do build → simulate → assemble → sign → submit themselves; write
 * methods take a `Signer` callback. `sdkSigner()` adapts Freighter to that
 * contract so call sites can do:
 *
 *   await sdk.pool.deposit({ signer: await sdkSigner(wallet.address), investor, token, amount, minRate });
 *
 * Read methods are simulation-only and need no signer. The read helpers remain
 * in `@/lib/contracts` (where they adapt the SDK's snake_case wire types to the
 * frontend's camelCase shapes) and are re-exported here so call sites import
 * everything through this one module.
 *
 * Clients are instantiated lazily (on first access) so tests that mock
 * `@/lib/stellar` constants observe the mocked values.
 */
import {
  AccessControlClient,
  ArbitrationClient,
  AuctionClient,
  ComplianceClient,
  CreditScoreClient,
  GovernanceClient,
  InvoiceClient,
  InsuranceClient,
  OracleRegistryClient,
  PoolClient,
  ReferralClient,
  SecondaryMarketClient,
  ShareClient,
  TrancheClient,
  type Signer,
} from '@astera/sdk';
import {
  ACCESS_CONTROL_CONTRACT_ID,
  ARBITRATION_CONTRACT_ID,
  AUCTION_CONTRACT_ID,
  COMPLIANCE_CONTRACT_ID,
  CREDIT_SCORE_CONTRACT_ID,
  GOVERNANCE_CONTRACT_ID,
  INVOICE_CONTRACT_ID,
  INSURANCE_CONTRACT_ID,
  NETWORK,
  ORACLE_REGISTRY_CONTRACT_ID,
  POOL_CONTRACT_ID,
  REFERRAL_CONTRACT_ID,
  RPC_URL,
  SECONDARY_MARKET_CONTRACT_ID,
  SHARE_TOKEN_ID,
  TRANCHE_CONTRACT_ID,
} from '@/lib/stellar';

export type { Signer };

/** Build an SDK `Signer` that signs an assembled transaction XDR with Freighter. */
export async function sdkSigner(address: string): Promise<Signer> {
  const freighter = await import('@stellar/freighter-api');
  return async (txXdr: string): Promise<string> => {
    const { signedTxXdr, error } = await freighter.signTransaction(txXdr, {
      networkPassphrase: NETWORK,
      address,
    });
    if (error) {
      throw new Error(typeof error === 'string' ? error : error.message);
    }
    if (!signedTxXdr) {
      throw new Error('Freighter did not return a signed transaction');
    }
    return signedTxXdr;
  };
}

function baseConfig(contractId: string) {
  return { rpcUrl: RPC_URL, network: NETWORK, contractId };
}

// Lazily-cached singletons — one per satellite contract.
let _pool: PoolClient | undefined;
let _invoice: InvoiceClient | undefined;
let _secondaryMarket: SecondaryMarketClient | undefined;
let _creditScore: CreditScoreClient | undefined;
let _oracleRegistry: OracleRegistryClient | undefined;
let _compliance: ComplianceClient | undefined;
let _tranche: TrancheClient | undefined;
let _accessControl: AccessControlClient | undefined;
let _auction: AuctionClient | undefined;
let _insurance: InsuranceClient | undefined;
let _arbitration: ArbitrationClient | undefined;
let _governance: GovernanceClient | undefined;
let _referral: ReferralClient | undefined;
let _share: ShareClient | undefined;

export function getPoolClient(): PoolClient {
  return (_pool ??= new PoolClient(baseConfig(POOL_CONTRACT_ID)));
}
export function getInvoiceClient(): InvoiceClient {
  return (_invoice ??= new InvoiceClient(baseConfig(INVOICE_CONTRACT_ID)));
}
export function getSecondaryMarketClient(): SecondaryMarketClient {
  return (_secondaryMarket ??= new SecondaryMarketClient(baseConfig(SECONDARY_MARKET_CONTRACT_ID)));
}
export function getCreditScoreClient(): CreditScoreClient {
  return (_creditScore ??= new CreditScoreClient(baseConfig(CREDIT_SCORE_CONTRACT_ID)));
}
export function getOracleRegistryClient(): OracleRegistryClient {
  return (_oracleRegistry ??= new OracleRegistryClient(baseConfig(ORACLE_REGISTRY_CONTRACT_ID)));
}
export function getComplianceClient(): ComplianceClient {
  return (_compliance ??= new ComplianceClient(baseConfig(COMPLIANCE_CONTRACT_ID)));
}
export function getTrancheClient(): TrancheClient {
  return (_tranche ??= new TrancheClient(baseConfig(TRANCHE_CONTRACT_ID)));
}
export function getAccessControlClient(): AccessControlClient {
  return (_accessControl ??= new AccessControlClient(baseConfig(ACCESS_CONTROL_CONTRACT_ID)));
}
export function getAuctionClient(): AuctionClient {
  return (_auction ??= new AuctionClient(baseConfig(AUCTION_CONTRACT_ID)));
}
export function getInsuranceClient(): InsuranceClient {
  return (_insurance ??= new InsuranceClient(baseConfig(INSURANCE_CONTRACT_ID)));
}
export function getArbitrationClient(): ArbitrationClient {
  return (_arbitration ??= new ArbitrationClient(baseConfig(ARBITRATION_CONTRACT_ID)));
}
export function getGovernanceClient(): GovernanceClient {
  return (_governance ??= new GovernanceClient(baseConfig(GOVERNANCE_CONTRACT_ID)));
}
export function getReferralClient(): ReferralClient {
  return (_referral ??= new ReferralClient(baseConfig(REFERRAL_CONTRACT_ID)));
}
export function getShareClient(): ShareClient {
  return (_share ??= new ShareClient(baseConfig(SHARE_TOKEN_ID)));
}

/** Grouped client access — `sdk.pool.deposit(...)`, `sdk.invoice.createInvoice(...)`, … */
export const sdk = {
  get pool() {
    return getPoolClient();
  },
  get invoice() {
    return getInvoiceClient();
  },
  get secondaryMarket() {
    return getSecondaryMarketClient();
  },
  get creditScore() {
    return getCreditScoreClient();
  },
  get oracleRegistry() {
    return getOracleRegistryClient();
  },
  get compliance() {
    return getComplianceClient();
  },
  get tranche() {
    return getTrancheClient();
  },
  get accessControl() {
    return getAccessControlClient();
  },
  get auction() {
    return getAuctionClient();
  },
  get insurance() {
    return getInsuranceClient();
  },
  get arbitration() {
    return getArbitrationClient();
  },
  get governance() {
    return getGovernanceClient();
  },
  get referral() {
    return getReferralClient();
  },
  get share() {
    return getShareClient();
  },
};

// ─────────────────────────────────────────────────────────────────────────────
// Read helpers (shape adapters) and pure helpers remain in `@/lib/contracts`.
// Re-exported here so call sites import a single module.
// ─────────────────────────────────────────────────────────────────────────────
// All read helpers, pure helpers and SDK-absent builders stay in `@/lib/contracts`
// and are re-exported here so call sites import a single module.
export * from '@/lib/contracts';
