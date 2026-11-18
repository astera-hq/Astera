import { NextResponse } from 'next/server';
import type { NextRequest } from 'next/server';

// #770: CSRF protection for state-mutating API routes.
//
// Astera has no cookie-based session today — the SEP-10 flow issues a JWT
// that the client stores and sends as `Authorization: Bearer <token>`, which
// a cross-site request cannot forge (browsers don't attach custom headers to
// cross-origin requests automatically). Two routes rely on that instead of a
// CSRF token:
//   - /api/auth/challenge issues a stateless, unauthenticated SEP-10 nonce
//     scoped to the account the caller names; it has no side effect worth
//     forging.
//   - /api/auth/token only succeeds if the request body carries a
//     transaction signed by the claimed account's private key — the
//     signature itself is the anti-forgery proof.
// Every other state-mutating /api/* route is protected with the
// double-submit cookie pattern below.
const CSRF_COOKIE = 'csrf_token';
const CSRF_HEADER = 'x-csrf-token';
const MUTATING_METHODS = new Set(['POST', 'PUT', 'PATCH', 'DELETE']);
const WALLET_SIGNATURE_EXEMPT = new Set(['/api/auth/challenge', '/api/auth/token']);
const SUPPORTED_LOCALES = new Set(['en', 'fr', 'es']);

function generateToken(): string {
  return crypto.randomUUID().replace(/-/g, '') + crypto.randomUUID().replace(/-/g, '');
}

function extractLocaleFromPath(pathname: string): string | null {
  const segments = pathname.split('/').filter(Boolean);
  if (segments.length > 0 && SUPPORTED_LOCALES.has(segments[0])) {
    return segments[0];
  }
  return null;
}

export function middleware(request: NextRequest) {
  const { pathname } = request.nextUrl;
  const cookieToken = request.cookies.get(CSRF_COOKIE)?.value;
  const pathLocale = extractLocaleFromPath(pathname);
  const localeCookie = request.cookies.get('NEXT_LOCALE')?.value;

  if (
    pathname.startsWith('/api/') &&
    MUTATING_METHODS.has(request.method) &&
    !WALLET_SIGNATURE_EXEMPT.has(pathname)
  ) {
    const headerToken = request.headers.get(CSRF_HEADER);
    if (!headerToken || !cookieToken || headerToken !== cookieToken) {
      return NextResponse.json({ error: 'invalid_csrf_token' }, { status: 403 });
    }
  }

  const response = NextResponse.next();

  // #971: Preserve requested locale on deep link navigations by setting
  // NEXT_LOCALE cookie when navigating directly to a locale-prefixed route.
  if (pathLocale && localeCookie !== pathLocale) {
    response.cookies.set('NEXT_LOCALE', pathLocale, {
      path: '/',
      maxAge: 31536000,
      sameSite: 'lax',
    });
  }

  // Mint a token on first contact (session start) so the frontend has one to
  // echo back by the time it makes its first mutating request.
  if (!cookieToken) {
    response.cookies.set(CSRF_COOKIE, generateToken(), {
      httpOnly: false,
      sameSite: 'lax',
      secure: process.env.NODE_ENV === 'production',
      path: '/',
    });
  }

  return response;
}

export const config = {
  // Run on every navigable route and API call (excluding static/image
  // assets) so the CSRF cookie is minted well before the first mutation.
  matcher: ['/((?!_next/static|_next/image|favicon.ico).*)'],
};
