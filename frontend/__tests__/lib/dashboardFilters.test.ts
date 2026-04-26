import { filterInvoicesByStatuses } from '@/lib/dashboardFilters';

describe('filterInvoicesByStatuses', () => {
  it('filters invoice rows by selected statuses', () => {
    const rows = [
      { invoice: { status: 'Pending' } },
      { invoice: { status: 'Funded' } },
      { invoice: { status: 'Paid' } },
    ] as const;

    const result = filterInvoicesByStatuses(rows, ['Pending', 'Paid']);
    expect(result).toHaveLength(2);
    expect(result.map((row) => row.invoice.status)).toEqual(['Pending', 'Paid']);
  });
});
