import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

const mockTrackedSubmit = jest.fn();
const mockBuildListPositionTx = jest.fn();
const mockBuildCommitToInvoiceTx = jest.fn();
const mockBuildSetPremiumConfigTx = jest.fn();
const mockSubmitTx = jest.fn();

jest.mock('@/lib/store', () => ({
  useStore: jest.fn(() => ({
    wallet: {
      address: 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
      connected: true,
    },
  })),
}));
jest.mock('@/hooks/useTrackTransaction', () => ({
  UseTrackTransaction: jest.fn(() => mockTrackedSubmit),
}));
jest.mock('@stellar/freighter-api', () => ({
  signTransaction: jest.fn(async () => ({ signedTxXdr: 'signed-xdr', error: null })),
}));
jest.mock('swr', () => ({
  __esModule: true,
  default: jest.fn((key: unknown) => {
    if (key === 'insurance-admin-tokens') return { data: ['CTOKEN'], mutate: jest.fn() };
    if (key === 'insurance-admin-premium-config')
      return {
        data: {
          base_rate_bps: 200,
          tenor_bps_per_day: 2,
          risk_tiers: [],
          default_risk_multiplier_bps: 25000,
          min_premium_bps: 50,
          max_premium_bps: 2000,
          default_coverage_bps: 8000,
        },
        mutate: jest.fn(),
      };
    if (key === 'insurance-admin-pool-link') return { data: null, mutate: jest.fn() };
    if (Array.isArray(key) && key[0] === 'insurance-admin-token-data')
      return {
        data: {
          reserve: {
            total_reserves: 10000000n,
            total_covered_exposure: 0n,
            total_premiums_collected: 0n,
            total_claims_paid: 0n,
            coverage_ratio_bps: 10000,
            min_coverage_ratio_bps: 8000,
          },
          health: { is_healthy: true },
          minReserve: 0n,
        },
        mutate: jest.fn(),
      };
    return { data: undefined, mutate: jest.fn() };
  }),
  mutate: jest.fn(),
}));
jest.mock('@/lib/cache', () => ({
  useOpenListings: jest.fn(() => ({ data: [], isLoading: false })),
  useMyListings: jest.fn(() => ({ data: [], isLoading: false })),
  useOrderBook: jest.fn(() => ({ data: null, isLoading: false })),
  useMyOrders: jest.fn(() => ({ data: [], isLoading: false })),
  useCoFundingRounds: jest.fn(() => ({
    data: [
      {
        invoiceId: 7,
        token: 'CTOKEN',
        targetPrincipal: 100000000n,
        committedPrincipal: 10000000n,
        fundingDeadline: 2000000000,
        status: 'Open',
      },
    ],
    isLoading: false,
  })),
  useCoFundingPositions: jest.fn(() => ({ data: [], isLoading: false })),
}));
jest.mock('@/lib/contracts', () => ({
  buildListPositionTx: mockBuildListPositionTx,
  buildCancelListingTx: jest.fn(),
  buildBuyListingTx: jest.fn(),
  buildPlaceOrderTx: jest.fn(),
  buildCancelOrderTx: jest.fn(),
  buildExpireOrderTx: jest.fn(),
  buildCommitToInvoiceTx: mockBuildCommitToInvoiceTx,
  buildWithdrawCoFundingCommitmentTx: jest.fn(),
  buildTransferCoFundShareTx: jest.fn(),
  getAcceptedTokens: jest.fn(),
  getPremiumConfig: jest.fn(),
  getReserveStatus: jest.fn(),
  checkReserveHealth: jest.fn(),
  getMinReserveAmount: jest.fn(),
  getInsuranceContractLink: jest.fn(),
  buildSetPremiumConfigTx: mockBuildSetPremiumConfigTx,
  buildSetMinCoverageRatioTx: jest.fn(),
  buildSetMinReserveAmountTx: jest.fn(),
  buildFundReserveFromTreasuryTx: jest.fn(),
  buildSetInsuranceContractTx: jest.fn(),
}));
jest.mock('@/lib/stellar', () => {
  const actual = jest.requireActual('@/lib/stellar');
  return { ...actual, submitTx: mockSubmitTx, INSURANCE_CONTRACT_ID: 'CINSURANCE' };
});

describe('transaction-heavy pages', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockBuildListPositionTx.mockResolvedValue('listing-xdr');
    mockBuildCommitToInvoiceTx.mockResolvedValue('commit-xdr');
    mockBuildSetPremiumConfigTx.mockResolvedValue('premium-xdr');
    mockTrackedSubmit.mockResolvedValue(undefined);
    mockSubmitTx.mockResolvedValue(undefined);
  });

  it('builds, signs, and submits a secondary-market listing', async () => {
    const { default: Page } = await import('@/app/invest/secondary-market/page');
    render(<Page />);
    const submit = screen.getByRole('button', { name: 'List Position' });
    const inputs = within(submit.closest('form')!).getAllByRole('spinbutton');
    await userEvent.type(inputs[0], '42');
    await userEvent.type(inputs[1], '25');
    await userEvent.type(inputs[2], '30');
    await userEvent.click(submit);
    await waitFor(() =>
      expect(mockBuildListPositionTx).toHaveBeenCalledWith(
        expect.objectContaining({ invoiceId: 42, amountOrBps: 250000000n, price: 300000000n }),
      ),
    );
    await waitFor(() => expect(mockTrackedSubmit).toHaveBeenCalledWith('signed-xdr'));
  });

  it('builds, signs, and submits a co-funding commitment', async () => {
    const { default: Page } = await import('@/app/invest/co-funding/page');
    render(<Page />);
    await userEvent.type(screen.getByPlaceholderText('Amount (USDC)'), '12.5');
    await userEvent.click(screen.getByRole('button', { name: 'Commit' }));
    await waitFor(() =>
      expect(mockBuildCommitToInvoiceTx).toHaveBeenCalledWith(
        expect.objectContaining({ invoiceId: 7, amount: 125000000n }),
      ),
    );
    await waitFor(() => expect(mockTrackedSubmit).toHaveBeenCalledWith('signed-xdr'));
  });

  it('builds, signs, and submits an insurance premium configuration update', async () => {
    const { default: Page } = await import('@/app/admin/insurance/page');
    render(<Page />);
    await userEvent.click(screen.getByRole('button', { name: 'Edit' }));
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() =>
      expect(mockBuildSetPremiumConfigTx).toHaveBeenCalledWith(
        'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
        expect.objectContaining({ base_rate_bps: 200 }),
      ),
    );
    await waitFor(() => expect(mockSubmitTx).toHaveBeenCalledWith('signed-xdr'));
  });
});
