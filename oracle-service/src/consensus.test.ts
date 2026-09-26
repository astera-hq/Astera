import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { ConsensusTracker } from './consensus.ts';

const ORACLE = 'GABCDEFEXAMPLEORACLEPUBLICKEY000000000000000000';

describe('ConsensusTracker', () => {
  it('tracks rnd_open as Open', () => {
    const tracker = new ConsensusTracker(ORACLE);
    tracker.handleEvent('rnd_open', ['42']);
    assert.equal(tracker.isOpen('42'), true);
    assert.equal(tracker.list().length, 1);
    assert.equal(tracker.list()[0].status, 'Open');
    assert.equal(tracker.list()[0].votedByThisNode, false);
  });

  it('records votes from this node and ignores others', () => {
    const tracker = new ConsensusTracker(ORACLE);
    tracker.handleEvent('rnd_open', [7n]);
    tracker.handleEvent('voted', [7n, 'GOTHERORACLE']);
    assert.equal(tracker.hasVoted(7n), false);
    tracker.handleEvent('voted', [7n, ORACLE]);
    assert.equal(tracker.hasVoted('7'), true);
    assert.equal(tracker.list()[0].votedByThisNode, true);
  });

  it('marks consensus approved and rejected', () => {
    const tracker = new ConsensusTracker(ORACLE);
    tracker.handleEvent('rnd_open', ['1']);
    tracker.handleEvent('consensus', ['1', true]);
    assert.equal(tracker.isOpen('1'), false);
    assert.equal(tracker.list()[0].status, 'ConsensusApproved');

    tracker.handleEvent('rnd_open', ['2']);
    tracker.handleEvent('consensus', ['2', false]);
    assert.equal(tracker.list().find((r) => r.invoiceId === '2')?.status, 'ConsensusRejected');
  });

  it('handles rnd_exp and fallback', () => {
    const tracker = new ConsensusTracker(ORACLE);
    tracker.handleEvent('rnd_open', ['9']);
    tracker.handleEvent('rnd_exp', '9');
    assert.equal(tracker.list()[0].status, 'Expired');

    tracker.handleEvent('rnd_open', ['10']);
    tracker.handleEvent('fallback', ['10', true]);
    assert.equal(tracker.list().find((r) => r.invoiceId === '10')?.status, 'ConsensusApproved');

    tracker.handleEvent('rnd_open', ['11']);
    tracker.handleEvent('fallback', ['11', false]);
    assert.equal(tracker.list().find((r) => r.invoiceId === '11')?.status, 'ConsensusRejected');
  });

  it('tracks paused / unpaused and markPaused', () => {
    const tracker = new ConsensusTracker(ORACLE);
    assert.equal(tracker.isPaused(), false);
    tracker.handleEvent('paused', null);
    assert.equal(tracker.isPaused(), true);
    tracker.handleEvent('unpaused', null);
    assert.equal(tracker.isPaused(), false);
    tracker.markPaused();
    assert.equal(tracker.isPaused(), true);
  });

  it('ignores unknown topic2 values', () => {
    const tracker = new ConsensusTracker(ORACLE);
    tracker.handleEvent('something_else', ['1']);
    assert.equal(tracker.list().length, 0);
  });

  it('accepts non-array event values via asArray fallback', () => {
    const tracker = new ConsensusTracker(ORACLE);
    tracker.handleEvent('rnd_open', '99');
    assert.equal(tracker.isOpen('99'), true);
  });
});
