import { describe, expect, it } from 'vitest';
import { assignLanes, byTime, OUTER_LANE, SHORT_LANES, type Lanes, type Settled } from './lanes';

const s = (id: string, t0: number, len = 1, added = 0): Settled => ({ id, t0, len, depth: 0, added });
const snap = (list: Settled[]) => new Map(list.map((x) => [x.id, x]));
const lanes = (list: Settled[], prev: Lanes = new Map(), before = new Map<string, Settled>()) => assignLanes(list, prev, before);

describe('lanes', () => {
  it('puts sounds that overlap side by side and reuses free lanes', () => {
    const l = lanes([s('a', 0, 2), s('b', 1, 2), s('c', 5, 1)]);
    expect([l.get('a'), l.get('b'), l.get('c')]).toEqual([0, 1, 0]);
  });

  it('adding a sound moves no other', () => {
    const list = [s('a', 0, 3), s('b', 1, 3), s('c', 2, 3)];
    const first = lanes(list);
    // starts earliest and overlaps them all: it still only looks for a free lane
    const after = assignLanes([s('n', 0.5, 3), ...list], first, snap(list));
    for (const x of list) expect(after.get(x.id)).toBe(first.get(x.id));
    expect(after.get('n')).toBe(3);
  });

  it('a removed sound leaves the others where they are', () => {
    const list = [s('a', 0, 3), s('b', 1, 3), s('c', 2, 3)];
    const first = lanes(list);
    const after = assignLanes(list.slice(1), first, snap(list));
    expect(after.get('b')).toBe(first.get('b'));
    expect(after.get('c')).toBe(first.get('c'));
  });

  it('only a moved sound looks for a lane', () => {
    const list = [s('a', 0, 2), s('b', 1, 2), s('c', 5, 1)];
    const first = lanes(list);
    const moved = [list[0], list[1], s('c', 1.5, 1)];
    const after = assignLanes(moved, first, snap(list));
    expect(after.get('a')).toBe(first.get('a'));
    expect(after.get('b')).toBe(first.get('b'));
    expect(after.get('c')).toBe(2);
  });

  it('sends long sounds to their own outer lane', () => {
    const l = lanes([s('bed', 0, 45), s('a', 1), s('b', 1)]);
    expect(l.get('bed')).toBe(OUTER_LANE);
    expect(l.get('a')).toBe(0);
  });

  it('a sound of exactly eight seconds is not a bed', () => {
    expect(lanes([s('a', 0, 8)]).get('a')).toBe(0);
  });

  it('more than five at once share the last lane', () => {
    const l = lanes(Array.from({ length: 8 }, (_, i) => s(`s${i}`, 0, 1, i)));
    expect([...l.values()].sort()).toEqual([0, 1, 2, 3, 4, 4, 4, 4]);
    expect(Math.max(...l.values())).toBe(SHORT_LANES - 1);
  });

  it('keeps a bit of air between sounds on a lane', () => {
    expect(lanes([s('a', 0, 1), s('b', 1.01, 1)]).get('b')).toBe(1);
    expect(lanes([s('a', 0, 1), s('b', 1.06, 1)]).get('b')).toBe(0);
  });

  it('sticks to a lane across settles when nothing changes', () => {
    const list = [s('a', 0, 2), s('b', 1, 2)];
    const first = lanes(list);
    expect(assignLanes(list, first, snap(list))).toEqual(first);
  });
});

describe('order', () => {
  it('goes by time, then by the order sounds were added', () => {
    const list = [s('late', 2, 1, 1), s('b', 1, 1, 7), s('a', 1, 1, 3)];
    expect([...list].sort(byTime).map((x) => x.id)).toEqual(['a', 'b', 'late']);
  });

  it('stays stable for older sounds with no order', () => {
    const list = [s('z', 1), s('y', 1)];
    expect([...list].sort(byTime).map((x) => x.id)).toEqual(['y', 'z']);
  });
});
