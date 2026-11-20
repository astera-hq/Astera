const usedNonces = new Set<string>();
const MAX_NONCES = 10000;
const CLEANUP_INTERVAL = 300000;

let cleanupTimer: NodeJS.Timeout | null = null;

function startCleanup() {
  if (cleanupTimer) return;
  cleanupTimer = setInterval(() => {
    if (usedNonces.size > MAX_NONCES) {
      const toRemove = Math.floor(usedNonces.size / 2);
      const iterator = usedNonces.values();
      for (let i = 0; i < toRemove; i++) {
        const { value } = iterator.next();
        if (value) usedNonces.delete(value);
      }
    }
  }, CLEANUP_INTERVAL);
}

export function markNonceUsed(nonce: string): boolean {
  if (!cleanupTimer) startCleanup();
  if (usedNonces.has(nonce)) return false;
  usedNonces.add(nonce);
  return true;
}

export function isNonceUsed(nonce: string): boolean {
  return usedNonces.has(nonce);
}
