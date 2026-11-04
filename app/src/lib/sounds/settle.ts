// Settling. Nothing but the dragged items reacts during a drag: merges, splits and lanes read
// a snapshot, and the snapshot only moves about half a second after things have gone quiet.
import type { Settled } from './lanes';

export const SETTLE_MS = 500;

export interface Live {
  id: string;
  start_s: number;
  len_s: number;
  depth: number;
  added: number;
}

export function snapshot(sounds: Live[]): Map<string, Settled> {
  return new Map(sounds.map((s) => [s.id, { id: s.id, t0: s.start_s, len: s.len_s, depth: s.depth, added: s.added }]));
}

/** A sound's settled values, and whether it has been settled yet (a fresh one can't merge). */
export function settledOf(s: Live, snap: Map<string, Settled>): { z: Settled; fresh: boolean } {
  const z = snap.get(s.id);
  return z ? { z, fresh: false } : { z: { id: s.id, t0: s.start_s, len: s.len_s, depth: s.depth, added: s.added }, fresh: true };
}

/** Runs `fire` once things have been quiet for the delay; never while a gesture is on. */
export class Settler {
  private timer: ReturnType<typeof setTimeout> | null = null;
  private held = false;
  private owed = false;

  constructor(
    private fire: () => void,
    private delay = SETTLE_MS,
  ) {}

  /** Something changed: settle a little later. */
  soon() {
    this.cancel();
    if (this.held) {
      this.owed = true;
      return;
    }
    this.timer = setTimeout(() => {
      this.timer = null;
      this.fire();
    }, this.delay);
  }

  /** A gesture started: nothing settles until it ends. */
  hold() {
    this.held = true;
    if (this.timer) {
      this.cancel();
      this.owed = true;
    }
  }

  /** The gesture ended. */
  release() {
    this.held = false;
    const owed = this.owed;
    this.owed = false;
    if (owed) this.soon();
  }

  cancel() {
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
  }
}
