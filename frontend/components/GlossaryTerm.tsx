'use client';

import Tooltip from '@/components/Tooltip';
import { getGlossaryEntry } from '@/lib/glossary';

interface GlossaryTermProps {
  /** id from lib/glossary.ts, e.g. "collateral-ratio" */
  id: string;
  /** Optional override for the visible label (defaults to the glossary term). */
  children?: React.ReactNode;
  className?: string;
}

/**
 * Inline, dotted-underline term that reveals its glossary definition on
 * hover or focus/tap. Falls back to rendering plain text if `id` doesn't
 * match a known glossary entry, so a typo never breaks the page.
 */
export default function GlossaryTerm({ id, children, className = '' }: GlossaryTermProps) {
  const entry = getGlossaryEntry(id);
  if (!entry) return <>{children}</>;

  return (
    <Tooltip content={entry.definition}>
      <button
        type="button"
        className={`underline decoration-dotted decoration-brand-muted/70 underline-offset-4 hover:decoration-brand-gold focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-gold rounded cursor-help ${className}`}
        aria-label={`${entry.term}: definition`}
      >
        {children ?? entry.term}
      </button>
    </Tooltip>
  );
}
