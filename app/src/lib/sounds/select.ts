// Multi-select and what a gesture commits. A selection moves as one: relative offsets stay, a
// limit stops the whole group, and whatever the gesture did goes out as one undo step.
import type { Anchor } from '../types';
import { clamp } from '../format';
import type { Ball } from './balls';
import type { SnapTarget } from './snap';

export type Cmd = Record<string, unknown>;

export const GAIN_MIN_DB = -24;
export const GAIN_MAX_DB = 6;

/** What the commands need of a sound. */
export interface SoundState {
  id: string;
  start_s: number;
  len_s: number;
  offset_s: number;
  anchor: Anchor | null;
  depth: number;
  pan: number;
  gain_db: number;
}

/** Click selects one sound; ⇧ click adds it or takes it out. The last one is the primary. */
export function clickSelect(sel: string[], id: string, additive: boolean): string[] {
  if (!additive) return [id];
  return sel.includes(id) ? sel.filter((x) => x !== id) : [...sel, id];
}

/** What a press on a ball leaves selected: a ball already selected keeps the group together. */
export function pressSelect(sel: string[], id: string): string[] {
  return sel.includes(id) ? sel : [id];
}

export interface Rect {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

export function normRect(x0: number, y0: number, x1: number, y1: number): Rect {
  return { x0: Math.min(x0, x1), y0: Math.min(y0, y1), x1: Math.max(x0, x1), y1: Math.max(y0, y1) };
}

/** Balls the band touches. */
export function ballsInBand(balls: Map<string, Ball>, r: Rect): string[] {
  const out: string[] = [];
  for (const [id, b] of balls) {
    const dx = b.x - clamp(b.x, r.x0, r.x1);
    const dy = b.y - clamp(b.y, r.y0, r.y1);
    if (dx * dx + dy * dy <= b.r * b.r) out.push(id);
  }
  return out;
}

export function bandSelect(sel: string[], hit: string[], additive: boolean): string[] {
  return additive ? [...sel, ...hit.filter((id) => !sel.includes(id))] : hit;
}

/** The share of `delta` the whole group can take within limits. A value already outside the
 *  limits (the Edit page reaches further, D77) doesn't stop the others moving back in. */
export function groupDelta(values: number[], delta: number, lo: number, hi: number): number {
  if (!values.length) return 0;
  const down = Math.min(0, lo - Math.min(...values));
  const up = Math.max(0, hi - Math.max(...values));
  return clamp(delta, down, up);
}

/** A time drag: the grabbed sound wants `wanted`, or a target. The group follows; if a sound
 *  would pass the start of the scene the group stops there and the snap is off. */
export function timeDrag(sounds: { id: string; start_s: number }[], grabId: string, wanted: number, target: SnapTarget | null) {
  const grabbed = sounds.find((s) => s.id === grabId);
  if (!grabbed) return { delta: 0, target: null };
  const want = (target?.t ?? wanted) - grabbed.start_s;
  const delta = groupDelta(
    sounds.map((s) => s.start_s),
    want,
    0,
    Infinity,
  );
  return { delta, target: delta === want ? target : null };
}

const ms = (x: number) => Math.round(x * 1000) / 1000;

/** The grabbed sound hangs on the target, or goes free when nothing snapped. The others keep
 *  their distance: anchored ones stay on their line by a new offset. */
export function moveCommands(sounds: SoundState[], delta: number, grabId: string | null, target: SnapTarget | null): Cmd[] {
  return sounds.map((s) => {
    const placement_id = s.id;
    if (s.id === grabId && target) return { cmd: 'move_sound', placement_id, anchor: target.anchor, offset_s: 0 };
    if (s.anchor && s.id !== grabId) return { cmd: 'move_sound', placement_id, offset_s: ms(s.offset_s + delta) };
    return { cmd: 'move_sound', placement_id, at_s: ms(s.start_s + delta) };
  });
}

export const depthCommands = (sounds: SoundState[], delta: number): Cmd[] => sounds.map((s) => ({ cmd: 'set_sound_depth', placement_id: s.id, depth: clamp(s.depth + delta, 0, 1) }));

export const panCommands = (sounds: SoundState[], delta: number): Cmd[] => sounds.map((s) => ({ cmd: 'set_sound_pan', placement_id: s.id, pan: clamp(s.pan + delta, -1, 1) }));

export const gainCommands = (sounds: SoundState[], delta: number): Cmd[] => sounds.map((s) => ({ cmd: 'set_sound_gain', placement_id: s.id, gain_db: clamp(s.gain_db + delta, GAIN_MIN_DB, GAIN_MAX_DB) }));

export const removeCommands = (ids: string[]): Cmd[] => ids.map((placement_id) => ({ cmd: 'delete_placement', placement_id }));

/** One command for the whole gesture, so it is one undo step. */
export function oneStep(cmds: Cmd[]): Cmd | null {
  if (!cmds.length) return null;
  return cmds.length === 1 ? cmds[0] : { cmd: 'batch', commands: cmds };
}

// ---- the loudness ring ----

/** Where a gain sits on the ring, 0…1 from the top, clockwise. */
export const ringOf = (db: number) => clamp((db - GAIN_MIN_DB) / (GAIN_MAX_DB - GAIN_MIN_DB), 0, 1);

export const dbOfRing = (g: number) => GAIN_MIN_DB + g * (GAIN_MAX_DB - GAIN_MIN_DB);

/** The ring position under the pointer; turning past the top stops at the end it came from. */
export function ringAt(x: number, y: number, cx: number, cy: number, last: number): number {
  let a = Math.atan2(y - cy, x - cx) + Math.PI / 2;
  if (a < 0) a += Math.PI * 2;
  const g = a / (Math.PI * 2);
  if (Math.abs(g - last) > 0.5) return last > 0.5 ? 1 : 0;
  return clamp(g, 0, 1);
}

// ---- words on the screen ----

export const dbText = (db: number) => {
  const v = Math.round(db);
  return `${v > 0 ? '+' : v < 0 ? '−' : ''}${Math.abs(v)} dB`;
};

export const panText = (p: number) => {
  const v = Math.round(Math.abs(p) * 100);
  return v < 3 ? 'Pan center' : `Pan ${p < 0 ? 'L' : 'R'} ${v}`;
};

export const zone = (d: number) => (d < 0.3 ? 'Close' : d < 0.7 ? 'Room' : 'Far');

/** How a ball announces itself. */
export function announce(s: { name: string; bucket: string; start_s: number; len_s: number; gain_db: number; pan: number; depth: number }, time: (t: number) => string) {
  return `${s.name}, ${s.bucket}, starts ${time(s.start_s)}, ${s.len_s.toFixed(1)} seconds, loudness ${dbText(s.gain_db)}, ${panText(s.pan)}, depth ${zone(s.depth)} ${Math.round(s.depth * 100)} percent`;
}
