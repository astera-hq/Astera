import { RoundStatus } from './types';

export interface TrackedRound {
  invoiceId: string;
  status: RoundStatus;
  votedByThisNode: boolean;
  updatedAt: number;
}

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

  constructor(private readonly oraclePublicKey: string) {}

  /** Call with the decoded `(topic1, topic2)` pair and event value for every
   * event emitted under the registry contract's "ORACLE" topic namespace. */
  handleEvent(topic2: string, value: unknown): void {
    switch (topic2) {
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
  }
}

function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [value];
}
