import type { Invoice } from '@/lib/types';

export function filterInvoicesByStatuses<T extends { invoice: Invoice }>(
  rows: T[],
  statuses: Invoice['status'][],
): T[] {
  if (statuses.length === 0) return rows;
  return rows.filter((row) => statuses.includes(row.invoice.status));
}
