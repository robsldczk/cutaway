import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { settledOf, Settler, snapshot, SETTLE_MS } from './settle';

const live = (id: string, start_s = 1) => ({ id, start_s, len_s: 2, depth: 0.5, added: 3 });

describe('snapshot', () => {
  it('holds what clustering reads', () => {
    expect(snapshot([live('a')]).get('a')).toEqual({ id: 'a', t0: 1, len: 2, depth: 0.5, added: 3 });
  });

  it('keeps the old values while the sound moves on', () => {
    const snap = snapshot([live('a', 1)]);
    const { z, fresh } = settledOf(live('a', 7), snap);
    expect(z.t0).toBe(1);
    expect(fresh).toBe(false);
  });

  it('marks a sound it has not seen as fresh', () => {
    const { z, fresh } = settledOf(live('new', 7), new Map());
    expect(z.t0).toBe(7);
    expect(fresh).toBe(true);
  });
});

describe('settling', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('happens half a second after things go quiet', () => {
    const fire = vi.fn();
    const s = new Settler(fire);
    s.soon();
    vi.advanceTimersByTime(SETTLE_MS - 1);
    expect(fire).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(fire).toHaveBeenCalledOnce();
  });

  it('waits on while changes keep coming', () => {
    const fire = vi.fn();
    const s = new Settler(fire);
    for (let i = 0; i < 5; i++) {
      s.soon();
      vi.advanceTimersByTime(300);
    }
    expect(fire).not.toHaveBeenCalled();
    vi.advanceTimersByTime(300);
    expect(fire).toHaveBeenCalledOnce();
  });

  it('never fires during a gesture', () => {
    const fire = vi.fn();
    const s = new Settler(fire);
    s.hold();
    s.soon();
    vi.advanceTimersByTime(5000);
    expect(fire).not.toHaveBeenCalled();
  });

  it('settles after a gesture that something happened in', () => {
    const fire = vi.fn();
    const s = new Settler(fire);
    s.hold();
    s.soon();
    s.release();
    vi.advanceTimersByTime(SETTLE_MS);
    expect(fire).toHaveBeenCalledOnce();
  });

  it('a gesture started just before settling puts it off', () => {
    const fire = vi.fn();
    const s = new Settler(fire);
    s.soon();
    vi.advanceTimersByTime(400);
    s.hold();
    vi.advanceTimersByTime(5000);
    expect(fire).not.toHaveBeenCalled();
    s.release();
    vi.advanceTimersByTime(SETTLE_MS);
    expect(fire).toHaveBeenCalledOnce();
  });

  it('a gesture that changed nothing leaves nothing to settle', () => {
    const fire = vi.fn();
    const s = new Settler(fire);
    s.hold();
    s.release();
    vi.advanceTimersByTime(5000);
    expect(fire).not.toHaveBeenCalled();
  });

  it('can be cancelled when the page goes away', () => {
    const fire = vi.fn();
    const s = new Settler(fire);
    s.soon();
    s.cancel();
    vi.advanceTimersByTime(5000);
    expect(fire).not.toHaveBeenCalled();
  });
});
