import { NextResponse } from 'next/server';
import { jwtVerify } from 'jose';
import { getSep10Secret, SEP10_AUDIENCE, SEP10_ISSUER, sep10Key } from '@/lib/sep10-jwt';

export const dynamic = 'force-dynamic';

const JWT_SECRET = getSep10Secret();

type AlertPayload = {
  type: string;
  priority: string;
  message: string;
  timestamp: number;
  data?: Record<string, unknown>;
};

export async function POST(request: Request) {
  const webhookUrl = process.env.ALERT_WEBHOOK_URL;
  if (!webhookUrl) {
    return NextResponse.json({ ok: false, error: 'Webhook not configured' }, { status: 503 });
  }

  if (!JWT_SECRET) {
    return NextResponse.json({ ok: false, error: 'Server not configured' }, { status: 500 });
  }

  const auth = request.headers.get('authorization') || '';
  const m = auth.match(/^Bearer\s+(.+)$/i);
  if (!m) {
    return NextResponse.json({ ok: false, error: 'Unauthorized' }, { status: 401 });
  }

  try {
    await jwtVerify(m[1]!, sep10Key(JWT_SECRET), {
      issuer: SEP10_ISSUER,
      audience: SEP10_AUDIENCE,
    });
  } catch {
    return NextResponse.json({ ok: false, error: 'Invalid token' }, { status: 401 });
  }

  let body: AlertPayload;
  try {
    body = (await request.json()) as AlertPayload;
  } catch {
    return NextResponse.json({ ok: false, error: 'Invalid JSON' }, { status: 400 });
  }

  const payload = {
    text: `*${body.priority} ALERT: ${body.type}*`,
    attachments: [
      {
        color: body.priority === 'CRITICAL' ? '#ff0000' : '#ffa500',
        fields: [
          { title: 'Message', value: body.message, short: false },
          { title: 'Time', value: new Date(body.timestamp).toISOString(), short: true },
          { title: 'Context', value: JSON.stringify(body.data || {}), short: false },
        ],
      },
    ],
  };

  try {
    const response = await fetch(webhookUrl, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });

    if (!response.ok) {
      throw new Error(`Webhook returned ${response.status}`);
    }

    return NextResponse.json({ ok: true });
  } catch (error) {
    console.error('[Webhook Dispatch] Failed:', error);
    return NextResponse.json(
      { ok: false, error: 'Webhook dispatch failed' },
      { status: 502 },
    );
  }
}
