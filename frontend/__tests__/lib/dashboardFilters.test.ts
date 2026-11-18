import { filterInvoicesByStatuses } from '@/lib/dashboardFilters';
import type { Invoice } from '@/lib/types';

describe('filterInvoicesByStatuses', () => {
  it('filters invoice rows by selected statuses', () => {
    const rows = [
      { invoice: { status: 'Pending' } },
      { invoice: { status: 'Funded' } },
      { invoice: { status: 'Paid' } },
    ];

    const result = filterInvoicesByStatuses(rows as unknown as { invoice: Invoice }[], [
      'Pending',
      'Paid',
    ]);
    expect(result).toHaveLength(2);
    expect(result.map((row) => row.invoice.status)).toEqual(['Pending', 'Paid']);
  });
});
