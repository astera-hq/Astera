import type { Invoice, InvoiceMetadata } from '@/lib/types';

const toBlobMock = jest
  .fn()
  .mockResolvedValue(new Blob(['%PDF-1.4 fake'], { type: 'application/pdf' }));
const pdfMock = jest.fn((_doc: unknown) => ({ toBlob: toBlobMock }));

jest.mock('@react-pdf/renderer', () => ({
  Document: 'mock-document',
  Page: 'mock-page',
  Text: 'mock-text',
  View: 'mock-view',
  Image: 'mock-image',
  StyleSheet: { create: (styles: unknown) => styles },
  pdf: (doc: unknown) => pdfMock(doc),
}));

jest.mock('qrcode', () => ({
  __esModule: true,
  default: {
    toDataURL: jest.fn().mockResolvedValue('data:image/png;base64,mockqrcode'),
  },
}));

import { downloadInvoicePDF } from '@/components/InvoicePDF';

const invoice: Invoice = {
  id: 42,
  owner: 'GABC1234567890OWNER',
  debtor: 'Acme Co',
  amount: 1_000_000_000n,
  dueDate: 1_700_000_000,
  description: 'Consulting services',
  status: 'Funded',
  createdAt: 1_690_000_000,
  fundedAt: 1_691_000_000,
  paidAt: 0,
  poolContract: 'CPOOLCONTRACTID',
  verificationHash: 'deadbeefcafe',
};

const metadata: InvoiceMetadata = {
  name: 'Invoice #42',
  description: 'Consulting services',
  image: '',
  amount: 1_000_000_000n,
  debtor: 'Acme Co',
  dueDate: 1_700_000_000,
  status: 'Funded',
  symbol: 'INV',
  decimals: 7,
};

describe('downloadInvoicePDF', () => {
  // jsdom doesn't implement createObjectURL/revokeObjectURL at all, so they
  // must be assigned directly rather than spied on.
  const createObjectURLMock = jest.fn().mockReturnValue('blob:mock-url');
  const revokeObjectURLMock = jest.fn();
  let clickSpy: jest.SpyInstance;

  beforeAll(() => {
    (URL as unknown as { createObjectURL: typeof createObjectURLMock }).createObjectURL =
      createObjectURLMock;
    (URL as unknown as { revokeObjectURL: typeof revokeObjectURLMock }).revokeObjectURL =
      revokeObjectURLMock;
  });

  beforeEach(() => {
    jest.clearAllMocks();
    global.fetch = jest.fn().mockResolvedValue({ ok: false }) as unknown as typeof fetch;
    clickSpy = jest.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
  });

  afterEach(() => {
    clickSpy.mockRestore();
  });

  it('renders the PDF and downloads it with the astera-invoice-{id}-{date} filename', async () => {
    await downloadInvoicePDF(invoice, metadata);

    expect(pdfMock).toHaveBeenCalledTimes(1);
    expect(toBlobMock).toHaveBeenCalledTimes(1);
    expect(createObjectURLMock).toHaveBeenCalledTimes(1);
    expect(clickSpy).toHaveBeenCalledTimes(1);
    expect(revokeObjectURLMock).toHaveBeenCalledWith('blob:mock-url');

    const todayStr = new Date().toISOString().slice(0, 10);
    // The anchor's download attribute is set right before click(); recover it
    // from the spy's `this` context.
    const anchor = clickSpy.mock.contexts[0] as HTMLAnchorElement;
    expect(anchor.download).toBe(`astera-invoice-42-${todayStr}.pdf`);
    expect(anchor.href).toContain('blob:mock-url');
  });
});
