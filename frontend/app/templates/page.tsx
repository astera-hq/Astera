'use client';

import Link from 'next/link';
import { useEffect, useState } from 'react';
import {
  deleteInvoiceTemplate,
  loadInvoiceTemplates,
  type InvoiceTemplate,
} from '@/lib/invoiceTemplates';
import ConfirmActionModal from '@/components/ConfirmActionModal';

export default function TemplatesPage() {
  const [templates, setTemplates] = useState<InvoiceTemplate[]>([]);
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null);
  useEffect(() => setTemplates(loadInvoiceTemplates()), []);
  const remove = (id: string) => {
    deleteInvoiceTemplate(id);
    setTemplates(loadInvoiceTemplates());
    setPendingDeleteId(null);
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
                    {template.amount.toLocaleString()} {template.token} · due in {template.dueDays}{' '}
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
                    onClick={() => setPendingDeleteId(template.id)}
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
      <ConfirmActionModal
        title="Delete this template?"
        description="This will permanently remove the saved invoice template. This action cannot be undone."
        onConfirm={() => pendingDeleteId !== null && remove(pendingDeleteId)}
        onCancel={() => setPendingDeleteId(null)}
        variant="destructive"
        isOpen={pendingDeleteId !== null}
        confirmLabel="Delete"
      />
    </main>
  );
}
