/**
 * #867: mock sanctions / watchlist screening.
 *
 * Ships a small bundled fixture list (OFAC-SDN-style address stubs) and a
 * pluggable interface so a real provider can be swapped in later.
 */

import type { RiskTier, ScreenDecision, ScreenResult } from './types';

const VALID_STATUSES: readonly ScreenDecision[] = [
  'Unscreened', 'Cleared', 'Flagged', 'Blocked', 'PendingReview',
];
const VALID_RISK_TIERS: readonly RiskTier[] = ['Low', 'Medium', 'High'];

function validateScreenResult(body: unknown): ScreenResult {
  if (body === null || typeof body !== 'object') {
    throw new Error('upstream screening provider returned non-object response');
  }
  const obj = body as Record<string, unknown>;

  if (typeof obj.address !== 'string' || obj.address.length === 0) {
    throw new Error('upstream screening provider returned invalid or missing "address"');
  }
  if (!VALID_STATUSES.includes(obj.status as ScreenDecision)) {
    throw new Error(
      `upstream screening provider returned invalid "status": ${JSON.stringify(obj.status)}`,
    );
  }
  if (typeof obj.reasonCode !== 'number' || !Number.isFinite(obj.reasonCode)) {
    throw new Error(
      `upstream screening provider returned invalid "reasonCode": ${JSON.stringify(obj.reasonCode)}`,
    );
  }
  if (!VALID_RISK_TIERS.includes(obj.riskTier as RiskTier)) {
    throw new Error(
      `upstream screening provider returned invalid "riskTier": ${JSON.stringify(obj.riskTier)}`,
    );
  }
  if (typeof obj.notes !== 'string') {
    throw new Error('upstream screening provider returned invalid or missing "notes"');
  }

  return {
    address: obj.address,
    status: obj.status as ScreenDecision,
    reasonCode: obj.reasonCode,
    riskTier: obj.riskTier as RiskTier,
    matchedList: typeof obj.matchedList === 'string' ? obj.matchedList : undefined,
    notes: obj.notes,
  };
}

/** Demo fixture — Stellar-like public keys used only for local testing. */
export const SANCTIONS_FIXTURE: ReadonlyArray<{ id: string; note: string }> = [
  { id: 'SANCTIONED_ENTITY_ALPHA', note: 'Demo SDN entry A' },
  { id: 'SANCTIONED_ENTITY_BETA', note: 'Demo SDN entry B' },
  { id: 'GBLOCKEDXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX', note: 'Demo blocked G-address prefix' },
];

export interface ScreenerProvider {
  screen(address: string, context?: ScreenContext): Promise<ScreenResult>;
}

export interface ScreenContext {
  /** Optional free-text name / jurisdiction for heuristic tiering. */
  name?: string;
  jurisdiction?: string;
  recentVolume?: bigint;
  debtorConcentrationBps?: number;
}

const DEFAULT_UPSTREAM_TIMEOUT_MS = 5000;

/**
 * #981: real upstream KYC/screening provider over HTTP.
 *
 * Enforces a hard timeout on the outbound call so a hung provider can't
 * block the queue from processing other subjects.
 */
export class HttpScreenerProvider implements ScreenerProvider {
  constructor(
    private readonly baseUrl: string,
    private readonly timeoutMs: number = DEFAULT_UPSTREAM_TIMEOUT_MS,
    private readonly apiKey?: string,
  ) {}

  async screen(address: string, context: ScreenContext = {}): Promise<ScreenResult> {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeoutMs);

    try {
      const res = await fetch(`${this.baseUrl.replace(/\/$/, '')}/screen`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(this.apiKey ? { Authorization: `Bearer ${this.apiKey}` } : {}),
        },
        body: JSON.stringify({ address, ...context, recentVolume: context.recentVolume?.toString() }),
        signal: controller.signal,
      });

      if (!res.ok) {
        throw new Error(`upstream screening provider returned ${res.status}`);
      }
      const body = await res.json();
      return validateScreenResult(body);
    } catch (err) {
      if (err instanceof Error && err.name === 'AbortError') {
        throw new Error(`upstream screening provider timed out after ${this.timeoutMs}ms`);
      }
      throw err;
    } finally {
      clearTimeout(timer);
    }
  }
}

const REASON_CLEAR = 0;
const REASON_SANCTIONS_HIT = 9001;
const REASON_HIGH_RISK_JURISDICTION = 2001;
const REASON_HIGH_VOLUME = 2002;
const REASON_DEBTOR_CONCENTRATION = 2003;

const HIGH_RISK_JURISDICTIONS = new Set(['XX', 'ZZ', 'UNKNOWN']);

export class MockScreener implements ScreenerProvider {
  private readonly blocked = new Set(
    SANCTIONS_FIXTURE.map((e) => e.id.toUpperCase()),
  );

  /** Extra addresses an operator can block at runtime (admin REST). */
  private readonly runtimeBlocked = new Set<string>();

  addRuntimeBlock(address: string): void {
    this.runtimeBlocked.add(address.toUpperCase());
  }

  async screen(address: string, context: ScreenContext = {}): Promise<ScreenResult> {
    const upper = address.toUpperCase();

    if (this.runtimeBlocked.has(upper) || this.blocked.has(upper)) {
      return {
        address,
        status: 'Blocked',
        reasonCode: REASON_SANCTIONS_HIT,
        riskTier: 'High',
        matchedList: 'OFAC-SDN-fixture',
        notes: 'Address matched bundled sanctions fixture or runtime block list',
      };
    }

    // Prefix match against fixture "G..." stubs for demo.
    for (const entry of SANCTIONS_FIXTURE) {
      if (entry.id.startsWith('G') && upper.startsWith(entry.id.slice(0, 8))) {
        return {
          address,
          status: 'Blocked',
          reasonCode: REASON_SANCTIONS_HIT,
          riskTier: 'High',
          matchedList: entry.id,
          notes: entry.note,
        };
      }
    }

    let riskTier: RiskTier = 'Low';
    let status: ScreenDecision = 'Cleared';
    let reasonCode = REASON_CLEAR;
    let notes = 'No sanctions hit';

    if (context.jurisdiction && HIGH_RISK_JURISDICTIONS.has(context.jurisdiction.toUpperCase())) {
      riskTier = 'High';
      status = 'Flagged';
      reasonCode = REASON_HIGH_RISK_JURISDICTION;
      notes = `High-risk jurisdiction: ${context.jurisdiction}`;
    }

    if (context.recentVolume !== undefined && context.recentVolume > 1_000_000_000n) {
      riskTier = riskTier === 'Low' ? 'Medium' : riskTier;
      if (status === 'Cleared') {
        status = 'Flagged';
        reasonCode = REASON_HIGH_VOLUME;
        notes = 'Elevated recent transaction volume';
      }
    }

    if (
      context.debtorConcentrationBps !== undefined &&
      context.debtorConcentrationBps > 5000
    ) {
      riskTier = 'High';
      if (status === 'Cleared') {
        status = 'Flagged';
        reasonCode = REASON_DEBTOR_CONCENTRATION;
        notes = 'High debtor concentration';
      }
    }

    return { address, status, reasonCode, riskTier, notes };
  }
}
