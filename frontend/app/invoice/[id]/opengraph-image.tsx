import { getInvoice } from '@/lib/contracts';
import { OG_IMAGE_SIZE, renderBrandOgImage } from '@/lib/og-image';

export const alt = 'Astera Invoice';
export const size = OG_IMAGE_SIZE;
export const contentType = 'image/png';

// #765: dynamic per-invoice OG image showing amount, token, and status, so
// shared invoice links render a rich preview instead of the generic
// site-wide default.
export default async function Image({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;

  try {
    const invoice = await getInvoice(Number(id));
    const amount = (Number(invoice.amount) / 10_000_000).toLocaleString();
    return renderBrandOgImage(
      `Invoice #${invoice.id} — ${amount} USDC`,
      `Status: ${invoice.status}`,
    );
  } catch {
    return renderBrandOgImage(`Invoice #${id}`, 'Stellar Invoice Financing');
  }
}
