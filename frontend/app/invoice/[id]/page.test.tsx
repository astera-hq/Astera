import { render, screen, waitFor } from '@testing-library/react';
import InvoiceDetailPage from './page';
import { useParams } from 'next/navigation';
import { useStore } from '@/lib/store';
import { sdk } from '@/lib/sdk';
import {
  getInvoice,
  getInvoiceMetadata,
  getPoolConfig,
  getFundedInvoice,
  getCollateralConfig,
  getCollateralDeposit,
  isInvoicePrivate,
  getFullCreditScore,
  getCoFundingRound,
} from '@/lib/contracts';

jest.mock('next/navigation', () => ({
  useParams: jest.fn(),
}));

jest.mock('@/lib/store', () => ({
  useStore: jest.fn(),
}));

jest.mock('@/lib/contracts', () => ({
  getInvoice: jest.fn(),
  getInvoiceMetadata: jest.fn(),
  getPoolConfig: jest.fn(),
  getFundedInvoice: jest.fn(),
  buildRepayTx: jest.fn(),
  buildDisputeTx: jest.fn(),
  getCollateralConfig: jest.fn(),
  getCollateralDeposit: jest.fn(),
  getLiveCollateralRatio: jest.fn(),
  getCollateralRiskConfig: jest.fn(),
  getAtRiskSince: jest.fn(),
  getAcceptedTokens: jest.fn(),
  getAssetPrice: jest.fn(),
  buildDepositCollateralTx: jest.fn(),
  buildTopUpCollateralTx: jest.fn(),
  isInvoicePrivate: jest.fn(),
  buildSetInvoicePrivateTx: jest.fn(),
  getFullCreditScore: jest.fn(),
  getCoFundingRound: jest.fn(),
  buildCommitToInvoiceTx: jest.fn(),
  submitTx: jest.fn(),
  buildRaiseDisputeTx: jest.fn(),
  getDispute: jest.fn(),
  getArbitrationCaseByInvoice: jest.fn(),
  getArbitrationEvidence: jest.fn(),
  buildSubmitEvidenceTx: jest.fn(),
}));

// Contract reads/writes with an SDK equivalent now go through @/lib/sdk.
jest.mock('@/lib/sdk', () => ({
  sdkSigner: jest.fn(async () => jest.fn()),
  sdk: {
    invoice: {
      getInvoice: jest.fn().mockResolvedValue(null),
      getMetadata: jest.fn().mockResolvedValue(null),
    },
    pool: {
      getConfig: jest.fn().mockResolvedValue(null),
      getFundedInvoice: jest.fn().mockResolvedValue(null),
      getCollateralConfig: jest.fn().mockResolvedValue(null),
      getCollateralDeposit: jest.fn().mockResolvedValue(null),
      getCoFundingRound: jest.fn().mockResolvedValue(null),
      repay: jest.fn(),
      depositCollateral: jest.fn(),
      topUpCollateral: jest.fn(),
      commitToInvoice: jest.fn(),
    },
    auction: {
      getLiveCollateralRatio: jest.fn().mockResolvedValue(0),
      getCollateralRiskConfig: jest.fn().mockResolvedValue(null),
      getAtRiskSince: jest.fn().mockResolvedValue(null),
    },
    arbitration: {
      getEvidence: jest.fn().mockResolvedValue([]),
      submitEvidence: jest.fn(),
    },
  },
}));

jest.mock('@/lib/stellar', () => ({
  formatAmount: () => '$1.00',
  formatUSDC: () => '$1.00',
  formatDate: () => '2024-01-01',
  daysUntil: () => 10,
  truncateAddress: (value: string) => value,
  rpcGetEvents: jest.fn().mockResolvedValue({ events: [] }),
  rpcGetLatestLedger: jest.fn().mockResolvedValue({ sequence: 1_000_000 }),
  toStroops: () => 1_000_000n,
  INVOICE_CONTRACT_ID: 'CINVOICE',
  POOL_CONTRACT_ID: 'CPOOL',
  USDC_TOKEN_ID: 'CUSD',
  nativeToScVal: (value: unknown) => value,
  scValToNative: (value: unknown) => value,
  xdr: {},
  Address: class {
    constructor(public value: string) {}
    toScVal() {
      return this.value;
    }
  },
}));

jest.mock('@/lib/simulateFee', () => ({
  simulateContractCall: jest.fn(),
}));

