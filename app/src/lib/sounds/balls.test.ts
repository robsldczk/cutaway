import { describe, expect, it } from 'vitest';
import { ball, depthAt, groups, merges, radius, spread, type Field } from './balls';
import type { Settled } from './lanes';

const f: Field = { x0: 100, width: 300, y: (t) => t * 40 };
const s = (id: string, t0: number, depth = 0.3, len = 1, added = 0): Settled => ({ id, t0, len, depth, added });

describe('ball geometry', () => {
  it('grows with length up to a limit and shrinks with depth', () => {
    expect(radius(1, 0)).toBe(20);
    expect(radius(100, 0)).toBe(44);
    expect(radius(1, 1)).toBeCloseTo(14);
  });

  it('sits at its depth and start', () => {
    const b = ball({ t0: 2, len: 1, depth: 0.5 }, f);
    expect(b.y).toBe(80);
    expect(b.x).toBeCloseTo(100 + 150 + radius(1, 0.5));
  });

  it('turns a drop position into a depth inside the field', () => {
    expect(depthAt(250, f)).toBe(0.5);
    expect(depthAt(0, f)).toBe(0);
    expect(depthAt(900, f)).toBe(1);
  });
});

describe('the merge rule', () => {
  it('merges balls that truly overlap at about the same depth', () => {
    expect(merges(s('a', 1), s('b', 1.3), f)).toBe(true);
  });

  it('needs centres closer than 0.85 of the radii: touching is not enough', () => {
    const r = radius(1, 0.3);
    const dt = (r * 2 * 0.86) / 40;
    expect(merges(s('a', 1), s('b', 1 + dt), f)).toBe(false);
    const dt2 = (r * 2 * 0.84) / 40;
    expect(merges(s('a', 1), s('b', 1 + dt2), f)).toBe(true);
  });

  it('never merges a far ball over a close one', () => {
    // a narrow field: they do share space here, so only the depth rule keeps them apart
    const narrow = { ...f, width: 60 };
    expect(merges(s('a', 1, 0.3), s('b', 1, 0.43), narrow)).toBe(false);
    expect(merges(s('a', 1, 0.3), s('b', 1, 0.41), narrow)).toBe(true);
  });

  it('a depth difference of exactly 0.12 stays apart', () => {
    expect(merges(s('a', 1, 0.5), s('b', 1, 0.62), { ...f, width: 0 })).toBe(false);
  });
});

describe('groups', () => {
  it('leaves a lone ball alone', () => {
    expect(groups([s('a', 1), s('b', 9)], f)).toEqual([]);
  });

  it('chains through a ball in the middle', () => {
    const g = groups([s('a', 1), s('b', 1.6), s('c', 2.2)], f);
    expect(g).toHaveLength(1);
    expect(g[0].members.map((m) => m.id)).toEqual(['a', 'b', 'c']);
  });

  it('orders members by time, ties by the order they were added', () => {
    const g = groups([s('z', 1, 0.3, 1, 2), s('y', 1, 0.3, 1, 5), s('x', 0.9, 0.3, 1, 9)], f);
    expect(g[0].members.map((m) => m.id)).toEqual(['x', 'z', 'y']);
  });

  it('sits between its members, around them', () => {
    const [g] = groups([s('a', 1), s('b', 1.4)], f);
    expect(g.y).toBeCloseTo(48);
    expect(g.r).toBeGreaterThan(radius(1, 0.3));
    expect(g.depth).toBeCloseTo(0.3);
  });
});

describe('an open group', () => {
  const [g] = groups([s('a', 1, 0.3, 1, 0), s('b', 1, 0.3, 1, 1), s('c', 1, 0.3, 1, 2)], f);
  const r = (_id: string) => 16;

  it('spreads in time order, top to bottom, without touching', () => {
    const p = spread(g, r, 0);
    const [a, b, c] = ['a', 'b', 'c'].map((id) => p.get(id)!);
    expect(a.y).toBeLessThan(b.y);
    expect(b.y).toBeLessThan(c.y);
    expect(b.y - a.y).toBe(46);
  });

  it('is centred on the group and staggered slightly', () => {
    const p = spread(g, r, 0);
    expect((p.get('a')!.y + p.get('c')!.y) / 2).toBeCloseTo(g.y);
    expect(p.get('c')!.delay).toBeCloseTo(0.06);
  });

  it('stays out of the lanes', () => {
    for (const b of spread(g, r, 500).values()) expect(b.x).toBeGreaterThanOrEqual(516);
  });
});
