import { NextResponse } from 'next/server';
import { jwtVerify } from 'jose';
import { getSep10Secret, SEP10_AUDIENCE, SEP10_ISSUER, sep10Key } from '@/lib/sep10-jwt';
import {
  savePreferences,
  getPreferences,
  deletePreferences,
  type NotificationPreferences,
} from '@/lib/notificationPrefsStore';

export const dynamic = 'force-dynamic';

const JWT_SECRET = getSep10Secret();

async function authenticateRequest(request: Request): Promise<string | null> {
  if (!JWT_SECRET) return null;

  const auth = request.headers.get('authorization') || '';
  const m = auth.match(/^Bearer\s+(.+)$/i);
  if (!m) return null;

  try {
    const { payload } = await jwtVerify(m[1]!, sep10Key(JWT_SECRET), {
      issuer: SEP10_ISSUER,
      audience: SEP10_AUDIENCE,
    });
    return (payload.sub as string) || null;
  } catch {
    return null;
  }
}

export async function GET(request: Request) {
  const account = await authenticateRequest(request);
  if (!account) {
    return NextResponse.json({ ok: false, error: 'Unauthorized' }, { status: 401 });
  }

  const prefs = await getPreferences(account);
  if (!prefs) {
    return NextResponse.json({ ok: true, preferences: null });
  }

  return NextResponse.json({ ok: true, preferences: prefs });
}

export async function POST(request: Request) {
  const account = await authenticateRequest(request);
  if (!account) {
    return NextResponse.json({ ok: false, error: 'Unauthorized' }, { status: 401 });
  }

  let body: Partial<NotificationPreferences> | null = null;
  try {
    body = (await request.json()) as Partial<NotificationPreferences>;
  } catch {
    return NextResponse.json({ ok: false, error: 'Invalid JSON body' }, { status: 400 });
  }

  const emailEnabled = Boolean(body?.email?.enabled);
  const email = typeof body?.email?.email === 'string' ? body.email.email : '';
  const webhookEnabled = Boolean(body?.webhook?.enabled);
  const webhookUrl = typeof body?.webhook?.url === 'string' ? body.webhook.url : '';

  if (emailEnabled && !email) {
    return NextResponse.json(
      { ok: false, error: 'Email is required when email notifications are enabled' },
      { status: 400 },
    );
  }
  if (webhookEnabled && !webhookUrl) {
    return NextResponse.json(
      { ok: false, error: 'Webhook URL is required when webhook forwarding is enabled' },
      { status: 400 },
    );
  }

  const prefs: NotificationPreferences = {
    email: {
      enabled: emailEnabled,
      email,
      events: Array.isArray(body?.email?.events) ? body.email.events : [],
    },
    webhook: {
      enabled: webhookEnabled,
      url: webhookUrl,
      events: Array.isArray(body?.webhook?.events) ? body.webhook.events : [],
    },
  };

  await savePreferences(account, prefs);

  return NextResponse.json({
    ok: true,
    preferences: prefs,
  });
}

export async function DELETE(request: Request) {
  const account = await authenticateRequest(request);
  if (!account) {
    return NextResponse.json({ ok: false, error: 'Unauthorized' }, { status: 401 });
  }

  const deleted = await deletePreferences(account);

  return NextResponse.json({ ok: true, deleted });
}
