import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

jest.mock('@/lib/store', () => ({
  useStore: jest.fn(() => ({
    wallet: { address: 'GADMINTEST', connected: true, network: 'testnet' },
  })),
}));

const mockGetInvoiceCount = jest.fn();
const mockGetMultipleInvoices = jest.fn();
const mockGetPoolTokenTotals = jest.fn();
const mockBuildInitCoFundingTx = jest.fn();
const mockSubmitTx = jest.fn();

jest.mock('@/lib/contracts', () => ({
  getInvoiceCount: mockGetInvoiceCount,
  getMultipleInvoices: mockGetMultipleInvoices,
  getPoolTokenTotals: mockGetPoolTokenTotals,
  buildInitCoFundingTx: mockBuildInitCoFundingTx,
  submitTx: mockSubmitTx,
}));

jest.mock('@stellar/freighter-api', () => ({
  signTransaction: jest.fn(async () => ({ signedTxXdr: 'signedXdr', error: null })),
}));

const invoice = {
  id: 123,
  owner: 'GSMEOWNER',
  debtor: 'Acme Corp',
  amount: 100000000n,
  dueDate: 1950000000,
  description: 'Test invoice',
  status: 'Verified',
  createdAt: 0,
  fundedAt: 0,
  paidAt: 0,
  poolContract: 'GUSDC123',
};

describe('AdminInvoicesPage batch funding', () => {
  const originalToken = process.env.NEXT_PUBLIC_USDC_TOKEN_ID;

  beforeEach(() => {
    jest.resetAllMocks();
    mockGetInvoiceCount.mockResolvedValue(1);
    mockGetMultipleInvoices.mockResolvedValue([invoice]);
    mockGetPoolTokenTotals.mockResolvedValue({
      totalDeposited: 200000000n,
      totalDeployed: 0n,
      totalPaidOut: 0n,
      totalFeeRevenue: 0n,
    });
    mockBuildInitCoFundingTx.mockResolvedValue('test-xdr');
    mockSubmitTx.mockResolvedValue({});
    process.env.NEXT_PUBLIC_USDC_TOKEN_ID = 'GUSDC123';
  });

  afterEach(() => {
    process.env.NEXT_PUBLIC_USDC_TOKEN_ID = originalToken;
  });

  it('selects a verified invoice and starts the batch funding flow', async () => {
    const { default: AdminInvoicesPage } = await import('@/app/admin/invoices/page');

    render(<AdminInvoicesPage />);

    expect(await screen.findByText('Verified Invoices Queue')).toBeInTheDocument();
    expect(await screen.findByText('#123')).toBeInTheDocument();

    const invoiceCheckbox = screen.getByRole('checkbox', { name: /Select invoice 123/i });
    await userEvent.click(invoiceCheckbox);

    const batchButton = await screen.findByRole('button', { name: /Fund Selected \(1\)/i });
    expect(batchButton).toBeEnabled();

    await userEvent.click(batchButton);
    expect(await screen.findByText('Fund 1 selected invoices')).toBeInTheDocument();

    const confirmButton = screen.getByRole('button', { name: /Fund selected/i });
    await userEvent.click(confirmButton);

    await waitFor(() => {
      expect(mockBuildInitCoFundingTx).toHaveBeenCalledWith({
        admin: 'GADMINTEST',
        invoiceId: 123,
        principal: 100000000n,
        sme: 'GSMEOWNER',
        dueDate: 1950000000,
        token: 'GUSDC123',
      });
    });

    await waitFor(() => expect(mockSubmitTx).toHaveBeenCalledWith('signedXdr'));
  });
});
