import { NextResponse } from 'next/server';
import { jwtVerify } from 'jose';
import { getSep10Secret, SEP10_AUDIENCE, SEP10_ISSUER, sep10Key } from '@/lib/sep10-jwt';

export const dynamic = 'force-dynamic';

const JWT_SECRET = getSep10Secret();

export async function GET(request: Request) {
  try {
    if (!JWT_SECRET) return NextResponse.json({ error: 'not_configured' }, { status: 500 });

    const auth = request.headers.get('authorization') || '';
    const m = auth.match(/^Bearer\s+(.+)$/i);
    if (!m) return NextResponse.json({ authenticated: false }, { status: 401 });

    const token = m[1]!;
    const { payload } = await jwtVerify(token, sep10Key(JWT_SECRET), {
      issuer: SEP10_ISSUER,
      audience: SEP10_AUDIENCE,
    });
    const sub = payload.sub as string | undefined;
    if (!sub) return NextResponse.json({ authenticated: false }, { status: 401 });

    return NextResponse.json({ authenticated: true, account: sub });
  } catch {
    return NextResponse.json({ authenticated: false }, { status: 401 });
  }
}
