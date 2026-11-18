'use client';

import { useState, useEffect, useCallback } from 'react';
import { useRouter, useSearchParams } from 'next/navigation';
import Link from 'next/link';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { buildCreateInvoiceTx, submitTx, getMaxInvoiceAmount } from '@/lib/contracts';
import {
  NETWORK,
  toStroops,
  nativeToScVal,
  Address,
  xdr,
  INVOICE_CONTRACT_ID,
} from '@/lib/stellar';
import { simulateContractCall } from '@/lib/simulateFee';
import { useTransactionSimulation } from '@/hooks/useTransactionSimulation';
import EstimatedFee from '@/components/EstimatedFee';
import GlossaryTerm from '@/components/GlossaryTerm';
import { getInvoiceTemplate, upsertInvoiceTemplate } from '@/lib/invoiceTemplates';

const MIN_AMOUNT = 10;
const DEFAULT_MAX_AMOUNT = 1_000_000;
const MAX_DUE_DAYS = 365;
const MAX_DESCRIPTION_LEN = 256;

function validateForm(
  form: {
    debtor: string;
    amount: string;
    dueDate: string;
    description: string;
    metadataUri: string;
  },
  maxAmount: number,
) {
  const errors: Record<string, string> = {};

  // Debtor
  const debtor = form.debtor.trim();
  if (!debtor) {
    errors.debtor = 'Debtor name is required.';
  } else if (debtor.length < 2) {
    errors.debtor = 'Debtor name must be at least 2 characters.';
  } else if (debtor.length > 100) {
    errors.debtor = 'Debtor name must be 100 characters or fewer.';
  }

  // Amount
  const amount = parseFloat(form.amount);
  if (!form.amount) {
    errors.amount = 'Amount is required.';
  } else if (isNaN(amount) || amount <= 0) {
    errors.amount = 'Amount must be a positive number.';
  } else if (amount < MIN_AMOUNT) {
    errors.amount = `Minimum invoice amount is $${MIN_AMOUNT} USDC.`;
  } else if (amount > maxAmount) {
    errors.amount = `Maximum invoice amount is $${maxAmount.toLocaleString()} USDC.`;
  }

  // Due date
  if (!form.dueDate) {
    errors.dueDate = 'Due date is required.';
  } else {
    const due = new Date(form.dueDate);
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const maxDate = new Date(today);
    maxDate.setDate(maxDate.getDate() + MAX_DUE_DAYS);

    if (due <= today) {
      errors.dueDate = 'Due date must be in the future.';
    } else if (due > maxDate) {
      errors.dueDate = `Due date must be within ${MAX_DUE_DAYS} days from today.`;
    }
  }

  // Description (optional but bounded)
  if (form.description.length > MAX_DESCRIPTION_LEN) {
    errors.description = `Description must be ${MAX_DESCRIPTION_LEN} characters or fewer.`;
  }
  if (
    form.metadataUri &&
    !(
      form.metadataUri.startsWith('ipfs://') ||
      form.metadataUri.startsWith('ar://') ||
      form.metadataUri.startsWith('https://')
    )
  ) {
    errors.metadataUri = 'Metadata URI must start with ipfs://, ar://, or https://';
  }

  return errors;
}

