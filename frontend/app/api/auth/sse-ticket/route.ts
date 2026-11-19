import { NextResponse } from 'next/server';
import { jwtVerify } from 'jose';
import { createTicket } from '@/lib/sse/ticketStore';
import { getSep10Secret, SEP10_AUDIENCE, SEP10_ISSUER, sep10Key } from '@/lib/sep10-jwt';

export const dynamic = 'force-dynamic';

const JWT_SECRET = getSep10Secret();

export async function GET(request: Request) {
  if (!JWT_SECRET) {
    return NextResponse.json({ error: 'Server not configured' }, { status: 500 });
  }

  const auth = request.headers.get('authorization') || '';
  const m = auth.match(/^Bearer\s+(.+)$/i);
  if (!m) {
    return NextResponse.json({ error: 'Missing or invalid Authorization header' }, { status: 401 });
  }

  const token = m[1]!;

  try {
    const { payload } = await jwtVerify(token, sep10Key(JWT_SECRET), {
      issuer: SEP10_ISSUER,
      audience: SEP10_AUDIENCE,
    });
    const sub = payload.sub as string | undefined;
    if (!sub) {
      return NextResponse.json({ error: 'Invalid token payload' }, { status: 401 });
    }

    const { ticket, expiresIn } = createTicket(sub);

    return NextResponse.json({ ticket, expires_in: expiresIn });
  } catch {
    return NextResponse.json({ error: 'Invalid or expired token' }, { status: 401 });
  }
}
