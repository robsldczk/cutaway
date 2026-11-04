import { describe, expect, it } from 'vitest';
import { EDGE_PX_PER_S, rowSpans, TimeMap } from './timemap';

// two 2 s lines of 100 px with a 1 s pause on a 30 px row between them
const rows = [
  { start_s: 0, end_s: 2, top: 0, height: 100 },
  { start_s: 3, end_s: 5, top: 130, height: 100 },
];
const map = () => new TimeMap(rowSpans(rows));

describe('time map', () => {
  it('has each card and gap row on its own scale', () => {
    const m = map();
    expect(m.y(1)).toBe(50);
    expect(m.y(2.5)).toBe(115);
    expect(m.y(4)).toBe(180);
  });

  it('goes both ways', () => {
    const m = map();
    for (const t of [0, 0.3, 1.9, 2.4, 3, 4.2, 5]) expect(m.t(m.y(t))).toBeCloseTo(t, 9);
  });

  it('counts the spacing between cards as no time', () => {
    const m = new TimeMap(
      rowSpans([
        { start_s: 0, end_s: 2, top: 0, height: 100 },
        { start_s: 2, end_s: 4, top: 106, height: 100 },
      ]),
    );
    expect(m.y(2)).toBe(106);
    expect(m.t(103)).toBe(2);
  });

  it('runs on past both ends at the edge scale', () => {
    const m = map();
    expect(m.y(6)).toBe(230 + EDGE_PX_PER_S);
    expect(m.y(-1)).toBe(-EDGE_PX_PER_S);
    expect(m.t(230 + EDGE_PX_PER_S * 2)).toBeCloseTo(7, 9);
  });

  it('keeps rising when lines overlap in time', () => {
    const m = new TimeMap(
      rowSpans([
        { start_s: 0, end_s: 2, top: 0, height: 100 },
        { start_s: 1.5, end_s: 3.5, top: 90, height: 100 },
      ]),
    );
    let last = -Infinity;
    for (let t = 0; t <= 3.5; t += 0.01) {
      expect(m.y(t)).toBeGreaterThanOrEqual(last);
      last = m.y(t);
    }
    expect(m.t(m.y(3))).toBeCloseTo(3, 6);
  });

  it('keeps rising when a pinned line sits out of order', () => {
    const m = new TimeMap(
      rowSpans([
        { start_s: 0, end_s: 2, top: 0, height: 100 },
        { start_s: 9, end_s: 10, top: 130, height: 100 },
        { start_s: 3, end_s: 5, top: 260, height: 100 },
      ]),
    );
    let last = -Infinity;
    for (let t = 0; t <= 12; t += 0.05) {
      const y = m.y(t);
      expect(Number.isFinite(y)).toBe(true);
      expect(y).toBeGreaterThanOrEqual(last);
      last = y;
    }
  });

  it('copes with lines of no length and with no lines', () => {
    const m = new TimeMap(
      rowSpans([
        { start_s: 0, end_s: 2, top: 0, height: 100 },
        { start_s: 2, end_s: 2, top: 130, height: 100 },
        { start_s: 3, end_s: 5, top: 260, height: 100 },
      ]),
    );
    expect(Number.isFinite(m.y(2.5))).toBe(true);
    expect(m.y(3)).toBe(260);
    const none = new TimeMap([]);
    expect(none.empty).toBe(true);
    expect(none.t(none.y(4))).toBe(4);
  });
});
