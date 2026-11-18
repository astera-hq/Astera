import type { Metadata } from 'next';
import { isInvoicePrivate } from '@/lib/contracts';

// #783: /invoice/[id] is a client component (page.tsx), which can't itself
// export `metadata` — a sibling server-component layout is the standard
// Next.js App Router way to attach metadata to a client page. A private
// invoice (#775 opt-out) still resolves at this URL for anyone holding the
// direct link, but is marked `noindex` and given no canonical tag so search
// engines don't surface or dedupe it as a public page.
export async function generateMetadata({
  params,
}: {
  params: Promise<{ id: string }>;
}): Promise<Metadata> {
  const { id } = await params;

  let isPrivate = false;
  try {
    isPrivate = await isInvoicePrivate(Number(id));
  } catch {
    // Contract unreachable or invoice not found — fall through to the
    // public default rather than blocking the page from rendering.
  }

  if (isPrivate) {
    return { robots: { index: false, follow: false } };
  }

  return { alternates: { canonical: `/invoice/${id}` } };
}

export default function InvoiceLayout({ children }: { children: React.ReactNode }) {
  return children;
}
