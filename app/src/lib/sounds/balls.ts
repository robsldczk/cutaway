// The balls: geometry, the merge rule and how groups open up. Size is duration, x is depth,
// y is when the sound starts. Clustering reads the settled snapshot, never live values.
import { byTime, type Settled } from './lanes';

export interface Field {
  /** Left edge of the depth field and its width. */
  x0: number;
  width: number;
  y: (t: number) => number;
}

export interface Ball {
  x: number;
  y: number;
  r: number;
}

/** Sounds at about this depth can merge; further apart in depth they never do. */
export const MERGE_DEPTH = 0.12;
/** Centres closer than this share of the sum of radii count as real overlap. */
export const MERGE_OVERLAP = 0.85;

export function radius(len: number, depth: number) {
  return Math.min(44, 14 + len * 6) * (1 - 0.3 * depth);
}

export function ball(s: { t0: number; len: number; depth: number }, f: Field): Ball {
  const r = radius(s.len, s.depth);
  return { x: f.x0 + s.depth * f.width + r, y: f.y(s.t0), r };
}

/** The depth a ball's centre stands for (what a drop position gives). */
export function depthAt(x: number, f: Field) {
  return Math.max(0, Math.min(1, (x - f.x0) / Math.max(1, f.width)));
}

/** Handover 5.2: only balls that physically share space, at about the same depth. */
export function merges(a: Settled, b: Settled, f: Field) {
  if (Math.abs(a.depth - b.depth) >= MERGE_DEPTH) return false;
  const p = ball(a, f);
  const q = ball(b, f);
  return Math.hypot(p.x - q.x, p.y - q.y) < (p.r + q.r) * MERGE_OVERLAP;
}

export interface Group {
  /** Stable while its members stay the same. */
  key: string;
  /** Members in time order, ties by the order they were added. */
  members: Settled[];
  x: number;
  y: number;
  r: number;
  depth: number;
}

/** Groups of two or more. `sounds` holds only what may merge: settled and not hidden. */
export function groups(sounds: Settled[], f: Field): Group[] {
  const parent = new Map(sounds.map((s) => [s.id, s.id]));
  const find = (id: string): string => {
    let a = id;
    while (parent.get(a) !== a) {
      parent.set(a, parent.get(parent.get(a)!)!);
      a = parent.get(a)!;
    }
    return a;
  };
  for (let i = 0; i < sounds.length; i++) {
    for (let j = i + 1; j < sounds.length; j++) {
      if (merges(sounds[i], sounds[j], f)) parent.set(find(sounds[i].id), find(sounds[j].id));
    }
  }
  const sets = new Map<string, Settled[]>();
  for (const s of sounds) {
    const k = find(s.id);
    sets.set(k, [...(sets.get(k) ?? []), s]);
  }
  const out: Group[] = [];
  for (const set of sets.values()) {
    if (set.length < 2) continue;
    const members = [...set].sort(byTime);
    let w = 0;
    let x = 0;
    let y = 0;
    let depth = 0;
    let biggest = 0;
    for (const m of members) {
      const b = ball(m, f);
      const a = b.r * b.r;
      w += a;
      x += b.x * a;
      y += b.y * a;
      depth += m.depth;
      biggest = Math.max(biggest, b.r);
    }
    out.push({ key: members.map((m) => m.id).join('-'), members, x: x / w, y: y / w, r: Math.max(Math.sqrt(w) * 0.82, biggest + 5), depth: depth / members.length });
  }
  return out;
}

export interface Spread extends Ball {
  /** Seconds to wait, for the small stagger when a group opens. */
  delay: number;
}

const SPREAD_GAP = 14;
const SPREAD_SWAY = 22;
const SPREAD_STAGGER_S = 0.03;

/** An open group: its balls in a column around the group's centre, in time order. */
export function spread(g: Group, radiusOf: (id: string) => number, minX: number): Map<string, Spread> {
  const out = new Map<string, Spread>();
  const total = g.members.reduce((a, m) => a + 2 * radiusOf(m.id), 0) + SPREAD_GAP * (g.members.length - 1);
  let y = g.y - total / 2;
  g.members.forEach((m, j) => {
    const r = radiusOf(m.id);
    out.set(m.id, { x: Math.max(minX + r, g.x + (j % 2 ? SPREAD_SWAY : -SPREAD_SWAY)), y: y + r, r, delay: j * SPREAD_STAGGER_S });
    y += 2 * r + SPREAD_GAP;
  });
  return out;
}

/** Where a ball shows; `tucked` balls are inside their closed group. */
export type Shown = Spread & { tucked?: boolean };

/** Every ball's place on the page: at its own spot, opened out in its group, or tucked into
 *  it. The dragged ones (`live`) are where the pointer puts them, whatever their group does. */
export function layoutBalls(sounds: { id: string; start_s: number; len_s: number; depth: number }[], gs: Group[], f: Field, isOpen: (g: Group) => boolean, live: Set<string>): Map<string, Shown> {
  const own = (s: (typeof sounds)[number]): Shown => ({ ...ball({ t0: s.start_s, len: s.len_s, depth: s.depth }, f), delay: 0 });
  const m = new Map<string, Shown>(sounds.map((s) => [s.id, own(s)]));
  for (const g of gs) {
    if (isOpen(g)) {
      for (const [id, p] of spread(g, (id) => m.get(id)?.r ?? 16, f.x0 - 10)) m.set(id, p);
      continue;
    }
    for (const x of g.members) {
      const b = m.get(x.id);
      if (b) m.set(x.id, { x: g.x, y: g.y, r: b.r, delay: 0, tucked: true });
    }
  }
  for (const s of sounds) if (live.has(s.id)) m.set(s.id, own(s));
  return m;
}
