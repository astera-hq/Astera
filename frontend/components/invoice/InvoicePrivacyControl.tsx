'use client';

/** Owner-only "Make Private" / "Make Public" toggle. */
interface InvoicePrivacyControlProps {
  isPrivate: boolean;
  privacyLoading: boolean;
  onToggle: () => void;
}

export default function InvoicePrivacyControl({
  isPrivate,
  privacyLoading,
  onToggle,
}: InvoicePrivacyControlProps) {
  return (
    <button
      onClick={onToggle}
      disabled={privacyLoading}
      className="print:hidden text-sm text-brand-muted hover:text-white ml-4 disabled:opacity-60"
    >
      {privacyLoading ? 'Updating...' : isPrivate ? 'Make Public' : 'Make Private'}
    </button>
  );
}
