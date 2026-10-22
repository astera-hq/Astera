import { NextResponse } from 'next/server';
import { jwtVerify } from 'jose';

export const dynamic = 'force-dynamic';

/**
 * #1550: server-side proxy for the compliance service's admin `GET /flags`.
 *
 * The compliance admin token must never reach the browser, so it is read from
 * the server-only `COMPLIANCE_ADMIN_TOKEN` and attached here. Callers must
 * present a valid SEP-10 JWT whose account is listed in the server-only
 * `COMPLIANCE_ADMIN_ACCOUNTS`; otherwise this route would simply re-expose the
 * admin endpoint to anyone.
 */
export async function GET(request: Request) {
  const jwtSecret = process.env.SEP10_JWT_SECRET || process.env.JWT_SECRET;
  const adminAccounts = (process.env.COMPLIANCE_ADMIN_ACCOUNTS ?? '')
    .split(',')
    .map((a) => a.trim())
    .filter(Boolean);
  if (!jwtSecret || adminAccounts.length === 0) {
    return NextResponse.json({ error: 'not_configured' }, { status: 503 });
  }

  const auth = request.headers.get('authorization') || '';
  const m = auth.match(/^Bearer\s+(.+)$/i);
  if (!m) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });

  let account: string | undefined;
  try {
    const { payload } = await jwtVerify(m[1]!, new TextEncoder().encode(jwtSecret));
    account = payload.sub;
  } catch {
    return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  }
  if (!account) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!adminAccounts.includes(account)) {
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });
  }

  const base =
    process.env.COMPLIANCE_SERVICE_API_URL ??
    process.env.NEXT_PUBLIC_COMPLIANCE_SERVICE_URL ??
    'http://localhost:8081';
  const token = process.env.COMPLIANCE_ADMIN_TOKEN ?? '';
  try {
    const res = await fetch(`${base.replace(/\/$/, '')}/flags`, {
      headers: token ? { 'x-admin-token': token } : {},
      cache: 'no-store',
    });
    if (!res.ok) {
      return NextResponse.json({ error: 'upstream_error' }, { status: 502 });
    }
    const body = (await res.json()) as { alerts?: unknown };
    return NextResponse.json({ alerts: Array.isArray(body.alerts) ? body.alerts : [] });
  } catch {
    return NextResponse.json({ error: 'upstream_unreachable' }, { status: 502 });
  }
}
