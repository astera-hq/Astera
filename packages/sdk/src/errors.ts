/** #984: typed contract error surfaced from a failed simulation/transaction. */
export class ContractError extends Error {
  /** Numeric error code from the contract's `#[contracterror]` enum, if parseable. */
  readonly code: number | null;
  /** Variant name (e.g. "InsufficientRevenue"), if it matched a known error map. */
  readonly variant: string | null;
  /** Raw error string returned by the RPC/simulation. */
  readonly raw: string;

  constructor(raw: string, code: number | null, variant: string | null) {
    super(variant ? `${variant} (contract error #${code})` : raw);
    this.name = 'ContractError';
    this.raw = raw;
    this.code = code;
    this.variant = variant;
  }
}

const CONTRACT_ERROR_CODE_RE = /Error\(Contract,\s*#(\d+)\)/;

/** Parses a Soroban simulation/tx error string into a typed ContractError using the given error map. */
export function parseContractError(
  raw: string,
  errors: Record<number, { message: string }>,
): ContractError {
  const match = raw.match(CONTRACT_ERROR_CODE_RE);
  if (!match) {
    return new ContractError(raw, null, null);
  }
  const code = Number(match[1]);
  const variant = errors[code]?.message ?? null;
  return new ContractError(raw, code, variant);
}
