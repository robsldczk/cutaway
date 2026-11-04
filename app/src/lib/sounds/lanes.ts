// Length-line lanes beside the spine. Sticky: a sound keeps its lane, only a new or moved one
// looks for a free lane, so adding a sound never shuffles the others (handover 5.4).

export const SHORT_LANES = 5;
/** Very long sounds (room tone, walla) live on their own outer lane. */
export const OUTER_LANE = SHORT_LANES;
export const LONG_S = 8;
/** Air between two sounds on a lane. */
const LANE_GAP_S = 0.05;

/** A sound as last settled. */
export interface Settled {
  id: string;
  t0: number;
  len: number;
  depth: number;
  added: number;
}

export type Lanes = Map<string, number>;

/** Sounds in time order; those that start together in the order they were added. */
export function byTime<T extends { id: string; t0: number; added: number }>(a: T, b: T) {
  return a.t0 - b.t0 || a.added - b.added || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
}

export function assignLanes(sounds: Settled[], prevLanes: Lanes, prev: Map<string, Settled>): Lanes {
  const same = (s: Settled) => {
    const p = prev.get(s.id);
    return !!p && p.t0 === s.t0 && p.len === s.len && prevLanes.has(s.id);
  };
  const order = [...sounds].sort((a, b) => Number(same(b)) - Number(same(a)) || byTime(a, b));
  const taken: [number, number][][] = Array.from({ length: SHORT_LANES }, () => []);
  const lanes: Lanes = new Map();
  for (const s of order) {
    if (s.len > LONG_S) {
      lanes.set(s.id, OUTER_LANE);
      continue;
    }
    const free = (l: number) => taken[l].every(([a, b]) => s.t0 + s.len + LANE_GAP_S <= a || s.t0 >= b + LANE_GAP_S);
    let lane = same(s) ? (prevLanes.get(s.id) ?? -1) : -1;
    if (lane < 0 || lane >= SHORT_LANES || !free(lane)) {
      lane = SHORT_LANES - 1; // more than five at once share the last lane (Rob)
      for (let l = 0; l < SHORT_LANES; l++) {
        if (free(l)) {
          lane = l;
          break;
        }
      }
    }
    taken[lane].push([s.t0, s.t0 + s.len]);
    lanes.set(s.id, lane);
  }
  return lanes;
}
