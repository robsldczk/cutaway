import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { EVERY_MS, flushSpot, keepSpot, landing, moveSpot, queueSpot, spotOf, type Spot } from './resume';

const spot = (over: Partial<Spot> = {}): Spot => ({ page: 'edit', tab: 'files', scene: 's2', line: 'l7', at: 12.5, zoom: 80, ...over });

beforeEach(() => {
  const m = new Map<string, string>();
  vi.stubGlobal('localStorage', {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
    removeItem: (k: string) => void m.delete(k),
  });
});
afterEach(() => {
  flushSpot();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe('spots', () => {
  it('keeps one spot per episode', () => {
    keepSpot('/a', spot());
    keepSpot('/b', spot({ scene: 's9' }));
    keepSpot('/a', spot({ at: 3 }));
    expect(spotOf('/a')?.at).toBe(3);
    expect(spotOf('/b')?.scene).toBe('s9');
    expect(spotOf('/c')).toBeNull();
  });

  it('remembers every episode, however many there are', () => {
    for (let i = 0; i < 500; i++) keepSpot(`/${i}`, spot({ scene: `s${i}` }));
    expect(spotOf('/0')?.scene).toBe('s0');
    expect(spotOf('/499')?.scene).toBe('s499');
  });

  it('moves with an episode that was found somewhere else', () => {
    keepSpot('/old', spot({ scene: 's7' }));
    keepSpot('/new', spot({ scene: 'stale' }));
    moveSpot('/old', '/new');
    expect(spotOf('/old')).toBeNull();
    expect(spotOf('/new')?.scene).toBe('s7');
  });

  it("doesn't trust what storage hands back", () => {
    localStorage.setItem('cutaway.spots', 'not json');
    expect(spotOf('/a')).toBeNull();
    localStorage.setItem('cutaway.spots', JSON.stringify([['/a', { page: 'gone', scene: 's1' }], ['/b', { page: 'edit' }], 'x']));
    expect(spotOf('/a')).toBeNull();
    expect(spotOf('/b')).toBeNull();
    localStorage.setItem('cutaway.spots', JSON.stringify([['/a', { page: 'edit', scene: 's1', at: 'x', zoom: -2, tab: 'old' }]]));
    expect(spotOf('/a')).toEqual({ page: 'edit', tab: 'files', scene: 's1', line: null, at: 0, zoom: 0 });
  });

  it('survives storage that refuses', () => {
    vi.stubGlobal('localStorage', {
      getItem: () => {
        throw new Error('denied');
      },
      setItem: () => {
        throw new Error('denied');
      },
    });
    expect(() => keepSpot('/a', spot())).not.toThrow();
    expect(spotOf('/a')).toBeNull();
  });
});

describe('queueSpot', () => {
  it('writes the newest spot once per interval, however fast the playhead moves', () => {
    vi.useFakeTimers();
    const set = vi.spyOn(localStorage, 'setItem');
    for (let i = 0; i < 30; i++) {
      queueSpot('/a', spot({ at: i }));
      vi.advanceTimersByTime(EVERY_MS / 30 - 1);
    }
    expect(set.mock.calls.length).toBeLessThanOrEqual(1);
    vi.advanceTimersByTime(EVERY_MS);
    expect(spotOf('/a')?.at).toBe(29);
  });

  it("doesn't write a spot that hasn't changed", () => {
    vi.useFakeTimers();
    queueSpot('/a', spot());
    vi.advanceTimersByTime(EVERY_MS);
    const set = vi.spyOn(localStorage, 'setItem');
    queueSpot('/a', spot());
    vi.advanceTimersByTime(EVERY_MS);
    expect(set).not.toHaveBeenCalled();
  });

  it('flushes at once', () => {
    vi.useFakeTimers();
    queueSpot('/f', spot({ at: 4 }));
    expect(spotOf('/f')).toBeNull();
    flushSpot();
    expect(spotOf('/f')?.at).toBe(4);
  });
});

describe('landing', () => {
  const lines = [{ line_id: 'l1' }, { line_id: 'l7' }];

  it('lands on the spot', () => {
    expect(landing(spot(), 60, lines)).toEqual({ at: 12.5, line: 'l7' });
  });

  it('stays inside a scene that got shorter', () => {
    expect(landing(spot(), 9, lines).at).toBe(9);
  });

  it('leaves the line open when it has gone', () => {
    expect(landing(spot({ line: 'gone' }), 60, lines).line).toBeNull();
  });
});
