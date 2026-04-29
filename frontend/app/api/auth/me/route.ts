import { NextResponse } from 'next/server';
import { jwtVerify } from 'jose';

export const dynamic = 'force-dynamic';

const JWT_SECRET = process.env.SEP10_JWT_SECRET || process.env.JWT_SECRET;

export async function GET(request: Request) {
  try {
    if (!JWT_SECRET) return NextResponse.json({ error: 'not_configured' }, { status: 500 });

    const auth = request.headers.get('authorization') || '';
    const m = auth.match(/^Bearer\s+(.+)$/i);
    if (!m) return NextResponse.json({ authenticated: false }, { status: 401 });

    const token = m[1];
    const key = new TextEncoder().encode(JWT_SECRET);
    const { payload } = await jwtVerify(token, key);
    const sub = payload.sub as string | undefined;
    if (!sub) return NextResponse.json({ authenticated: false }, { status: 401 });

    return NextResponse.json({ authenticated: true, account: sub });
  } catch (err) {
    return NextResponse.json({ authenticated: false }, { status: 401 });
  }
}
