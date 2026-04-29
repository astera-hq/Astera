'use client';

import { useEffect, useRef, useCallback, useState } from 'react';

export type ConfirmActionVariant = 'default' | 'destructive';

export interface ConfirmActionModalProps {
  /** Modal title */
  title: string;
  /** Modal description / warning text */
  description: string;
  /**
   * If provided, user must type this exact phrase to enable the confirm button.
   * Used for destructive actions (e.g. "CONFIRM DEFAULT").
   * If omitted, a simple OK/Cancel dialog is shown.
   */
  confirmPhrase?: string;
  /** Called when user confirms the action */
  onConfirm: () => void;
  /** Called when user cancels */
  onCancel: () => void;
  /** Visual variant - destructive uses red theme */
  variant?: ConfirmActionVariant;
  /** Whether the modal is currently visible */
  isOpen: boolean;
  /** Confirm button label (default: "Confirm") */
  confirmLabel?: string;
  /** Cancel button label (default: "Cancel") */
  cancelLabel?: string;
  /** Optional extra content (e.g. inputs) */
  children?: React.ReactNode;
}

/**
 * Reusable confirmation modal for destructive and standard admin actions.
 *
 * Features:
 * - Focus trap within the modal
 * - Keyboard dismissal (Escape)
 * - Optional confirmation phrase for destructive actions
 * - ARIA labels for screen reader accessibility
 * - Backdrop overlay
 *
 * @example
 * // Simple confirmation
 * <ConfirmActionModal
 *   title="Verify Invoice #42"
 *   description="Mark this invoice as verified and release funds."
 *   onConfirm={handleVerify}
 *   onCancel={() => setModalOpen(false)}
 *   isOpen={isOpen}
 * />
 *
 * @example
 * // Destructive action with confirmation phrase
 * <ConfirmActionModal
 *   title="Mark Invoice #42 as Defaulted"
 *   description="This will permanently mark the invoice as defaulted, seize any collateral, and reduce the SME credit score by 50 points. This cannot be undone."
 *   confirmPhrase="CONFIRM DEFAULT"
 *   onConfirm={handleMarkDefaulted}
 *   onCancel={() => setModalOpen(false)}
 *   variant="destructive"
 *   isOpen={isOpen}
 * />
 */
