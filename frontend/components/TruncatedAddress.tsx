'use client';

import { useState } from 'react';
import { truncateAddress } from '@/lib/stellar';

interface TruncatedAddressProps {
  /** Full Stellar wallet address */
  address: string;
  /** Number of characters to show on each side (default: 6) */
  chars?: number;
  /** Additional CSS classes */
  className?: string;
}

/**
 * TruncatedAddress — renders a shortened Stellar address with
 * click-to-copy functionality.
 *
 * Shows a checkmark icon for 2 seconds after copying.
 * Displays the full address in a tooltip on hover.
 */
export function TruncatedAddress({
  address,
  chars = 6,
  className = '',
}: TruncatedAddressProps) {
  const [copied, setCopied] = useState(false);

  if (!address) return null;

  const truncated = truncateAddress(address, chars);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(address);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Clipboard write failed silently
    }
  };

  return (
    <span
      className={`inline-flex items-center gap-1 font-mono text-xs ${className}`}
      title={address}
    >
      <span>{truncated}</span>
      <button
        onClick={handleCopy}
        aria-label={copied ? 'Address copied' : 'Copy address to clipboard'}
        className="inline-flex items-center justify-center w-4 h-4 text-[var(--muted)] hover:text-[var(--text-primary)] transition-colors"
      >
        {copied ? (
          /* Checkmark icon */
          <svg
            xmlns="http://www.w3.org/2000/svg"
            viewBox="0 0 16 16"
            fill="currentColor"
            className="w-3.5 h-3.5 text-green-500"
          >
            <path
              fillRule="evenodd"
              d="M12.416 3.376a.75.75 0 0 1 .208 1.04l-5 7.5a.75.75 0 0 1-1.154.114l-3-3a.75.75 0 0 1 1.06-1.06l2.353 2.353 4.493-6.74a.75.75 0 0 1 1.04-.207z"
              clipRule="evenodd"
            />
          </svg>
        ) : (
          /* Copy icon */
          <svg
            xmlns="http://www.w3.org/2000/svg"
            viewBox="0 0 16 16"
            fill="currentColor"
            className="w-3.5 h-3.5"
          >
            <path d="M5.5 3.5A1.5 1.5 0 0 1 7 2h5a1.5 1.5 0 0 1 1.5 1.5v5A1.5 1.5 0 0 1 12 10H7a1.5 1.5 0 0 1-1.5-1.5v-5z" />
            <path d="M3.5 5A1.5 1.5 0 0 0 2 6.5v5A1.5 1.5 0 0 0 3.5 13h5a1.5 1.5 0 0 0 1.5-1.5V10H8.5v1.5a.5.5 0 0 1-.5.5h-5a.5.5 0 0 1-.5-.5v-5a.5.5 0 0 1 .5-.5H5V5h-1.5z" />
          </svg>
        )}
      </button>
    </span>
  );
}
