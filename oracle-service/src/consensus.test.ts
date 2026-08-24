import { ConsensusTracker } from './consensus';

// #1181: no test coverage previously existed for ConsensusTracker's
// event-driven state transitions or (#1178) its bounded-growth eviction.
// These tests exercise: round open/vote/consensus/expiry transitions,
// hasVoted/isOpen/isPaused bookkeeping, and finalized-round pruning.

const ORACLE_KEY = 'GORACLEPUBLICKEYAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA';
const OTHER_ORACLE_KEY = 'GOTHERPUBLICKEYAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA';

describe('ConsensusTracker — event-driven state transitions', () => {
  it('starts with no tracked rounds and an unpaused, non-voted state', () => {
    const tracker = new ConsensusTracker(ORACLE_KEY);
    expect(tracker.list()).toEqual([]);
    expect(tracker.isPaused()).toBe(false);
    expect(tracker.hasVoted('1')).toBe(false);
    expect(tracker.isOpen('1')).toBe(false);
  });

  it('opens a round on rnd_open and marks it Open', () => {
    const tracker = new ConsensusTracker(ORACLE_KEY);
    tracker.handleEvent('rnd_open', ['42']);
    expect(tracker.isOpen('42')).toBe(true);
    const round = tracker.list().find((r) => r.invoiceId === '42');
    expect(round?.status).toBe('Open');
    expect(round?.votedByThisNode).toBe(false);
  });

  it('records hasVoted only when the voting oracle matches this node key', () => {
    const tracker = new ConsensusTracker(ORACLE_KEY);
    tracker.handleEvent('rnd_open', ['7']);

    // A different oracle votes — should not mark this node as having voted.
    tracker.handleEvent('voted', ['7', OTHER_ORACLE_KEY]);
    expect(tracker.hasVoted('7')).toBe(false);

    // This node's own oracle key votes.
    tracker.handleEvent('voted', ['7', ORACLE_KEY]);
    expect(tracker.hasVoted('7')).toBe(true);
    expect(tracker.hasVoted(7n)).toBe(true);
  });

  it('preserves the round status across a voted event (does not reset to Open)', () => {
    const tracker = new ConsensusTracker(ORACLE_KEY);
    tracker.handleEvent('rnd_open', ['7']);
    tracker.handleEvent('consensus', ['7', true]);
    expect(tracker.list().find((r) => r.invoiceId === '7')?.status).toBe('ConsensusApproved');

    // A late-arriving voted event for the same (now finalized) round must
    // not regress the status back to 'Open'.
    tracker.handleEvent('voted', ['7', ORACLE_KEY]);
    expect(tracker.list().find((r) => r.invoiceId === '7')?.status).toBe('ConsensusApproved');
  });

  it('marks consensus approval/rejection correctly', () => {
    const tracker = new ConsensusTracker(ORACLE_KEY);
    tracker.handleEvent('rnd_open', ['1']);
    tracker.handleEvent('consensus', ['1', true]);
    expect(tracker.list().find((r) => r.invoiceId === '1')?.status).toBe('ConsensusApproved');

    tracker.handleEvent('rnd_open', ['2']);
    tracker.handleEvent('consensus', ['2', false]);
    expect(tracker.list().find((r) => r.invoiceId === '2')?.status).toBe('ConsensusRejected');
  });

  it('marks fallback resolution the same way as consensus', () => {
    const tracker = new ConsensusTracker(ORACLE_KEY);
    tracker.handleEvent('rnd_open', ['3']);
    tracker.handleEvent('fallback', ['3', true]);
    expect(tracker.list().find((r) => r.invoiceId === '3')?.status).toBe('ConsensusApproved');
  });

  it('marks a round expired on rnd_exp, accepting a bare (non-array) value', () => {
    const tracker = new ConsensusTracker(ORACLE_KEY);
    tracker.handleEvent('rnd_open', ['5']);
    tracker.handleEvent('rnd_exp', '5');
    expect(tracker.list().find((r) => r.invoiceId === '5')?.status).toBe('Expired');
    expect(tracker.isOpen('5')).toBe(false);
  });

  it('toggles paused/unpaused from events, and markPaused sets it directly', () => {
    const tracker = new ConsensusTracker(ORACLE_KEY);
    expect(tracker.isPaused()).toBe(false);

    tracker.handleEvent('paused', null);
    expect(tracker.isPaused()).toBe(true);

    tracker.handleEvent('unpaused', null);
    expect(tracker.isPaused()).toBe(false);

    tracker.markPaused();
    expect(tracker.isPaused()).toBe(true);
  });

  it('ignores unrecognized topics without throwing', () => {
    const tracker = new ConsensusTracker(ORACLE_KEY);
    expect(() => tracker.handleEvent('some_unknown_topic', ['x'])).not.toThrow();
    expect(tracker.list()).toEqual([]);
  });
});

