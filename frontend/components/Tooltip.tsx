'use client';

import { useId, useState, type ReactNode } from 'react';

interface TooltipProps {
  /** Tooltip body text. */
  content: string;
  /** Trigger element — must be able to receive keyboard focus. */
  children: ReactNode;
  className?: string;
}

/**
 * Minimal accessible tooltip: shows on hover *and* keyboard focus, hides on
 * Escape/blur/mouse-leave, and links the trigger to the tooltip text via
 * `aria-describedby` so screen readers announce it.
 */
export default function Tooltip({ content, children, className = '' }: TooltipProps) {
  const [visible, setVisible] = useState(false);
  const tooltipId = useId();

  const show = () => setVisible(true);
  const hide = () => setVisible(false);

  return (
    <span
      className={`relative inline-flex ${className}`}
      onMouseEnter={show}
      onMouseLeave={hide}
      onFocus={show}
      onBlur={hide}
      onKeyDown={(e) => {
        if (e.key === 'Escape') hide();
      }}
    >
      <span aria-describedby={visible ? tooltipId : undefined}>{children}</span>
      {visible && (
        <span
          id={tooltipId}
          role="tooltip"
          className="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 w-max max-w-xs px-3 py-2 text-xs leading-relaxed text-white bg-brand-dark border border-brand-border rounded-lg shadow-lg z-50 pointer-events-none"
        >
          {content}
        </span>
      )}
    </span>
  );
}
