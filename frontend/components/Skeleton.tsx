interface SkeletonProps {
  className?: string;
}

export function Skeleton({ className = '' }: SkeletonProps) {
  return (
    <div
      className={`bg-brand-dark/50 animate-pulse rounded ${className}`}
      role="status"
      aria-label="Loading"
    />
  );
}

// StatCard skeleton for metrics (4-column grid)
export function StatCardSkeleton() {
  return (
    <div className="p-4 bg-brand-card border border-brand-border rounded-2xl">
      <Skeleton className="h-4 w-24 mb-3" />
      <Skeleton className="h-8 w-32 mb-2" />
      <Skeleton className="h-3 w-20" />
    </div>
  );
}

// HistoryEvent skeleton for transaction history
export function HistoryEventSkeleton() {
  return (
    <div className="p-4 bg-brand-card border border-brand-border rounded-2xl flex items-start gap-4">
      <Skeleton className="h-7 w-16 shrink-0 rounded-lg" />
      <div className="flex-1 min-w-0">
        <Skeleton className="h-5 w-40 mb-2" />
        <Skeleton className="h-4 w-64" />
      </div>
      <Skeleton className="h-4 w-28 shrink-0" />
    </div>
  );
}

// TableRow skeleton for admin tables
export function TableRowSkeleton({ colSpan = 5 }: { colSpan?: number }) {
  return (
    <tr className="animate-pulse">
      <td colSpan={colSpan} className="px-6 py-4">
        <div className="flex items-center gap-4">
          <Skeleton className="h-5 w-16" />
          <Skeleton className="h-5 w-32" />
          <Skeleton className="h-5 w-24" />
          <Skeleton className="h-5 w-20" />
          <Skeleton className="h-5 w-28" />
        </div>
      </td>
    </tr>
  );
}

// Chart skeleton for analytics charts
export function ChartSkeleton() {
  return (
    <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
      <Skeleton className="h-6 w-32 mb-4" />
      <Skeleton className="h-48 w-full rounded-lg" />
    </div>
  );
}

// Button loading skeleton (inline)
export function ButtonSkeleton({ width = 'w-32' }: { width?: string }) {
  return <Skeleton className={`h-10 ${width} rounded-xl`} />;
}
