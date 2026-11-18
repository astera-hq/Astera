import { NextResponse } from 'next/server';
import { isSafeWebhookUrl } from '@/lib/server/webhook-url';

export const dynamic = 'force-dynamic';

const TIMEOUT_MS = 6000;

export async function POST(req: Request) {
  let body: unknown = null;
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ ok: false, error: 'Invalid JSON body.' }, { status: 400 });
  }

  const url =
    typeof (body as { url?: unknown })?.url === 'string' ? (body as { url: string }).url : '';

  // Use a single generic error for all validation/SSRF rejections so callers
  // cannot distinguish "bad URL format" from "blocked internal address".
  if (!(await isSafeWebhookUrl(url))) {
    return NextResponse.json({ ok: false, error: 'Invalid webhook URL.' }, { status: 400 });
  }

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), TIMEOUT_MS);

  try {
    const payload = {
      event: 'TEST_WEBHOOK',
      message: 'This is a test webhook from Astera (notification settings).',
      timestamp: new Date().toISOString(),
      data: {
        exampleAlert: {
          type: 'INVOICE_FUNDED',
          priority: 'MEDIUM',
          message: 'Invoice #123 has been funded for 2500 USDC.',
        },
      },
    };

    const res = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
      signal: controller.signal,
      // Prevent open-redirect SSRF: if the server returns a 3xx the request
      // fails immediately rather than following the Location header to a
      // potentially internal URL that bypassed our isSafeWebhookUrl check.
      redirect: 'error',
    });

    if (!res.ok) {
      return NextResponse.json(
        { ok: false, error: `Webhook responded with HTTP ${res.status}.` },
        { status: 502 },
      );
    }

    return NextResponse.json({ ok: true });
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : 'Webhook request failed.';
    return NextResponse.json({ ok: false, error: msg }, { status: 502 });
  } finally {
    clearTimeout(timer);
  }
}
