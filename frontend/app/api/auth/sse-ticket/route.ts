import { NextResponse } from 'next/server';
import { jwtVerify } from 'jose';
import { createTicket } from '@/lib/sse/ticketStore';

export const dynamic = 'force-dynamic';

const JWT_SECRET = process.env.SEP10_JWT_SECRET || process.env.JWT_SECRET;

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
    const key = new TextEncoder().encode(JWT_SECRET);
    const { payload } = await jwtVerify(token, key);
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