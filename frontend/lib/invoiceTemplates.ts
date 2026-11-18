export interface InvoiceTemplate {
  id: string;
  name: string;
  amount: number;
  token: string;
  dueDays: number;
  description: string;
  createdAt: string;
  lastUsedAt: string | null;
}

const STORAGE_KEY = 'astera.invoiceTemplates';

export function loadInvoiceTemplates(): InvoiceTemplate[] {
  if (typeof window === 'undefined') return [];
  try {
    const value: unknown = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? '[]');
    return Array.isArray(value) ? (value as InvoiceTemplate[]) : [];
  } catch {
    return [];
  }
}

export function saveInvoiceTemplates(templates: InvoiceTemplate[]): void {
  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(templates));
}

export function upsertInvoiceTemplate(template: InvoiceTemplate): void {
  const templates = loadInvoiceTemplates();
  const index = templates.findIndex((item) => item.id === template.id);
  if (index >= 0) templates[index] = template;
  else templates.unshift(template);
  saveInvoiceTemplates(templates);
}

export function deleteInvoiceTemplate(id: string): void {
  saveInvoiceTemplates(loadInvoiceTemplates().filter((template) => template.id !== id));
}

export function getInvoiceTemplate(id: string): InvoiceTemplate | null {
  return loadInvoiceTemplates().find((template) => template.id === id) ?? null;
}
