/**
 * @jest-environment jsdom
 */
import React from 'react';
import { render, screen } from '@testing-library/react';
import NewInvoicePage from '../page';

jest.mock('next/navigation', () => ({
  useRouter: () => ({ push: jest.fn() }),
  useSearchParams: () => new URLSearchParams(),
}));

jest.mock('@/lib/store', () => ({
  useStore: () => ({
    wallet: {
      connected: true,
      address: 'GBRPYHIL2CI3FNQ4BXLFMNDLFJUNPU2HY3ZMFXYCZTM6WPIXY6OROLET',
    },
  }),
}));

jest.mock('@/lib/contracts', () => ({
  getMaxInvoiceAmount: jest.fn().mockResolvedValue(1000000),
  buildCreateInvoiceTx: jest.fn(),
  submitTx: jest.fn(),
}));

jest.mock('@/lib/stellar', () => ({
  toStroops: jest.fn((v: number) => BigInt(Math.round(v * 10_000_000))),
  nativeToScVal: jest.fn(),
  Address: jest.fn().mockImplementation(() => ({ toScVal: jest.fn() })),
  xdr: { ScVal: { scvVoid: jest.fn() } },
  INVOICE_CONTRACT_ID: 'CTEST',
}));

jest.mock('@/lib/simulateFee', () => ({
  simulateContractCall: jest.fn(),
}));

jest.mock('@/hooks/useTransactionSimulation', () => ({
  useTransactionSimulation: jest.fn(() => ({ status: 'idle', fee: null, error: null })),
}));

jest.mock('@/components/EstimatedFee', () => {
  return function MockEstimatedFee() {
    return <div data-testid="estimated-fee" />;
  };
});

jest.mock('@/components/GlossaryTerm', () => {
  return function MockGlossaryTerm({ children }: { children?: React.ReactNode }) {
    return <span>{children}</span>;
  };
});

jest.mock('@/lib/invoiceTemplates', () => ({
  getInvoiceTemplate: jest.fn(),
  upsertInvoiceTemplate: jest.fn(),
}));

jest.mock('react-hot-toast', () => ({
  __esModule: true,
  default: { success: jest.fn(), error: jest.fn() },
}));

describe('NewInvoicePage Accessibility (#970)', () => {
  it('associates all form inputs with explicit label elements', () => {
    render(<NewInvoicePage />);

    expect(screen.getByLabelText(/debtor/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/invoice amount/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/due date/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/description/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/document metadata uri/i)).toBeInTheDocument();
  });
});
