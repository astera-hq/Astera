'use client';

import { TransactionStatusPanel } from '@/components/TransactionStatus';
import { RealTimeNotificationProvider } from '@/components/RealTimeNotificationProvider';
import ToastHost from '@/components/Toast';

export function ClientShell({ children }: { children: React.ReactNode }) {
  return (
    <>
      <RealTimeNotificationProvider />
      <ToastHost />
      {children}
      <TransactionStatusPanel />
    </>
  );
}
