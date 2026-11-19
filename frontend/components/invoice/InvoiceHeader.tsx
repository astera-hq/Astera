'use client';

import { formatAmount, formatDate, truncateAddress } from '@/lib/stellar';
import BorrowerCreditBadge from '@/components/BorrowerCreditBadge';
import type { Invoice, InvoiceMetadata } from '@/lib/types';

/** Main invoice detail card: image, title, status, amount and key metadata. */
interface InvoiceHeaderProps {
  invoice: Invoice;
  metadata: InvoiceMetadata;
  days: number;
  isOwner: boolean;
}

export default function InvoiceHeader({ invoice, metadata, days, isOwner }: InvoiceHeaderProps) {
  return (
    <div className="p-6 bg-brand-card border border-brand-border rounded-2xl mb-6">
      {metadata.image ? (
        <div className="mb-6 rounded-xl overflow-hidden border border-brand-border bg-brand-dark">
          <img src={metadata.image} alt="" className="w-full h-40 object-cover" />
        </div>
      ) : null}
      <div className="flex items-start justify-between mb-6 gap-4">
        <div className="min-w-0">
          <p className="text-xs text-brand-muted mb-1">
            {metadata.symbol} · Invoice #{invoice.id}
          </p>
          <h1 className="text-2xl font-bold">{metadata.name}</h1>
          <p className="text-brand-muted mt-1">{metadata.debtor}</p>
        </div>
        <span
          className={`text-sm font-medium px-3 py-1.5 rounded-full flex-shrink-0 badge-${metadata.status.toLowerCase()}`}
        >
          {metadata.status}
        </span>
      </div>

      <div className="text-4xl font-bold gradient-text mb-6">
        {formatAmount(metadata.amount, metadata.decimals)}
      </div>

      <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 text-sm">
        <div>
          <p className="text-brand-muted mb-1">Due Date</p>
          <p className="font-medium">{formatDate(metadata.dueDate)}</p>
        </div>
        <div>
          <p className="text-brand-muted mb-1">Time Remaining</p>
          <p
            className={`font-medium ${
              days < 0 ? 'text-red-400' : days <= 7 ? 'text-yellow-400' : 'text-white'
            }`}
          >
            {days < 0 ? `${Math.abs(days)} days overdue` : `${days} days`}
          </p>
        </div>
        <div className="col-span-2">
          <p className="text-brand-muted mb-1">Owner</p>
          {/* #775: only the owner sees their own full address; every other
              viewer (the public sharing case) only sees it abbreviated. */}
          <p className="font-mono text-xs text-white break-all">
            {isOwner ? invoice.owner : truncateAddress(invoice.owner)}
          </p>
        </div>
        <div className="col-span-2">
          <BorrowerCreditBadge borrower={invoice.owner} />
        </div>
        {metadata.description && (
          <div className="col-span-2">
            <p className="text-brand-muted mb-1">Description</p>
            <p className="text-sm">{metadata.description}</p>
          </div>
        )}
      </div>
    </div>
  );
}
