/**
 * Contract-ID resolution and validation for E2E tests (#395).
 *
 * E2E tests previously ran against a zeroed placeholder contract ID
 * (`CAAAA…D2KM`), so they never exercised a real deployed contract and gave
 * false confidence. This module resolves the IDs from the environment and, in
 * CI, fails fast if they are missing or still a placeholder.
 */

/** Zeroed contract address historically used as a placeholder. */
const ZERO_CONTRACT_ID = 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM';

/** A Stellar contract ID is 56 chars and starts with `C`. */
function isWellFormed(id: string | undefined): id is string {
  return !!id && id.length === 56 && id.startsWith('C');
}

/** True when an ID is missing, malformed, or an obvious zeroed placeholder. */
export function isPlaceholderContractId(id: string | undefined): boolean {
  if (!isWellFormed(id)) return true;
  if (id === ZERO_CONTRACT_ID) return true;
  // Any near-all-`A` body is a zeroed/placeholder address.
  if (/^CA{40,}/.test(id)) return true;
  return false;
}

/**
 * Resolve a contract ID, accepting either the bare name (e.g.
 * `INVOICE_CONTRACT_ID`, as exported by the CI deploy step) or the
 * `NEXT_PUBLIC_`-prefixed variant the frontend build consumes.
 */
function resolve(name: string): string | undefined {
  return process.env[name] ?? process.env[`NEXT_PUBLIC_${name}`];
}

export const INVOICE_CONTRACT_ID = resolve('INVOICE_CONTRACT_ID');
export const POOL_CONTRACT_ID = resolve('POOL_CONTRACT_ID');

/** True only when real (non-placeholder) invoice + pool IDs are configured. */
export const hasRealContractIds =
  !isPlaceholderContractId(INVOICE_CONTRACT_ID) && !isPlaceholderContractId(POOL_CONTRACT_ID);

/**
 * In CI, real testnet contract IDs are mandatory — throw with an actionable
 * message otherwise so the run fails loudly instead of silently skipping
 * contract interaction.
 */
export function assertRealContractIds(): void {
  const missing: string[] = [];
  if (isPlaceholderContractId(INVOICE_CONTRACT_ID)) missing.push('INVOICE_CONTRACT_ID');
  if (isPlaceholderContractId(POOL_CONTRACT_ID)) missing.push('POOL_CONTRACT_ID');
  if (missing.length > 0) {
    throw new Error(
      `E2E contract IDs are missing or placeholders: ${missing.join(', ')}.\n` +
        'Set real deployed testnet contract IDs before running E2E in CI — e.g. the ' +
        'repository variables TESTNET_INVOICE_CONTRACT_ID / TESTNET_POOL_CONTRACT_ID, ' +
        'wired into NEXT_PUBLIC_INVOICE_CONTRACT_ID / NEXT_PUBLIC_POOL_CONTRACT_ID. ' +
        'See .github/workflows/ci.yml (e2e-tests job).',
    );
  }
}