jest.mock('@/hooks/useTransactionSimulation', () => ({
  useTransactionSimulation: () => null,
}));

jest.mock('@/components/InvoicePDF', () => ({
  downloadInvoicePDF: jest.fn(),
}));

jest.mock('@/components/GlossaryTerm', () => ({
  __esModule: true,
  default: () => null,
}));

jest.mock('@/components/ConfirmActionModal', () => ({
  __esModule: true,
  default: () => null,
}));

jest.mock('@/components/WalletConnect', () => ({
  __esModule: true,
  default: () => null,
}));

jest.mock('@/components/Skeleton', () => ({
  Skeleton: ({ className }: { className: string }) => <div className={className} />,
}));

jest.mock('@/components/EstimatedFee', () => ({
  __esModule: true,
  default: () => null,
}));

jest.mock('react-hot-toast', () => ({
  __esModule: true,
  default: {
    success: jest.fn(),
    error: jest.fn(),
  },
}));

describe('InvoiceDetailPage private access', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    (useParams as jest.Mock).mockReturnValue({ id: '42' });
    (useStore as unknown as jest.Mock).mockReturnValue({
      wallet: { address: 'GBOWNER', connected: false, network: 'testnet' },
    });
    (isInvoicePrivate as jest.Mock).mockResolvedValue(true);
    (getInvoice as jest.Mock).mockResolvedValue({
      id: 42,
      owner: 'GBOWNER',
      debtor: 'Acme',
      amount: 10_000n,
      dueDate: 1_700_000_000,
      description: 'Invoice',
      status: 'Funded',
      createdAt: 1,
      fundedAt: 1,
      paidAt: 0,
      poolContract: 'CPOOL',
    });
    (getInvoiceMetadata as jest.Mock).mockResolvedValue({
      name: 'Private invoice title',
      description: 'Invoice description',
      image: '',
      amount: 10_000n,
      debtor: 'Acme',
      dueDate: 1_700_000_000,
      status: 'Funded',
      symbol: 'USDC',
      decimals: 6,
    });
    (getPoolConfig as jest.Mock).mockResolvedValue({
      admin: 'GADMIN',
      yieldBps: 0,
      factoringFeeBps: 0,
      compoundInterest: false,
      proposedYieldBps: 0,
      yieldProposalAt: 0,
      yieldTimelockSecs: 0,
      maxSingleInvestorBps: 0,
      maxWithdrawalQueueAgeDays: 0,
      maxWithdrawalQueueDepth: 0,
      invoiceContract: 'CINVOICE',
    });
    (getFundedInvoice as jest.Mock).mockResolvedValue({
      principal: 10_000n,
      dueDate: 1_700_000_000,
      fundedAt: 1,
      factoringFee: 0n,
      repaidAmount: 0n,
    });
    (getCollateralConfig as jest.Mock).mockResolvedValue(null);
    (getCollateralDeposit as jest.Mock).mockResolvedValue(null);
    (getFullCreditScore as jest.Mock).mockResolvedValue(null);
    (getCoFundingRound as jest.Mock).mockResolvedValue(null);
    // SDK-backed reads mirror the legacy mocks so the private-access guard is
    // the only thing under test here.
    (sdk.invoice.getInvoice as jest.Mock).mockResolvedValue({
      id: 42,
      owner: 'GBOWNER',
      debtor: 'Acme',
      amount: 10_000n,
      dueDate: 1_700_000_000,
      description: 'Invoice',
      status: 'Funded',
      createdAt: 1,
      fundedAt: 1,
      paidAt: 0,
      poolContract: 'CPOOL',
    });
    (sdk.invoice.getMetadata as jest.Mock).mockResolvedValue({
      name: 'Private invoice title',
      description: 'Invoice description',
      image: '',
      amount: 10_000n,
      debtor: 'Acme',
      dueDate: 1_700_000_000,
      status: 'Funded',
      symbol: 'USDC',
      decimals: 6,
    });
  });

  it('blocks private invoices for non-owners before rendering invoice content', async () => {
    render(<InvoiceDetailPage />);

    await waitFor(() => {
      expect(screen.getByText('Invoice not found.')).toBeInTheDocument();
    });

    expect(screen.queryByText('Private invoice title')).not.toBeInTheDocument();
    expect(isInvoicePrivate).toHaveBeenCalledWith(42);
  });
});
