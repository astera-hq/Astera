'use client';

/**
 * ExplorerLink — Stellar deep-link component (#228).
 *
 * Renders a truncated identifier with a tooltip showing the full value,
 * and an external-link icon. Opens in a new tab with rel="noopener noreferrer".
 *
 * Usage:
 *   <ExplorerLink type="transaction" id={txHash} />
 *   <ExplorerLink type="account" id={address}>View wallet ↗</ExplorerLink>
 *   <ExplorerLink type="contract" id={contractId} network="mainnet" />
 */

import { explorerUrl, truncateAddress } from '@/lib/stellar';
import type { ExplorerEntity, StellarNetwork } from '@/lib/stellar';

interface ExplorerLinkProps {
  /** Entity type passed to explorerUrl(). */
  type: ExplorerEntity;
  /** Full on-chain identifier — address, tx hash, contract ID, or ledger number. */
  id: string;
  /** Override network; defaults to NEXT_PUBLIC_STELLAR_NETWORK. */
  network?: StellarNetwork;
  /** Custom link label. Defaults to a truncated version of `id`. */
  children?: React.ReactNode;
  /** Extra CSS classes applied to the <a> element. */
  className?: string;
}

export function ExplorerLink({
  type,
  id,
  network,
  children,
  className = '',
}: ExplorerLinkProps) {
  if (!id) return null;

  const url = explorerUrl(type, id, network);
  const label = children ?? truncateAddress(id);

  return (
    <a
      href={url}
      target="_blank"
      rel="noopener noreferrer"
      title={id}
      className={`inline-flex items-center gap-1 font-mono text-xs text-blue-400 hover:text-blue-300 hover:underline transition-colors ${className}`}
    >
      {label}
      {/* External link icon */}
      <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 16 16"
        fill="currentColor"
        className="h-3 w-3 shrink-0"
        aria-hidden="true"
      >
        <path
          fillRule="evenodd"
          d="M4.75 3.5A.75.75 0 0 0 4 4.25v7.5c0 .414.336.75.75.75h7.5a.75.75 0 0 0 .75-.75V9a.75.75 0 0 1 1.5 0v2.75A2.25 2.25 0 0 1 12.25 14h-7.5A2.25 2.25 0 0 1 2.5 11.75v-7.5A2.25 2.25 0 0 1 4.75 2H7.5a.75.75 0 0 1 0 1.5H4.75Z"
          clipRule="evenodd"
        />
        <path
          fillRule="evenodd"
          d="M9.25 2a.75.75 0 0 1 .75-.75h4a.75.75 0 0 1 .75.75v4a.75.75 0 0 1-1.5 0V3.56L8.03 8.78a.75.75 0 0 1-1.06-1.06l5.22-5.22H10a.75.75 0 0 1-.75-.75Z"
          clipRule="evenodd"
        />
      </svg>
    </a>
  );
}
