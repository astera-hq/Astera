import type { Invoice } from '@/lib/types';

export const DASHBOARD_VIEW_MODES = {
  LIST: 'list',
  PIPELINE: 'pipeline',
} as const;

export type DashboardViewMode =
  (typeof DASHBOARD_VIEW_MODES)[keyof typeof DASHBOARD_VIEW_MODES];

export const DASHBOARD_VIEW_STORAGE_KEY = 'astera_dashboard_view_mode';

export type DashboardRowLike = {
  invoice: Invoice;
  metadata: {
    amount: bigint;
    debtor: string;
    dueDate: number;
    name: string;
  };
};

export type PipelineColumnKey =
  | 'pending'
  | 'awaitingVerification'
  | 'verified'
  | 'funded'
  | 'paid';

export type PipelineColumnDefinition = {
  key: PipelineColumnKey;
  labelKey: string;
  statuses: Invoice['status'][];
};

export const PIPELINE_COLUMN_DEFINITIONS: PipelineColumnDefinition[] = [
  { key: 'pending', labelKey: 'pipeline.columns.pending', statuses: ['Pending'] },
  {
    key: 'awaitingVerification',
    labelKey: 'pipeline.columns.awaitingVerification',
    statuses: ['AwaitingVerification'],
  },
  { key: 'verified', labelKey: 'pipeline.columns.verified', statuses: ['Verified'] },
  { key: 'funded', labelKey: 'pipeline.columns.funded', statuses: ['Funded'] },
  { key: 'paid', labelKey: 'pipeline.columns.paid', statuses: ['Paid'] },
];

export const ATTENTION_STATUSES: Invoice['status'][] = ['Disputed', 'Defaulted', 'Expired'];

export function isOverdueInvoice(row: DashboardRowLike, nowUnix: number): boolean {
  return row.invoice.status !== 'Paid' && row.metadata.dueDate < nowUnix;
}

export function isAttentionInvoice(row: DashboardRowLike, nowUnix: number): boolean {
  return ATTENTION_STATUSES.includes(row.invoice.status) || isOverdueInvoice(row, nowUnix);
}

