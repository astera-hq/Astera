'use client';

import Link from 'next/link';
import { useEffect, useState } from 'react';
import {
  deleteInvoiceTemplate,
  loadInvoiceTemplates,
  type InvoiceTemplate,
} from '@/lib/invoiceTemplates';

export default function TemplatesPage() {
  const [templates, setTemplates] = useState<InvoiceTemplate[]>([]);
  useEffect(() => setTemplates(loadInvoiceTemplates()), []);
  const remove = (id: string) => {
    if (typeof window !== 'undefined' && window.confirm('Are you sure you want to delete this invoice template?')) {
      deleteInvoiceTemplate(id);
      setTemplates(loadInvoiceTemplates());
    }
  };
  return (
    <main className="min-h-screen px-6 pb-16 pt-24">
      <div className="mx-auto max-w-3xl">
        <div className="mb-8 flex items-center justify-between">
          <div>
            <h1 className="text-3xl font-bold">Invoice templates</h1>
            <p className="text-brand-muted">Reuse your common invoice configurations.</p>
          </div>
          <Link
            className="rounded-xl bg-brand-gold px-4 py-2 font-medium text-brand-dark"
            href="/invoice/new"
          >
            New invoice
          </Link>
        </div>
        {templates.length === 0 ? (
          <div className="rounded-2xl border border-brand-border bg-brand-card p-10 text-center text-brand-muted">
            No templates yet. Save one while creating an invoice.
          </div>
        ) : (
          <div className="space-y-3">
            {templates.map((template) => (
              <article
                key={template.id}
                className="flex items-center justify-between rounded-2xl border border-brand-border bg-brand-card p-5"
              >
                <div>
                  <h2 className="font-semibold">{template.name}</h2>
                  <p className="text-sm text-brand-muted">
                    ${template.amount.toLocaleString()} {template.token} · due in {template.dueDays}{' '}
                    days
                  </p>
                  <p className="text-sm text-brand-muted">
                    {template.description || 'No description'}
                  </p>
                </div>
                <div className="flex gap-3">
                  <Link
                    className="text-brand-gold hover:underline"
                    href={`/invoice/new?template=${template.id}`}
                  >
                    Use template
                  </Link>
                  <button
                    onClick={() => remove(template.id)}
                    className="text-red-400 hover:underline"
                  >
                    Delete
                  </button>
                </div>
              </article>
            ))}
          </div>
        )}
      </div>
    </main>
  );
}
