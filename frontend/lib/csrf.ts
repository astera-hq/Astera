const CSRF_COOKIE_NAME = 'csrf_token';
const CSRF_HEADER_NAME = 'x-csrf-token';

/** Reads the CSRF token minted by `middleware.ts` into `document.cookie`. */
export function getCsrfToken(): string | null {
  if (typeof document === 'undefined') return null;
  const match = document.cookie.match(new RegExp(`(?:^|; )${CSRF_COOKIE_NAME}=([^;]*)`));
  return match ? decodeURIComponent(match[1]!) : null;
}

/**
 * `fetch` wrapper that echoes the CSRF cookie back as a header, satisfying
 * the double-submit check in `middleware.ts` for state-mutating requests.
 * Use for every POST/PUT/PATCH/DELETE to a same-origin `/api/*` route.
 */
export function csrfFetch(input: RequestInfo | URL, init: RequestInit = {}): Promise<Response> {
  const token = getCsrfToken();
  const headers = new Headers(init.headers);
  if (token) headers.set(CSRF_HEADER_NAME, token);
  return fetch(input, { ...init, headers });
}
