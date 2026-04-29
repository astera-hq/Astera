import { NextResponse } from 'next/server';

export const dynamic = 'force-dynamic';

const TIMEOUT_MS = 6000;

function isValidUrl(url: string): boolean {
  try {
    const u = new URL(url);
    return u.protocol === 'https:' || u.protocol === 'http:';
  } catch {
    return false;
  }
}

export async function POST(req: Request) {
  let body: unknown = null;
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ ok: false, error: 'Invalid JSON body.' }, { status: 400 });
  }

  const url =
    typeof (body as { url?: unknown })?.url === 'string' ? (body as { url: string }).url : '';
  if (!isValidUrl(url)) {
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
