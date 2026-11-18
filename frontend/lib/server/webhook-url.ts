import dns from 'dns/promises';

type LookupAll = (
  hostname: string,
  options: { all: true },
) => Promise<readonly { address: string; family: number }[]>;

// RFC 1918 private ranges, loopback, link-local, cloud metadata, and
// unspecified. Applied to both IPv4 and (after decoding) IPv4-mapped IPv6.
const PRIVATE_IPV4_PATTERNS = [
  /^127\./,
  /^10\./,
  /^172\.(1[6-9]|2\d|3[01])\./,
  /^192\.168\./,
  /^169\.254\./,
  /^0\./,
];

const PRIVATE_IPV6_PATTERNS = [/^::1$/, /^fc00:/i, /^fe80:/i];

const BLOCKED_HOSTNAMES = new Set([
  '169.254.169.254',
  'metadata.google.internal',
  'metadata.goog',
  'instance-data',
]);

function decodeMappedIPv4(address: string): string | null {
  const match = address.match(/^::ffff:([0-9a-f]+):([0-9a-f]+)$/i);
  if (!match) return null;
  const high = parseInt(match[1]!, 16);
  const low = parseInt(match[2]!, 16);
  return `${high >> 8}.${high & 0xff}.${low >> 8}.${low & 0xff}`;
}

function isPrivateAddress(address: string, family: number): boolean {
  if (family === 4) return PRIVATE_IPV4_PATTERNS.some((pattern) => pattern.test(address));
  if (PRIVATE_IPV6_PATTERNS.some((pattern) => pattern.test(address))) return true;
  const mapped = decodeMappedIPv4(address);
  return mapped !== null && PRIVATE_IPV4_PATTERNS.some((pattern) => pattern.test(mapped));
}

/** Validate a webhook destination before making any outbound request. */
export async function isSafeWebhookUrl(
  rawUrl: string,
  lookup: LookupAll = (hostname, options) => dns.lookup(hostname, options),
): Promise<boolean> {
  let parsed: URL;
  try {
    parsed = new URL(rawUrl);
  } catch {
    return false;
  }

  if (parsed.protocol !== 'https:') return false;

  const hostname = parsed.hostname.toLowerCase().replace(/^\[|\]$/g, '');
  if (BLOCKED_HOSTNAMES.has(hostname)) return false;

  try {
    const addresses = await lookup(hostname, { all: true });
    return (
      addresses.length > 0 &&
      !addresses.some(({ address, family }) => isPrivateAddress(address, family))
    );
  } catch {
    // Fail closed when the destination cannot be resolved.
    return false;
  }
}
