'use client';

import { useEffect, useRef, useState } from 'react';
import { notificationService } from '@/lib/notifications';
import type { NotificationAlert } from '@/lib/notifications';
import { isInAppEnabled } from '@/lib/notification-preferences';

const PRIORITY_COLORS: Record<string, string> = {
  LOW: 'text-brand-muted',
  MEDIUM: 'text-yellow-400',
  HIGH: 'text-orange-400',
  CRITICAL: 'text-red-400',
};

const PRIORITY_BADGE: Record<string, string> = {
  LOW: 'bg-brand-border text-brand-muted',
  MEDIUM: 'bg-yellow-900/40 text-yellow-400 border-yellow-800/50',
  HIGH: 'bg-orange-900/40 text-orange-400 border-orange-800/50',
  CRITICAL: 'bg-red-900/40 text-red-400 border-red-800/50',
};

const MAX_NOTIFICATIONS = 30;

export default function NotificationBell() {
  const [notifications, setNotifications] = useState<NotificationAlert[]>([]);
  const [open, setOpen] = useState(false);
  const [hasNew, setHasNew] = useState(false);
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const unsub = notificationService.subscribe((alert) => {
      if (!isInAppEnabled(alert.type)) return;
      setNotifications((prev) => [alert, ...prev].slice(0, MAX_NOTIFICATIONS));
      setHasNew(true);
    });
    return unsub;
  }, []);

  // Close panel when clicking outside
  useEffect(() => {
    if (!open) return;
    function handleClick(e: MouseEvent) {
      if (panelRef.current && !panelRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener('mousedown', handleClick);
    return () => document.removeEventListener('mousedown', handleClick);
  }, [open]);

  function handleOpen() {
    setOpen((prev) => !prev);
    setHasNew(false);
  }

  function clearAll() {
    setNotifications([]);
    setOpen(false);
  }

  const unreadCount = notifications.length;

  return (
    <div className="relative" ref={panelRef}>
      {/* Bell button */}
      <button
        onClick={handleOpen}
        aria-label={`Notifications${unreadCount > 0 ? ` (${unreadCount} unread)` : ''}`}
        aria-haspopup="true"
        aria-expanded={open}
        className="relative p-2 rounded-lg text-brand-muted hover:text-white hover:bg-brand-card transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-gold"
      >
        {/* Bell icon */}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="18"
          height="18"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" />
          <path d="M13.73 21a2 2 0 0 1-3.46 0" />
        </svg>

        {/* Badge */}
        {hasNew && unreadCount > 0 && (
          <span
            aria-hidden="true"
            className="absolute -top-0.5 -right-0.5 w-4 h-4 flex items-center justify-center rounded-full bg-red-500 text-white text-[9px] font-bold"
          >
            {unreadCount > 9 ? '9+' : unreadCount}
          </span>
        )}
      </button>

      {/* Dropdown panel */}
      {open && (
        <div
          role="dialog"
          aria-label="Notifications panel"
          className="absolute right-0 mt-2 w-80 max-h-[420px] flex flex-col bg-brand-card border border-brand-border rounded-2xl shadow-2xl z-[200] overflow-hidden"
        >
          {/* Header */}
          <div className="flex items-center justify-between px-4 py-3 border-b border-brand-border shrink-0">
            <h2 className="text-white font-semibold text-sm">Notifications</h2>
            {notifications.length > 0 && (
              <button
                onClick={clearAll}
                className="text-xs text-brand-muted hover:text-white transition-colors"
              >
                Clear all
              </button>
            )}
          </div>

          {/* List */}
          <div className="overflow-y-auto flex-1">
            {notifications.length === 0 ? (
              <p className="text-brand-muted text-sm text-center py-8">No notifications yet.</p>
            ) : (
              <ul className="divide-y divide-brand-border">
                {notifications.map((n) => (
                  <li key={n.id} className="px-4 py-3 hover:bg-brand-border/30 transition-colors">
                    <div className="flex items-start gap-2">
                      <span
                        className={`mt-0.5 shrink-0 text-xs border px-1.5 py-0.5 rounded font-medium ${PRIORITY_BADGE[n.priority]}`}
                      >
                        {n.priority}
                      </span>
                      <div className="flex-1 min-w-0">
                        <p
                          className={`text-xs font-medium leading-snug ${PRIORITY_COLORS[n.priority]}`}
                        >
                          {n.type.replace(/_/g, ' ')}
                        </p>
                        <p className="text-brand-muted text-xs mt-0.5 leading-snug">{n.message}</p>
                        <p className="text-brand-muted/60 text-[10px] mt-1">
                          {new Date(n.timestamp).toLocaleTimeString()}
                        </p>
                      </div>
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
