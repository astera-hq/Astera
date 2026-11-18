import { getInvoice } from '@/lib/contracts';
import { OG_IMAGE_SIZE, renderBrandOgImage } from '@/lib/og-image';

export const alt = 'Astera Invoice';
export const size = OG_IMAGE_SIZE;
export const contentType = 'image/png';

// #765: Twitter Card counterpart of ./opengraph-image.tsx — Next.js does not
// fall back to a route's own opengraph-image for the Twitter card, so this
// mirrors it to make sure the per-invoice image (not the site-wide default)
// shows up when invoice links are shared on Twitter/X.
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