describe('ConsensusTracker — bounded growth / eviction (#1178)', () => {
  it('evicts a finalized round once it exceeds the retention window', () => {
    const tracker = new ConsensusTracker(ORACLE_KEY, { finalizedRetentionMs: 1000 });
    const realNow = Date.now;
    let now = 1_000_000;
    jest.spyOn(Date, 'now').mockImplementation(() => now);

    try {
      tracker.handleEvent('rnd_open', ['1']);
      tracker.handleEvent('consensus', ['1', true]);
      expect(tracker.finalizedCount()).toBe(1);
      expect(tracker.list().some((r) => r.invoiceId === '1')).toBe(true);

      // Advance time past the retention window and trigger another prune
      // via any subsequent event.
      now += 2000;
      tracker.handleEvent('rnd_open', ['2']);

      expect(tracker.list().some((r) => r.invoiceId === '1')).toBe(false);
      expect(tracker.hasVoted('1')).toBe(false);
    } finally {
      (Date.now as jest.Mock).mockRestore?.();
      Date.now = realNow;
    }
  });

  it('never evicts an open (non-terminal) round, regardless of age', () => {
    const tracker = new ConsensusTracker(ORACLE_KEY, { finalizedRetentionMs: 1000 });
    const realNow = Date.now;
    let now = 1_000_000;
    jest.spyOn(Date, 'now').mockImplementation(() => now);

    try {
      tracker.handleEvent('rnd_open', ['1']);
      now += 10_000;
      tracker.handleEvent('rnd_open', ['2']); // triggers a prune pass

      expect(tracker.isOpen('1')).toBe(true);
    } finally {
      (Date.now as jest.Mock).mockRestore?.();
      Date.now = realNow;
    }
  });

  it('caps the number of retained finalized rounds (FIFO) via maxFinalizedRounds', () => {
    const tracker = new ConsensusTracker(ORACLE_KEY, {
      // Retention effectively infinite; only the count cap should evict.
      finalizedRetentionMs: 10 ** 12,
      maxFinalizedRounds: 2,
    });

    tracker.handleEvent('rnd_open', ['1']);
    tracker.handleEvent('consensus', ['1', true]);
    tracker.handleEvent('rnd_open', ['2']);
    tracker.handleEvent('consensus', ['2', true]);
    tracker.handleEvent('rnd_open', ['3']);
    tracker.handleEvent('consensus', ['3', true]);

    // Oldest finalized round ('1') should have been evicted once the cap
    // of 2 was exceeded.
    const ids = tracker.list().map((r) => r.invoiceId);
    expect(ids).not.toContain('1');
    expect(ids).toContain('2');
    expect(ids).toContain('3');
    expect(tracker.finalizedCount()).toBe(2);
  });

  it('moves a round to the back of the eviction queue if it re-finalizes', () => {
    const tracker = new ConsensusTracker(ORACLE_KEY, {
      finalizedRetentionMs: 10 ** 12,
      maxFinalizedRounds: 2,
    });

    tracker.handleEvent('rnd_open', ['1']);
    tracker.handleEvent('consensus', ['1', true]); // finalized first
    tracker.handleEvent('rnd_open', ['2']);
    tracker.handleEvent('consensus', ['2', true]); // finalized second

    // Re-finalize '1' (e.g. a late fallback event) — it should move to the
    // back of the FIFO queue, so '2' becomes the next eviction candidate.
    tracker.handleEvent('fallback', ['1', false]);

    tracker.handleEvent('rnd_open', ['3']);
    tracker.handleEvent('consensus', ['3', true]); // pushes the queue over cap 2

    const ids = tracker.list().map((r) => r.invoiceId);
    expect(ids).not.toContain('2');
    expect(ids).toContain('1');
    expect(ids).toContain('3');
  });

  it('keeps unbounded growth from accumulating across many finalized rounds', () => {
    const tracker = new ConsensusTracker(ORACLE_KEY, {
      finalizedRetentionMs: 10 ** 12,
      maxFinalizedRounds: 100,
    });

    for (let i = 0; i < 1000; i++) {
      tracker.handleEvent('rnd_open', [String(i)]);
      tracker.handleEvent('consensus', [String(i), true]);
    }

    expect(tracker.list().length).toBeLessThanOrEqual(100);
    expect(tracker.finalizedCount()).toBeLessThanOrEqual(100);
  });
});
