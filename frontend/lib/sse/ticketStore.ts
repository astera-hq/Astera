const ticketStore = new Map<string, { account: string; expiresAt: number }>();

const TICKET_TTL_SECONDS = 300;

setInterval(() => {
  const now = Date.now();
  for (const [ticket, data] of ticketStore.entries()) {
    if (data.expiresAt <= now) {
      ticketStore.delete(ticket);
    }
  }
}, 60_000);

export function generateTicket(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
}

export function createTicket(account: string): { ticket: string; expiresIn: number } {
  const ticket = generateTicket();
  const expiresAt = Date.now() + TICKET_TTL_SECONDS * 1000;
  ticketStore.set(ticket, { account, expiresAt });
  return { ticket, expiresIn: TICKET_TTL_SECONDS };
}

export function consumeTicket(ticket: string): string | null {
  const data = ticketStore.get(ticket);
  if (!data) return null;
  if (data.expiresAt <= Date.now()) {
    ticketStore.delete(ticket);
    return null;
  }
  ticketStore.delete(ticket);
  return data.account;
}

export function validateTicket(ticket: string): string | null {
  const data = ticketStore.get(ticket);
  if (!data) return null;
  if (data.expiresAt <= Date.now()) {
    ticketStore.delete(ticket);
    return null;
  }
  return data.account;
}