import { NextResponse } from 'next/server';

export const dynamic = 'force-dynamic';

type Body = {
  email?: {
    enabled?: boolean;
    email?: string;
    events?: string[];
  };
  webhook?: {
    enabled?: boolean;
    url?: string;
    events?: string[];
  };
};

export async function POST(req: Request) {
  let body: Body | null = null;
  try {
    body = (await req.json()) as Body;
  } catch {
    return NextResponse.json({ ok: false, error: 'Invalid JSON body.' }, { status: 400 });
  }

  const emailEnabled = Boolean(body?.email?.enabled);
  const email = typeof body?.email?.email === 'string' ? body.email.email : '';
  const webhookEnabled = Boolean(body?.webhook?.enabled);
  const webhookUrl = typeof body?.webhook?.url === 'string' ? body.webhook.url : '';

  // Mock persistence: accept and validate basic structure.
  if (emailEnabled && !email) {
    return NextResponse.json(
      { ok: false, error: 'Email is required when email notifications are enabled.' },
      { status: 400 },
    );
  }
  if (webhookEnabled && !webhookUrl) {
    return NextResponse.json(
      { ok: false, error: 'Webhook URL is required when webhook forwarding is enabled.' },
      { status: 400 },
    );
  }

  return NextResponse.json({
    ok: true,
    received: {
      email: {
        enabled: emailEnabled,
        email,
        events: Array.isArray(body?.email?.events) ? body!.email!.events : [],
      },
      webhook: {
        enabled: webhookEnabled,
        url: webhookUrl,
        events: Array.isArray(body?.webhook?.events) ? body!.webhook!.events : [],
      },
    },
    note: 'Mock endpoint: preferences are not persisted server-side yet.',
  });
}
