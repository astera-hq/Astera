import { NextResponse } from 'next/server';
import { isSafeWebhookUrl } from '@/lib/server/webhook-url';
import { checkRateLimit, getClientIdentifier, getRateLimitHeaders } from '@/lib/rateLimiter';

export const dynamic = 'force-dynamic';

const TIMEOUT_MS = 6000;

export async function POST(req: Request) {
  const identifier = getClientIdentifier(req);
  const rateLimitResult = checkRateLimit(identifier, { maxRequests: 5, windowMs: 60_000 });

  if (!rateLimitResult.allowed) {
    return NextResponse.json(
      { ok: false, error: 'Too many requests' },
      { status: 429, headers: getRateLimitHeaders(rateLimitResult) },
    );
  }

  let body: unknown = null;
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ ok: false, error: 'Invalid JSON body' }, { status: 400 });
  }

  const url =
    typeof (body as { url?: unknown })?.url === 'string' ? (body as { url: string }).url : '';

  if (!(await isSafeWebhookUrl(url))) {
    return NextResponse.json({ ok: false, error: 'Invalid webhook URL' }, { status: 400 });
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
      redirect: 'error',
    });

    if (!res.ok) {
      return NextResponse.json(
        { ok: false, error: `Webhook responded with HTTP ${res.status}` },
        { status: 502 },
      );
    }

    return NextResponse.json({ ok: true }, { headers: getRateLimitHeaders(rateLimitResult) });
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : 'Webhook request failed';
    return NextResponse.json({ ok: false, error: msg }, { status: 502 });
  } finally {
    clearTimeout(timer);
  }
}
