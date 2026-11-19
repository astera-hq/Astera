import { SignJWT, jwtVerify } from 'jose';

const TICKET_TTL_SECONDS = 300;

function getTicketSecret(): Uint8Array | null {
  const secret = process.env.SEP10_JWT_SECRET;
  if (!secret) return null;
  return new TextEncoder().encode(secret);
}

export async function createTicket(account: string): Promise<{ ticket: string; expiresIn: number } | null> {
  const secret = getTicketSecret();
  if (!secret) return null;

  const now = Math.floor(Date.now() / 1000);
  const exp = now + TICKET_TTL_SECONDS;

  const ticket = await new SignJWT({ sub: account })
    .setProtectedHeader({ alg: 'HS256' })
    .setAudience('sse')
    .setJti(crypto.randomUUID())
    .setIssuedAt(now)
    .setExpirationTime(exp)
    .sign(secret);

  return { ticket, expiresIn: TICKET_TTL_SECONDS };
}

export async function consumeTicket(ticket: string): Promise<string | null> {
  const secret = getTicketSecret();
  if (!secret) return null;

  try {
    const { payload } = await jwtVerify(ticket, secret, {
      audience: 'sse',
    });
    return payload.sub || null;
  } catch {
    return null;
  }
}

export async function validateTicket(ticket: string): Promise<string | null> {
  const secret = getTicketSecret();
  if (!secret) return null;

  try {
    const { payload } = await jwtVerify(ticket, secret, {
      audience: 'sse',
    });
    return payload.sub || null;
  } catch {
    return null;
  }
}
