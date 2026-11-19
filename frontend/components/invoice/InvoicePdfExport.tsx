'use client';

/** PDF export + share toolbar buttons from the invoice detail header. */
interface InvoicePdfExportProps {
  pdfLoading: boolean;
  onExport: () => void;
  onShare: () => void;
}

export default function InvoicePdfExport({
  pdfLoading,
  onExport,
  onShare,
}: InvoicePdfExportProps) {
  return (
    <>
      <button
        onClick={onExport}
        disabled={pdfLoading}
        className="print:hidden text-sm text-brand-muted hover:text-white ml-4 disabled:opacity-60"
      >
        {pdfLoading ? 'Generating PDF...' : 'Export PDF'}
      </button>
      <button
        onClick={onShare}
        className="print:hidden text-sm text-brand-muted hover:text-white ml-4"
      >
        Share
      </button>
    </>
  );
}
