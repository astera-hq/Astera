export type InvoiceStatus =
  | 'Pending'
  | 'AwaitingVerification'
  | 'Verified'
  | 'Disputed'
  | 'Funded'
  | 'Paid'
  | 'Defaulted';

export interface InvoiceMetadata {
  name: string;
  description: string;
  image: string;
  amount: bigint;
  debtor: string;
  dueDate: number;
  status: InvoiceStatus;
  symbol: string;
  decimals: number;
}

export interface Invoice {
  id: bigint;
  owner: string;
  debtor: string;
  amount: bigint;
  due_date: number;
  description: string;
  status: InvoiceStatus;
  created_at: number;
  funded_at: number;
  paid_at: number;
  pool_contract: string;
  verification_hash?: string;
  metadata_uri?: string;
  oracle_verified?: boolean;
}

export interface InvestorPosition {
  deposited: bigint;
  available: bigint;
  deployed: bigint;
  earned: bigint;
  depositCount: number;
}

export interface PoolConfig {
  invoiceContract: string;
  admin: string;
  yieldBps: number;
  factoringFeeBps: number;
  compoundInterest: boolean;
}

export interface PoolTokenTotals {
  totalDeposited: bigint;
  totalDeployed: bigint;
  totalPaidOut: bigint;
  totalFeeRevenue: bigint;
}

export interface FundedInvoice {
  invoiceId: bigint;
  sme: string;
  token: string;
  principal: bigint;
  committed: bigint;
  fundedAt: number;
  factoringFee: bigint;
  dueDate: number;
  repaidAmount: bigint;
}

export interface AsteraConfig {
  rpcUrl: string;
  network: string;
  invoiceContractId: string;
  poolContractId: string;
  creditScoreContractId?: string;
}

export interface TransactionProgress {
  status: 'pending' | 'confirmed' | 'failed';
  hash: string;
  error?: string;
}
