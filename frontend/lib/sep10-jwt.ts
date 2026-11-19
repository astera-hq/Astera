export const SEP10_ISSUER = 'astera-sep10';
export const SEP10_AUDIENCE = 'astera-api';
export const SEP10_TTL_SECONDS = 15 * 60;

/** SEP-10 must never share the application's general signing secret. */
export function getSep10Secret(): string | undefined {
  return process.env.SEP10_JWT_SECRET;
}

export function sep10Key(secret: string): Uint8Array {
  return new TextEncoder().encode(secret);
}
