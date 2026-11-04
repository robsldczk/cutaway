// Time on the Arrange page. The column isn't linear in time: cards and gap rows each have
// their own scale, and the spacing between them is zero time. One map drives the spine, the
// length lines, the balls and the snapping, so they can never disagree.

/** A card or gap row: the time it stands for and where it sits. */
export interface Span {
  t0: number;
  t1: number;
  y0: number;
  y1: number;
}

/** What Arrange measures of a card. */
export interface RowBox {
  start_s: number;
  end_s: number;
  top: number;
  height: number;
}

interface Knot {
  t: number;
  y: number;
}

/** Scale above the first and below the last row, in px per second. */
export const EDGE_PX_PER_S = 14;

/** Cards and the gaps between them, in row order. */
export function rowSpans(rows: RowBox[]): Span[] {
  const out: Span[] = [];
  rows.forEach((r, i) => {
    const bottom = r.top + r.height;
    out.push({ t0: r.start_s, t1: r.end_s, y0: r.top, y1: bottom });
    const next = rows[i + 1];
    if (next) out.push({ t0: r.end_s, t1: next.start_s, y0: bottom, y1: next.top });
  });
  return out;
}

export class TimeMap {
  private knots: Knot[] = [];

  /** Rows can overlap in time (a negative pause) or sit out of order (a pinned clip): each
   *  span only adds the time and height not covered yet, so the map always rises. */
  constructor(
    spans: Span[],
    private edge = EDGE_PX_PER_S,
  ) {
    const k = this.knots;
    for (const s of spans) {
      let { t0, y0 } = s;
      const { t1, y1 } = s;
      const last = k[k.length - 1];
      if (t1 <= t0 && last) {
        // no time of its own; the next span's start makes any jump in height
        continue;
      }
      if (last && t1 <= last.t) continue;
      if (last && t0 < last.t) {
        y0 += ((last.t - t0) / (t1 - t0)) * (y1 - y0);
        t0 = last.t;
      }
      const a = Math.max(y0, last?.y ?? y0);
      k.push({ t: t0, y: a }, { t: t1, y: Math.max(y1, a) });
    }
  }

  get empty() {
    return this.knots.length === 0;
  }

  /** Where a time sits. Where height jumps at one time, the later height: a line's start is
   *  its card's top even with no pause before it. */
  y(t: number): number {
    const k = this.knots;
    if (!k.length) return t * this.edge;
    if (t < k[0].t) return k[0].y - (k[0].t - t) * this.edge;
    let lo = 0;
    let hi = k.length - 1;
    while (lo < hi) {
      const m = (lo + hi + 1) >> 1;
      if (k[m].t <= t) lo = m;
      else hi = m - 1;
    }
    const a = k[lo];
    const b = k[lo + 1];
    if (!b) return a.y + (t - a.t) * this.edge;
    return a.y + ((t - a.t) / (b.t - a.t)) * (b.y - a.y);
  }

  /** The time at a height. Between cards, where no time passes, that one time. */
  t(y: number): number {
    const k = this.knots;
    if (!k.length) return y / this.edge;
    if (y < k[0].y) return k[0].t - (k[0].y - y) / this.edge;
    let lo = 0;
    let hi = k.length - 1;
    while (lo < hi) {
      const m = (lo + hi + 1) >> 1;
      if (k[m].y <= y) lo = m;
      else hi = m - 1;
    }
    const a = k[lo];
    const b = k[lo + 1];
    if (!b) return a.t + (y - a.y) / this.edge;
    return a.t + ((y - a.y) / (b.y - a.y)) * (b.t - a.t);
  }
}
