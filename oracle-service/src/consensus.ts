import { RoundStatus } from './types';

export interface TrackedRound {
  invoiceId: string;
  status: RoundStatus;
  votedByThisNode: boolean;
  updatedAt: number;
}

// Terminal statuses: once a round reaches one of these it will never change
// again, so it's safe to evict from memory after a retention window.
const TERMINAL_STATUSES: ReadonlySet<RoundStatus> = new Set([
  'ConsensusApproved',
  'ConsensusRejected',
  'Expired',
]);

// #1178: `rounds`/`votedByMe` used to grow forever — every invoice ever
// verified stayed in memory for the lifetime of the process. These bound
// that growth: a finalized round is evicted `FINALIZED_RETENTION_MS` after
// it reaches a terminal state (giving the health endpoint a window to still
// report on recently-finalized rounds), and `MAX_FINALIZED_ROUNDS` is a hard
// cap enforced FIFO so a burst of finalizations can't blow past the
// retention window before the next prune runs.
const FINALIZED_RETENTION_MS = 60 * 60 * 1000; // 1 hour
const MAX_FINALIZED_ROUNDS = 5000;

/**
 * #861: tracks `VerificationRound` state for the oracle registry this node
 * participates in, purely from the events streamed by `listener.ts` (no
 * separate polling loop). Used so the health endpoint can report which
 * rounds are open, which this node has already voted on (so a restart
 * doesn't cause it to blindly re-vote and hit `AlreadyVoted`), and which
 * have finalized — without needing a persisted database for a reference
 * implementation.
 */
export class ConsensusTracker {
  private rounds = new Map<string, TrackedRound>();
  private votedByMe = new Set<string>();
  private paused = false;

  // #1178: FIFO queue of invoiceIds in the order they reached a terminal
  // state, so eviction can cheaply find "oldest finalized round" without
  // scanning `rounds`. An invoiceId appears at most once (re-finalizing,
  // e.g. `consensus` followed by `fallback`, moves it to the back instead
  // of duplicating it).
  private finalizedOrder: string[] = [];
  private finalizedSet = new Set<string>();

  constructor(
    private readonly oraclePublicKey: string,
    private readonly options: { finalizedRetentionMs?: number; maxFinalizedRounds?: number } = {},
  ) {}

  /** Call with the decoded `(topic1, topic2)` pair and event value for every
   * event emitted under the registry contract's "ORACLE" topic namespace. */
  handleEvent(topic2: string, value: unknown): void {
    switch (topic2) {
      case 'paused': {
        this.paused = true;
        break;
      }
      case 'unpaused': {
        this.paused = false;
        break;
      }
      case 'rnd_open': {
        const [invoiceId] = asArray(value);
        this.upsert(String(invoiceId), 'Open');
        break;
      }
      case 'voted': {
        const [invoiceId, oracle] = asArray(value);
        if (String(oracle) === this.oraclePublicKey) {
          this.votedByMe.add(String(invoiceId));
        }
        this.upsert(String(invoiceId), this.rounds.get(String(invoiceId))?.status ?? 'Open');
        break;
      }
      case 'consensus': {
        const [invoiceId, approved] = asArray(value);
        this.upsert(String(invoiceId), approved ? 'ConsensusApproved' : 'ConsensusRejected');
        break;
      }
      case 'rnd_exp': {
        const invoiceId = Array.isArray(value) ? value[0] : value;
        this.upsert(String(invoiceId), 'Expired');
        break;
      }
      case 'fallback': {
        const [invoiceId, approved] = asArray(value);
        this.upsert(String(invoiceId), approved ? 'ConsensusApproved' : 'ConsensusRejected');
        break;
      }
      default:
        break;
    }
  }

  /** Whether this node has already cast a vote on `invoiceId` — used to skip
   * re-voting after a restart rather than eating an `AlreadyVoted` error. */
  hasVoted(invoiceId: bigint | string): boolean {
    return this.votedByMe.has(String(invoiceId));
  }

  /** Whether the registry is currently paused, per the last observed
   * `paused`/`unpaused` event — used to skip vote submission instead of
   * hitting (and immediately retrying) a `ContractPaused` failure. */
  isPaused(): boolean {
    return this.paused;
  }

  /** Marks the registry as paused directly, for when a `submit_vote`/
   * `open_verification_round` call fails with `ContractPaused` before this
   * node has observed the corresponding `paused` event (e.g. a race on
   * startup, or a missed event). */
  markPaused(): void {
    this.paused = true;
  }

  isOpen(invoiceId: bigint | string): boolean {
    return this.rounds.get(String(invoiceId))?.status === 'Open';
  }

  list(): TrackedRound[] {
    return Array.from(this.rounds.values());
  }

  private upsert(invoiceId: string, status: RoundStatus): void {
    this.rounds.set(invoiceId, {
      invoiceId,
      status,
      votedByThisNode: this.votedByMe.has(invoiceId),
      updatedAt: Date.now(),
    });

    if (TERMINAL_STATUSES.has(status)) {
      this.markFinalized(invoiceId);
    }

    this.prune();
  }

  /** Records `invoiceId` as finalized, moving it to the back of the FIFO
   * eviction queue if it was already there (e.g. a `consensus` event
   * followed by a later `fallback` event for the same round). */
  private markFinalized(invoiceId: string): void {
    if (this.finalizedSet.has(invoiceId)) {
      const idx = this.finalizedOrder.indexOf(invoiceId);
      if (idx !== -1) this.finalizedOrder.splice(idx, 1);
    }
    this.finalizedOrder.push(invoiceId);
    this.finalizedSet.add(invoiceId);
  }

  /** Evicts finalized rounds that have aged past the retention window, and
   * caps the number of retained finalized rounds at `maxFinalizedRounds`
   * (oldest evicted first) so a burst of finalizations can't outpace the
   * time-based prune. Open rounds are never evicted here — only rounds that
   * have already reached a terminal status. */
  private prune(): void {
    const retentionMs = this.options.finalizedRetentionMs ?? FINALIZED_RETENTION_MS;
    const maxFinalized = this.options.maxFinalizedRounds ?? MAX_FINALIZED_ROUNDS;
    const now = Date.now();

    while (this.finalizedOrder.length > 0) {
      const oldestId = this.finalizedOrder[0];
      const round = this.rounds.get(oldestId);

      // Round already gone (shouldn't normally happen) — drop the stale
      // queue entry and keep going.
      if (!round) {
        this.finalizedOrder.shift();
        this.finalizedSet.delete(oldestId);
        continue;
      }

      const isStale = now - round.updatedAt >= retentionMs;
      const overCapacity = this.finalizedOrder.length > maxFinalized;
      if (!isStale && !overCapacity) break;

      this.finalizedOrder.shift();
      this.finalizedSet.delete(oldestId);
      this.rounds.delete(oldestId);
      this.votedByMe.delete(oldestId);
    }
  }

  /** Exposed for tests: current count of rounds retained purely because
   * they're finalized and within the retention/capacity window. */
  finalizedCount(): number {
    return this.finalizedOrder.length;
  }
}

function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [value];
}
