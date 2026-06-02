'use client';

import { useState, useRef, useCallback } from 'react';
import Link from 'next/link';
import toast from 'react-hot-toast';
import { useStore } from '@/lib/store';
import { buildCreateInvoiceTx, submitTx } from '@/lib/contracts';
import { toStroops, formatUSDC } from '@/lib/stellar';

const MAX_INVOICES_PER_DAY = 10;

interface ParsedRow {
  rowNum: number;
  debtor: string;
  amount: string;
  dueDate: string;
  description: string;
  verificationHash: string;
  errors: string[];
}

const CSV_TEMPLATE = `debtor_name,amount,due_date,description,verification_hash
ACME Corporation Ltd.,5000.00,2026-06-30,Invoice #001 - Goods delivery,abc123
ACME Corporation Ltd.,7500.00,2026-07-15,Invoice #002 - Services renderedo,def456`;

type Step = 'upload' | 'preview' | 'submit' | 'results';

export default function ImportInvoicePage() {
  const { wallet } = useStore();
  const fileInputRef = useRef<HTMLInputElement>(null);

  const [step, setStep] = useState<Step>('upload');
  const [parsedRows, setParsedRows] = useState<ParsedRow[]>([]);
  const [validRows, setValidRows] = useState<ParsedRow[]>([]);
  const [currentIndex, setCurrentIndex] = useState(0);
  const [results, setResults] = useState<{ row: ParsedRow; success: boolean; invoiceId?: number; error?: string }[]>([]);

  const parseCSV = useCallback((content: string): ParsedRow[] => {
    const lines = content.trim().split('\n');
    if (lines.length < 2) return [];

    const header = lines[0]!.toLowerCase().split(',').map(h => h.trim());
    const debtorIdx = header.indexOf('debtor_name');
    const amountIdx = header.indexOf('amount');
    const dueDateIdx = header.indexOf('due_date');
    const descIdx = header.indexOf('description');
    const hashIdx = header.indexOf('verification_hash');

    if (debtorIdx === -1 || amountIdx === -1 || dueDateIdx === -1) {
      toast.error('Missing required columns: debtor_name, amount, due_date');
      return [];
    }

    const rows: ParsedRow[] = [];
    for (let i = 1; i < lines.length; i++) {
      const cols = lines[i]!.split(',').map(c => c.trim().replace(/^"|"$/g, ''));
      const debtor = cols[debtorIdx] || '';
      const amount = cols[amountIdx] || '';
      const dueDate = cols[dueDateIdx] || '';
      const description = descIdx >= 0 ? cols[descIdx] || '' : '';
      const verificationHash = hashIdx >= 0 ? cols[hashIdx] || '' : '';

      const errors: string[] = [];
      if (!debtor) errors.push('Debtor name is required');
      const amountNum = parseFloat(amount);
      if (!amount || isNaN(amountNum) || amountNum <= 0) {
        errors.push('Amount must be greater than 0');
      }
      if (!/^\d{4}-\d{2}-\d{2}$/.test(dueDate)) {
        errors.push('Due date must be YYYY-MM-DD format');
      } else if (new Date(dueDate).getTime() <= Date.now()) {
        errors.push('Due date must be in the future');
      }
      if (description.length > 256) {
        errors.push('Description too long (max 256 chars)');
      }

      rows.push({
        rowNum: i,
        debtor,
        amount,
        dueDate,
        description,
        verificationHash,
        errors,
      });
    }

    return rows;
  }, []);

  const handleFileSelect = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    if (!file.name.endsWith('.csv')) {
      toast.error('Please select a CSV file');
      return;
    }

    const reader = new FileReader();
    reader.onload = (ev) => {
      const content = ev.target?.result as string;
      const rows = parseCSV(content);
      if (rows.length > 0) {
        setParsedRows(rows);
        setValidRows(rows.filter(r => r.errors.length === 0));
        setStep('preview');
      }
    };
    reader.readAsText(file);
  }, [parseCSV]);

  const downloadTemplate = useCallback(() => {
    const blob = new Blob([CSV_TEMPLATE], { type: 'text/csv;charset=utf-8;' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'invoice-template.csv';
    a.click();
    URL.revokeObjectURL(url);
  }, []);

  const removeRow = useCallback((rowNum: number) => {
    setParsedRows(prev => prev.filter(r => r.rowNum !== rowNum));
    setValidRows(prev => prev.filter(r => r.rowNum !== rowNum));
  }, []);

  const handleStartImport = useCallback(async () => {
    if (!wallet.address) {
      toast.error('Please connect your wallet');
      return;
    }
    if (validRows.length > MAX_INVOICES_PER_DAY) {
      toast.error(`Maximum ${MAX_INVOICES_PER_DAY} invoices per day. Please reduce the number of invoices.`);
      return;
    }

    setStep('submit');
    setCurrentIndex(0);
    setResults([]);

    for (let i = 0; i < validRows.length; i++) {
      setCurrentIndex(i);
      const row = validRows[i]!;

      try {
        const dueTimestamp = Math.floor(new Date(row.dueDate).getTime() / 1000);
        const amountStroops = toStroops(parseFloat(row.amount));

        const xdr = await buildCreateInvoiceTx({
          owner: wallet.address,
          debtor: row.debtor,
          amount: amountStroops,
          dueDate: dueTimestamp,
          description: row.description,
        });

        const freighter = await import('@stellar/freighter-api');
        const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
          networkPassphrase: 'Test SDF Network ; September 2015',
          address: wallet.address,
        });
        if (signError) throw new Error(signError.message);

        await submitTx(signedTxXdr);

        setResults(prev => [...prev, { row, success: true, invoiceId: i + 1 }]);
      } catch (e) {
        const error = e instanceof Error ? e.message : 'Transaction failed';
        setResults(prev => [...prev, { row, success: false, error }]);
      }
    }

    setStep('results');
  }, [wallet.address, validRows]);

  const downloadResults = useCallback(() => {
    const header = ['Row', 'Debtor', 'Amount', 'Due Date', 'Status', 'Invoice ID', 'Error'];
    const rows = results.map(r => [
      r.row.rowNum,
      r.row.debtor,
      r.row.amount,
      r.row.dueDate,
      r.success ? 'SUCCESS' : 'FAILED',
      r.invoiceId?.toString() || '',
      r.error || '',
    ]);

    const csv = [header, ...rows]
      .map(r => r.map(c => `"${String(c).replace(/"/g, '""')}"`).join(','))
      .join('\n');

    const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `import-results-${new Date().toISOString().slice(0, 10)}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  }, [results]);

  return (
    <div className="min-h-screen pt-24 pb-16 px-6">
      <div className="max-w-3xl mx-auto">
        <Link
          href="/dashboard"
          className="text-brand-muted hover:text-white text-sm mb-6 inline-flex items-center gap-2 transition-colors"
        >
          ← Back to Dashboard
        </Link>

        <div className="mb-8">
          <h1 className="text-3xl font-bold mb-1">Import Invoices</h1>
          <p className="text-brand-muted">
            Bulk create invoices from a CSV file. Each invoice requires a separate transaction.
          </p>
        </div>

        {!wallet.connected ? (
          <div className="p-12 bg-brand-card border border-brand-border rounded-2xl text-center">
            <p className="text-brand-muted">Connect your wallet first.</p>
          </div>
        ) : step === 'upload' ? (
          <div className="space-y-6">
            <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
              <h2 className="text-lg font-semibold mb-4">Upload CSV</h2>
              <p className="text-sm text-brand-muted mb-4">
                Upload a CSV file with invoice data. The file must include the following columns:
              </p>
              <ul className="text-sm text-brand-muted mb-4 space-y-1">
                <li><code className="text-brand-gold">debtor_name</code> - Name of the debtor (required)</li>
                <li><code className="text-brand-gold">amount</code> - Invoice amount in USDC (required)</li>
                <li><code className="text-brand-gold">due_date</code> - Due date as YYYY-MM-DD (required)</li>
                <li><code className="text-brand-gold">description</code> - Invoice description (optional)</li>
                <li><code className="text-brand-gold">verification_hash</code> - Verification hash (optional)</li>
              </ul>

              <input
                ref={fileInputRef}
                type="file"
                accept=".csv"
                onChange={handleFileSelect}
                className="hidden"
              />
              <div className="flex gap-3">
                <button
                  onClick={() => fileInputRef.current?.click()}
                  className="px-5 py-3 bg-brand-gold text-brand-dark font-semibold rounded-xl hover:bg-brand-amber transition-colors"
                >
                  Select CSV File
                </button>
                <button
                  onClick={downloadTemplate}
                  className="px-5 py-3 border border-brand-border text-white font-semibold rounded-xl hover:border-brand-gold/50 transition-colors"
                >
                  Download Template
                </button>
              </div>
            </div>
          </div>
        ) : step === 'preview' ? (
          <div className="space-y-6">
            <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
              <div className="flex items-center justify-between mb-4">
                <h2 className="text-lg font-semibold">Preview ({parsedRows.length} rows)</h2>
                <span className="text-sm text-brand-muted">
                  {validRows.length} valid, {parsedRows.length - validRows.length} with errors
                </span>
              </div>

              {validRows.length > MAX_INVOICES_PER_DAY && (
                <div className="p-3 bg-yellow-900/20 border border-yellow-800/50 rounded-xl text-sm text-yellow-400 mb-4">
                  Warning: You are about to create {validRows.length} invoices. The rate limit is {MAX_INVOICES_PER_DAY} invoices/day. Consider reducing the number.
                </div>
              )}

              <div className="overflow-x-auto">
                <table className="w-full text-left text-sm">
                  <thead>
                    <tr className="border-b border-brand-border">
                      <th className="pb-2 font-medium text-brand-muted">#</th>
                      <th className="pb-2 font-medium text-brand-muted">Debtor</th>
                      <th className="pb-2 font-medium text-brand-muted">Amount</th>
                      <th className="pb-2 font-medium text-brand-muted">Due Date</th>
                      <th className="pb-2 font-medium text-brand-muted">Status</th>
                      <th className="pb-2 font-medium text-brand-muted"></th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-brand-border">
                    {parsedRows.slice(0, 20).map(row => (
                      <tr key={row.rowNum} className={row.errors.length > 0 ? 'opacity-60' : ''}>
                        <td className="py-2">{row.rowNum}</td>
                        <td className="py-2">{row.debtor}</td>
                        <td className="py-2">{formatUSDC(toStroops(parseFloat(row.amount)))}</td>
                        <td className="py-2">{row.dueDate}</td>
                        <td className="py-2">
                          {row.errors.length > 0 ? (
                            <span className="text-red-400 text-xs">{row.errors.join(', ')}</span>
                          ) : (
                            <span className="text-green-400 text-xs">Valid</span>
                          )}
                        </td>
                        <td className="py-2">
                          <button
                            onClick={() => removeRow(row.rowNum)}
                            className="text-brand-muted hover:text-red-400 text-xs"
                          >
                            Remove
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>

              {parsedRows.length > 20 && (
                <p className="text-xs text-brand-muted mt-2">Showing first 20 rows...</p>
              )}

              <div className="flex gap-3 mt-6">
                <button
                  onClick={() => setStep('upload')}
                  className="px-5 py-3 border border-brand-border text-white font-semibold rounded-xl hover:border-brand-gold/50 transition-colors"
                >
                  Back
                </button>
                <button
                  onClick={handleStartImport}
                  disabled={validRows.length === 0}
                  className="px-5 py-3 bg-brand-gold text-brand-dark font-semibold rounded-xl hover:bg-brand-amber transition-colors disabled:opacity-60"
                >
                  Import {validRows.length} Invoice{validRows.length !== 1 ? 's' : ''}
                </button>
              </div>
            </div>
          </div>
        ) : step === 'submit' ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
            <h2 className="text-lg font-semibold mb-4">Importing Invoices</h2>
            <p className="text-brand-muted mb-4">
              Processing invoice {currentIndex + 1} of {validRows.length}...
            </p>
            <div className="h-2 bg-brand-border rounded-full overflow-hidden">
              <div
                className="h-full bg-brand-gold rounded-full transition-all"
                style={{ width: `${((currentIndex + 1) / validRows.length) * 100}%` }}
              />
            </div>
            <p className="text-sm text-brand-muted mt-2">
              Please approve each transaction in Freighter
            </p>
          </div>
        ) : (
          <div className="space-y-6">
            <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
              <h2 className="text-lg font-semibold mb-4">Import Complete</h2>
              <div className="space-y-2 mb-4">
                <p className="text-green-400">
                  {results.filter(r => r.success).length} invoices created successfully
                </p>
                {results.filter(r => !r.success).length > 0 && (
                  <p className="text-red-400">
                    {results.filter(r => !r.success).length} invoices failed
                  </p>
                )}
              </div>

              <div className="flex gap-3">
                <button
                  onClick={downloadResults}
                  className="px-5 py-3 bg-brand-gold text-brand-dark font-semibold rounded-xl hover:bg-brand-amber transition-colors"
                >
                  Download Results CSV
                </button>
                <Link
                  href="/dashboard"
                  className="px-5 py-3 border border-brand-border text-white font-semibold rounded-xl hover:border-brand-gold/50 transition-colors"
                >
                  Back to Dashboard
                </Link>
              </div>
            </div>

            <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
              <h3 className="font-semibold mb-3">Details</h3>
              <div className="space-y-2 text-sm">
                {results.map((r, i) => (
                  <div key={i} className="flex items-center justify-between p-2 bg-brand-dark rounded-lg">
                    <span className={r.success ? 'text-green-400' : 'text-red-400'}>
                      {r.success ? '✓' : '✗'} Invoice #{r.row.rowNum}
                    </span>
                    <span className="text-brand-muted">
                      {r.row.debtor} - {formatUSDC(toStroops(parseFloat(r.row.amount)))}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}