export default function NewInvoicePage() {
  const { wallet } = useStore();
  const router = useRouter();
  const searchParams = useSearchParams();

  const [form, setForm] = useState({
    debtor: '',
    amount: '',
    dueDate: '',
    description: '',
    metadataUri: '',
  });
  const [touched, setTouched] = useState<Record<string, boolean>>({});
  const [loading, setLoading] = useState(false);
  const [maxAmount, setMaxAmount] = useState(DEFAULT_MAX_AMOUNT);

  useEffect(() => {
    getMaxInvoiceAmount()
      .then((amount) => setMaxAmount(amount))
      .catch(() => setMaxAmount(DEFAULT_MAX_AMOUNT));
  }, []);

  useEffect(() => {
    const id = searchParams.get('template');
    if (!id) return;
    const template = getInvoiceTemplate(id);
    if (!template) return;
    const dueDate = new Date(Date.now() + template.dueDays * 86_400_000)
      .toISOString()
      .split('T')[0]!;
    setForm((current) => ({
      ...current,
      amount: String(template.amount),
      dueDate,
      description: template.description,
    }));
    upsertInvoiceTemplate({ ...template, lastUsedAt: new Date().toISOString() });
  }, [searchParams]);

  function saveAsTemplate() {
    const name = window.prompt('Template name');
    if (!name?.trim()) return;
    const dueDays = form.dueDate
      ? Math.max(1, Math.ceil((new Date(form.dueDate).getTime() - Date.now()) / 86_400_000))
      : 30;
    upsertInvoiceTemplate({
      id: crypto.randomUUID(),
      name: name.trim(),
      amount: Number(form.amount) || 0,
      token: 'USDC',
      dueDays,
      description: form.description,
      createdAt: new Date().toISOString(),
      lastUsedAt: null,
    });
    toast.success('Template saved.');
  }

  const errors = validateForm(form, maxAmount);
  const isValid = Object.keys(errors).length === 0;

  const simulateCreate = useCallback(() => {
    if (!wallet.address || !isValid) return null;
    const dueTimestamp = Math.floor(new Date(form.dueDate).getTime() / 1000);
    const amountStroops = toStroops(parseFloat(form.amount));
    return simulateContractCall(
      INVOICE_CONTRACT_ID,
      'create_invoice_with_metadata',
      [
        new Address(wallet.address).toScVal(),
        nativeToScVal(form.debtor, { type: 'string' }),
        nativeToScVal(amountStroops, { type: 'i128' }),
        nativeToScVal(dueTimestamp, { type: 'u64' }),
        nativeToScVal(form.description, { type: 'string' }),
        nativeToScVal('frontend-placeholder-hash', { type: 'string' }),
        form.metadataUri.trim()
          ? nativeToScVal(form.metadataUri.trim(), { type: 'string' })
          : xdr.ScVal.scvVoid(),
      ],
      wallet.address,
    );
  }, [form, wallet.address, isValid]);

  const simulation = useTransactionSimulation(simulateCreate, isValid && !!wallet.address);

  function handleChange(e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) {
    const { name, value } = e.target;
    setForm((prev) => ({ ...prev, [name]: value }));
    setTouched((prev) => ({ ...prev, [name]: true }));
  }

  function handleBlur(e: React.FocusEvent<HTMLInputElement | HTMLTextAreaElement>) {
    setTouched((prev) => ({ ...prev, [e.target.name]: true }));
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    // Mark all fields touched to show all errors on submit attempt
    setTouched({ debtor: true, amount: true, dueDate: true, description: true, metadataUri: true });
    if (!isValid || !wallet.address) return;

    setLoading(true);
    try {
      const dueTimestamp = Math.floor(new Date(form.dueDate).getTime() / 1000);
      const amountStroops = toStroops(parseFloat(form.amount));

      const xdr = await buildCreateInvoiceTx({
        owner: wallet.address,
        debtor: form.debtor,
        amount: amountStroops,
        dueDate: dueTimestamp,
        description: form.description,
        verificationHash: 'frontend-placeholder-hash',
        metadataUri: form.metadataUri.trim() || undefined,
      });

      const freighter = await import('@stellar/freighter-api');
      const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
        networkPassphrase: NETWORK,
        address: wallet.address,
      });
      if (signError) throw new Error(signError.message);

      await submitTx(signedTxXdr);
      toast.success('Invoice tokenized successfully!');
      router.push('/dashboard');
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Transaction failed.');
    } finally {
      setLoading(false);
    }
  }

  const minDate = new Date(Date.now() + 86_400_000).toISOString().split('T')[0];
  const maxDate = new Date(Date.now() + MAX_DUE_DAYS * 86_400_000).toISOString().split('T')[0];

  return (
    <div className="min-h-screen pt-24 pb-16 px-6">
      <div className="max-w-xl mx-auto">
        <div className="mb-8">
          <h1 className="text-3xl font-bold mb-1">Tokenize Invoice</h1>
          <p className="text-brand-muted">
            Mint your unpaid invoice as a <GlossaryTerm id="soroban" /> RWA token to access instant
            liquidity.
          </p>
          <div className="mt-2 flex gap-4">
            <Link href="/invoice/import" className="text-sm text-brand-gold hover:underline">
              Import multiple invoices via CSV
            </Link>
            <Link href="/templates" className="text-sm text-brand-gold hover:underline">
              Template library
            </Link>
          </div>
        </div>

        {!wallet.connected ? (
          <div className="p-12 bg-brand-card border border-brand-border rounded-2xl text-center">
            <p className="text-brand-muted">Connect your wallet first.</p>
          </div>
        ) : (
          <form onSubmit={handleSubmit} className="space-y-5" noValidate>
            <div className="p-6 bg-brand-card border border-brand-border rounded-2xl space-y-5">
              {/* Debtor */}
              <Field
                label="Debtor (who owes you)"
                name="debtor"
                placeholder="ACME Corporation Ltd."
                value={form.debtor}
                onChange={handleChange}
                onBlur={handleBlur}
                error={touched.debtor ? errors.debtor : undefined}
              />

              {/* Amount */}
              <div>
                <label htmlFor="amount" className="block text-sm text-brand-muted mb-2">
                  Invoice Amount (USDC)
                </label>
                <div className="relative">
                  <input
                    id="amount"
                    type="number"
                    name="amount"
                    min={MIN_AMOUNT}
                    max={maxAmount}
                    step="0.01"
                    placeholder="0.00"
                    value={form.amount}
                    onChange={handleChange}
                    onBlur={handleBlur}
                    className={`w-full bg-brand-dark border rounded-xl px-4 py-3 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-lg ${
                      touched.amount && errors.amount ? 'border-red-500' : 'border-brand-border'
                    }`}
                  />
                  <span className="absolute right-4 top-1/2 -translate-y-1/2 text-brand-muted text-sm font-medium">
                    USDC
                  </span>
                </div>
                <ErrorMsg message={touched.amount ? errors.amount : undefined} />
                <p className="text-xs text-brand-muted mt-1">
                  Max: ${maxAmount.toLocaleString()} USDC (synced from contract)
                </p>
              </div>

              {/* Due Date */}
              <div>
                <label htmlFor="dueDate" className="block text-sm text-brand-muted mb-2">
                  Due Date
                </label>
                <input
                  id="dueDate"
                  type="date"
                  name="dueDate"
                  min={minDate}
                  max={maxDate}
                  value={form.dueDate}
                  onChange={handleChange}
                  onBlur={handleBlur}
                  className={`w-full bg-brand-dark border rounded-xl px-4 py-3 text-white focus:outline-none focus:border-brand-gold ${
                    touched.dueDate && errors.dueDate ? 'border-red-500' : 'border-brand-border'
                  }`}
                />
                <ErrorMsg message={touched.dueDate ? errors.dueDate : undefined} />
              </div>

              {/* Description */}
              <div>
                <label htmlFor="description" className="block text-sm text-brand-muted mb-2">
                  Description{' '}
                  <span className="text-brand-muted/60">
                    (optional, {form.description.length}/{MAX_DESCRIPTION_LEN})
                  </span>
                </label>
                <textarea
                  id="description"
                  name="description"
                  rows={3}
                  placeholder="Invoice #001 - Goods delivery, 500 units..."
                  value={form.description}
                  onChange={handleChange}
                  onBlur={handleBlur}
                  className={`w-full bg-brand-dark border rounded-xl px-4 py-3 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold resize-none ${
                    touched.description && errors.description
                      ? 'border-red-500'
                      : 'border-brand-border'
                  }`}
                />
                <ErrorMsg message={touched.description ? errors.description : undefined} />
              </div>

              <Field
                label="Document Metadata URI (optional)"
                name="metadataUri"
                placeholder="ipfs://bafy... or https://..."
                value={form.metadataUri}
                onChange={handleChange}
                onBlur={handleBlur}
                error={touched.metadataUri ? errors.metadataUri : undefined}
              />
            </div>

            {/* Summary */}
            {form.amount && form.dueDate && !errors.amount && !errors.dueDate && (
              <div className="p-4 bg-brand-gold/10 border border-brand-gold/20 rounded-xl text-sm space-y-2">
                <p className="text-brand-gold font-medium">Invoice Summary</p>
                <div className="flex justify-between text-brand-muted">
                  <span>Invoice amount</span>
                  <span className="text-white">
                    ${parseFloat(form.amount).toLocaleString()} USDC
                  </span>
                </div>
                <div className="flex justify-between text-brand-muted">
                  <span>Due date</span>
                  <span className="text-white">
                    {new Date(form.dueDate).toLocaleDateString('en-US', {
                      year: 'numeric',
                      month: 'short',
                      day: 'numeric',
                    })}
                  </span>
                </div>
                <div className="flex justify-between text-brand-muted">
                  <span>Estimated repayment (8% APY)</span>
                  <span className="text-white">
                    ${(parseFloat(form.amount) * 1.08).toFixed(2)} USDC
                  </span>
                </div>
              </div>
            )}

            <EstimatedFee simulation={simulation} />

            <button
              type="submit"
              disabled={loading || simulation.status === 'loading'}
              className="w-full py-3.5 bg-brand-gold text-brand-dark font-semibold rounded-xl hover:bg-brand-amber transition-colors disabled:opacity-60 text-lg"
            >
              {loading ? 'Minting on Stellar...' : 'Mint Invoice Token'}
            </button>

            <p className="text-xs text-brand-muted text-center">
              Your invoice will be tokenized on Stellar Testnet. Gas fees are under $0.01.
            </p>
          </form>
        )}
      </div>
    </div>
  );
}

function ErrorMsg({ message }: { message?: string }) {
  if (!message) return null;
  return <p className="mt-1.5 text-xs text-red-400">{message}</p>;
}

function Field({
  label,
  name,
  placeholder,
  value,
  onChange,
  onBlur,
  error,
}: {
  label: string;
  name: string;
  placeholder: string;
  value: string;
  onChange: (e: React.ChangeEvent<HTMLInputElement>) => void;
  onBlur: (e: React.FocusEvent<HTMLInputElement>) => void;
  error?: string;
}) {
  return (
    <div>
      <label htmlFor={name} className="block text-sm text-brand-muted mb-2">
        {label}
      </label>
      <input
        id={name}
        type="text"
        name={name}
        placeholder={placeholder}
        value={value}
        onChange={onChange}
        onBlur={onBlur}
        className={`w-full bg-brand-dark border rounded-xl px-4 py-3 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold ${
          error ? 'border-red-500' : 'border-brand-border'
        }`}
      />
      <ErrorMsg message={error} />
    </div>
  );
}
