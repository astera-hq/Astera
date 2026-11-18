import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import AdminExchangeRatesPage from './page';

const mockGetAcceptedTokens = jest.fn();
const mockGetExchangeRate = jest.fn();
const mockBuildSetExchangeRateTx = jest.fn();
const mockSubmitTx = jest.fn();
const mockSignTransaction = jest.fn();

jest.mock('@/lib/contracts', () => ({
  getAcceptedTokens: (...args: unknown[]) => mockGetAcceptedTokens(...args),
  getExchangeRate: (...args: unknown[]) => mockGetExchangeRate(...args),
  buildSetExchangeRateTx: (...args: unknown[]) => mockBuildSetExchangeRateTx(...args),
  submitTx: (...args: unknown[]) => mockSubmitTx(...args),
}));

jest.mock('@/lib/store', () => ({
  useStore: () => ({
    wallet: { address: 'GABC123', connected: true, network: 'testnet' },
  }),
}));

jest.mock('@stellar/freighter-api', () => ({
  signTransaction: (...args: unknown[]) => mockSignTransaction(...args),
}));

jest.mock('react-hot-toast', () => ({
  __esModule: true,
  default: {
    success: jest.fn(),
    error: jest.fn(),
  },
}));

jest.mock('@/components/Skeleton', () => ({
  Skeleton: ({ className }: { className: string }) => (
    <div data-testid="skeleton" className={className} />
  ),
}));

describe('AdminExchangeRatesPage', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockGetAcceptedTokens.mockResolvedValue(['USDC', 'EURC']);
    mockGetExchangeRate.mockImplementation((token: string) =>
      Promise.resolve(token === 'USDC' ? 10_000 : 10_800),
    );
    mockBuildSetExchangeRateTx.mockResolvedValue('xdr');
    mockSubmitTx.mockResolvedValue(undefined);
    mockSignTransaction.mockResolvedValue({ signedTxXdr: 'signed-xdr', error: null });
  });

  it('requires confirmation before submitting the exchange rate transaction', async () => {
    const user = userEvent.setup();

    render(<AdminExchangeRatesPage />);

    // The next-intl mock (__mocks__/next-intl.js) returns translation keys
    // verbatim (and ignores interpolation params), so assertions match the
    // raw key strings rather than the rendered English copy.
    await waitFor(() => expect(screen.getByText('currentRates')).toBeInTheDocument());

    await user.type(screen.getByPlaceholderText('ratePlaceholder'), '108');
    await user.click(screen.getByRole('button', { name: 'setRate' }));

    expect(screen.getByText('reviewTitle')).toBeInTheDocument();
    expect(mockBuildSetExchangeRateTx).not.toHaveBeenCalled();
    expect(mockSubmitTx).not.toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: 'confirmSubmit' }));

    await waitFor(() =>
      expect(mockBuildSetExchangeRateTx).toHaveBeenCalledWith('GABC123', 'USDC', 10_800),
    );
    expect(mockSubmitTx).toHaveBeenCalledWith('signed-xdr');
  });
});