export default function ConfirmActionModal({
  title,
  description,
  confirmPhrase,
  onConfirm,
  onCancel,
  variant = 'default',
  isOpen,
  confirmLabel = 'Confirm',
  cancelLabel = 'Cancel',
  children,
}: ConfirmActionModalProps) {
  const modalRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const firstFocusableRef = useRef<HTMLButtonElement>(null);
  const [typedPhrase, setTypedPhrase] = useState('');
  const previousFocusRef = useRef<HTMLElement | null>(null);

  const isDestructive = variant === 'destructive';
  const isPhraseMatch = confirmPhrase ? typedPhrase === confirmPhrase : true;
  const canConfirm = confirmPhrase ? isPhraseMatch : true;

  // Store the previously focused element and manage focus
  useEffect(() => {
    if (isOpen) {
      previousFocusRef.current = document.activeElement as HTMLElement;
      // Focus the input (if phrase required) or the confirm button
      const focusTarget = confirmPhrase ? inputRef.current : firstFocusableRef.current;
      focusTarget?.focus();
    } else {
      // Reset state when modal closes
      queueMicrotask(() => setTypedPhrase(''));
      // Restore focus to the element that was focused before the modal opened
      previousFocusRef.current?.focus();
    }
  }, [isOpen, confirmPhrase]);

  // Focus trap
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onCancel();
        return;
      }

      if (e.key === 'Tab' && modalRef.current) {
        const focusableElements = modalRef.current.querySelectorAll<HTMLElement>(
          'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
        );
        const firstElement = focusableElements[0];
        const lastElement = focusableElements[focusableElements.length - 1];

        if (e.shiftKey) {
          if (document.activeElement === firstElement) {
            e.preventDefault();
            lastElement?.focus();
          }
        } else {
          if (document.activeElement === lastElement) {
            e.preventDefault();
            firstElement?.focus();
          }
        }
      }
    },
    [onCancel],
  );

  const handleConfirm = useCallback(() => {
    if (canConfirm) {
      onConfirm();
      setTypedPhrase('');
    }
  }, [canConfirm, onConfirm]);

  const handleBackdropClick = useCallback(
    (e: React.MouseEvent) => {
      if (e.target === e.currentTarget) {
        onCancel();
      }
    },
    [onCancel],
  );

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onClick={handleBackdropClick}
      role="presentation"
    >
      <div
        ref={modalRef}
        onKeyDown={handleKeyDown}
        className={`
          relative w-full max-w-md mx-4 rounded-2xl border shadow-2xl
          bg-brand-card border-brand-border
          ${isDestructive ? 'border-red-700/50' : ''}
          animate-in fade-in zoom-in-95 duration-200
        `}
        role="dialog"
        aria-modal="true"
        aria-labelledby="confirm-modal-title"
        aria-describedby="confirm-modal-description"
      >
        {/* Header */}
        <div className="flex items-start gap-4 p-6 pb-0">
          {/* Icon */}
          <div
            className={`
              flex-shrink-0 w-10 h-10 rounded-full flex items-center justify-center
              ${isDestructive ? 'bg-red-900/40 text-red-400' : 'bg-brand-gold/20 text-brand-gold'}
            `}
            aria-hidden="true"
          >
            {isDestructive ? (
              /* Warning triangle icon */
              <svg
                className="w-6 h-6"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                strokeWidth={2}
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z"
                />
              </svg>
            ) : (
              /* Info icon */
              <svg
                className="w-6 h-6"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                strokeWidth={2}
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M11.25 11.25l.041-.02a.75.75 0 011.063.852l-.708 2.836a.75.75 0 001.063.853l.041-.021M21 12a9 9 0 11-18 0 9 9 0 0118 0zm-9-3.75h.008v.008H12V8.25z"
                />
              </svg>
            )}
          </div>

          {/* Text content */}
          <div className="flex-1 min-w-0">
            <h2 id="confirm-modal-title" className="text-lg font-semibold text-white leading-tight">
              {title}
            </h2>
            <p
              id="confirm-modal-description"
              className="mt-2 text-sm text-brand-muted leading-relaxed"
            >
              {description}
            </p>
          </div>

          {/* Close button */}
          <button
            onClick={onCancel}
            className="flex-shrink-0 text-brand-muted hover:text-white transition-colors p-1 -mr-1 -mt-1 rounded-lg hover:bg-brand-border"
            aria-label="Close dialog"
          >
            <svg
              className="w-5 h-5"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Confirmation phrase input (for destructive actions) */}
        {confirmPhrase && (
          <div className="px-6 pt-4">
            <label
              htmlFor="confirm-phrase-input"
              className="block text-xs font-medium text-brand-muted mb-2"
            >
              Type <span className="font-mono text-white">{confirmPhrase}</span> to confirm:
            </label>
            <input
              ref={inputRef}
              id="confirm-phrase-input"
              type="text"
              value={typedPhrase}
              onChange={(e) => setTypedPhrase(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && canConfirm) {
                  e.preventDefault();
                  handleConfirm();
                }
              }}
              placeholder={confirmPhrase}
              className={`
                w-full px-4 py-2.5 rounded-xl border text-sm font-mono
                bg-brand-dark text-white placeholder-brand-muted/50
                focus:outline-none focus:ring-2 transition-colors
                ${
                  isPhraseMatch
                    ? 'border-brand-border focus:ring-brand-gold/40'
                    : 'border-red-700/40 focus:ring-red-500/40'
                }
              `}
              aria-describedby="confirm-phrase-hint"
              autoComplete="off"
              spellCheck={false}
            />
            <p
              id="confirm-phrase-hint"
              className={`mt-1.5 text-xs ${isPhraseMatch ? 'text-brand-muted' : 'text-red-400'}`}
            >
              {isPhraseMatch ? 'Phrase matches' : `Current: "${typedPhrase}"`}
            </p>
          </div>
        )}

        {children && <div className="px-6 pt-4">{children}</div>}

        {/* Footer actions */}
        <div className="flex items-center justify-end gap-3 p-6 pt-4">
          <button
            ref={firstFocusableRef}
            onClick={onCancel}
            className="px-4 py-2.5 text-sm font-medium text-brand-muted hover:text-white bg-brand-dark border border-brand-border rounded-xl hover:bg-brand-border transition-colors focus:outline-none focus:ring-2 focus:ring-brand-gold/40"
          >
            {cancelLabel}
          </button>
          <button
            onClick={handleConfirm}
            disabled={!canConfirm}
            className={`
              px-5 py-2.5 text-sm font-medium rounded-xl transition-colors
              focus:outline-none focus:ring-2
              ${
                canConfirm
                  ? isDestructive
                    ? 'bg-red-600 hover:bg-red-500 text-white focus:ring-red-500/40'
                    : 'bg-brand-gold hover:bg-brand-amber text-brand-dark focus:ring-brand-gold/40'
                  : 'bg-brand-border text-brand-muted cursor-not-allowed'
              }
            `}
            aria-disabled={!canConfirm}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
