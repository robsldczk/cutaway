// The opened sound: a vertical editor zoomed to one sound. Everything a gesture or a key does
// to it is worked out here, as the same three steps: where the part wants to go, what the
// sound looks like then (the preview), and what goes to the document on release.
import type { Anchor, ItemView } from '../types';
import { clamp, fmt } from '../format';
import { autoFadeOut, moveCommands, timeDrag, type Cmd, type SoundState } from './select';
import { SNAP_PX, startLabel, type SnapTarget } from './snap';

/** Shortest length a trim leaves, as on the length line. */
export const MIN_TRIM_S = 0.2;
/** A source up to this long shows whole; a longer one shows the sound and a second each side. */
export const SHORT_SOURCE_S = 8;
/** The window keeps this much room around the sound; closer and it fits itself again. */
const WINDOW_EDGE_S = 0.1;
/** How far past the start of a sound a line must reach for the quick buttons to count it. */
const NEXT_LINE_S = 0.05;
/** A fade out leaves this share of the sound. */
const FADE_MAX = 0.9;
const NEAR_S = 0.001;
const QUICK_NEAR_S = 0.02;

/** What the opened view needs of a sound. */
export interface OpenState extends SoundState {
  in_s: number;
  asset_len_s: number;
  looped?: boolean;
  fade_in_s: number;
  fade_out_s: number;
  auto_fade: boolean;
}

export type Shape = Pick<OpenState, 'start_s' | 'len_s' | 'in_s' | 'fade_in_s' | 'fade_out_s' | 'auto_fade'>;
export type Part = 'in' | 'out' | 'fadeIn' | 'fadeOut' | 'move' | 'slip';
export type Quick = 'line' | 'next' | 'full';

/** What the opened view asks of the sounds layer, which holds the preview and sends commands. */
export interface Ops {
  /** A gesture takes the sound; it ends in `finish` or `cancel`. */
  begin(): void;
  preview(patch: Partial<Shape>): void;
  finish(cmds: Cmd[], said: string): void;
  cancel(): void;
  /** A click: one change, at once. */
  apply(patch: Partial<Shape>, cmds: Cmd[], said: string): void;
  /** A key press on a part: bursts end in one command. */
  nudge(part: Part, delta: number): void;
}

// ---- the scrubber: a place in the sound's source, shown in scene time ----

/** How far into the file a scene time is; a loop comes round. */
export function sourceAt(s: Pick<OpenState, 'start_s' | 'in_s' | 'asset_len_s'> & { looped?: boolean }, t: number) {
  const at = t - (s.start_s - s.in_s);
  if (s.asset_len_s <= 0) return Math.max(0, at);
  return s.looped ? ((at % s.asset_len_s) + s.asset_len_s) % s.asset_len_s : clamp(at, 0, s.asset_len_s);
}

/** Where the scrubber can go: the source, on the view. */
export function scrubLimits(s: Pick<OpenState, 'start_s' | 'len_s' | 'in_s' | 'asset_len_s'> & { looped?: boolean }) {
  const from = s.start_s - s.in_s;
  return { from, to: s.asset_len_s > 0 && !s.looped ? from + s.asset_len_s : s.start_s + s.len_s };
}

// ---- the window: what stretch of time the view shows ----

export interface Win {
  t0: number;
  len: number;
}

export function openWindow(s: Pick<OpenState, 'start_s' | 'len_s' | 'in_s' | 'asset_len_s'>): Win {
  const full = s.asset_len_s > 0 ? s.asset_len_s : s.in_s + s.len_s;
  if (full <= SHORT_SOURCE_S) return { t0: s.start_s - s.in_s - 0.25, len: full + 0.5 };
  return { t0: s.start_s - 1, len: s.len_s + 2 };
}

/** The window stays put through a gesture; after one it follows a sound that reached its edge. */
export function keepWindow(w: Win, s: Pick<OpenState, 'start_s' | 'len_s' | 'in_s' | 'asset_len_s'>): Win {
  const inside = s.start_s >= w.t0 + WINDOW_EDGE_S && s.start_s + s.len_s <= w.t0 + w.len - WINDOW_EDGE_S;
  return inside ? w : openWindow(s);
}

export const yOf = (w: Win, h: number, t: number) => ((t - w.t0) / w.len) * h;
export const tOf = (w: Win, h: number, y: number) => w.t0 + (y / h) * w.len;

/** The spine's snap distance, in the view's time. */
export const snapTol = (w: Win, h: number, px = SNAP_PX) => (px / h) * w.len;

/** The nearest target within `tol` seconds. */
export function nearestIn(targets: SnapTarget[], t: number, tol: number): SnapTarget | null {
  let best: SnapTarget | null = null;
  let bd = tol;
  for (const s of targets) {
    const d = Math.abs(s.t - t);
    if (d < bd) {
      bd = d;
      best = s;
    }
  }
  return best;
}

// ---- reshaping ----

/** The longest the sound can be from its in point. */
export const maxLen = (s: Pick<OpenState, 'in_s' | 'asset_len_s'> & { looped?: boolean }) => (s.asset_len_s > 0 && !s.looped ? s.asset_len_s - s.in_s : Infinity);

const minLen = (s: Pick<OpenState, 'len_s'>) => Math.min(MIN_TRIM_S, s.len_s);

/** What the core does to the fades at a new length (`trim_placement`, then `settle`). */
export function fadesAt(s: Pick<OpenState, 'fade_in_s' | 'fade_out_s' | 'auto_fade'>, len: number) {
  const fade_out_s = s.auto_fade ? autoFadeOut(len) : clamp(Math.min(s.fade_out_s, len / 2), 0, len);
  return { fade_in_s: clamp(Math.min(s.fade_in_s, len / 2), 0, len - fade_out_s), fade_out_s };
}

/** Where a part sits, in scene time. The audio's own start stands for a slip. */
export function partAt(part: Part, s: Shape) {
  if (part === 'in' || part === 'move') return s.start_s;
  if (part === 'out') return s.start_s + s.len_s;
  if (part === 'fadeIn') return s.start_s + s.fade_in_s;
  if (part === 'fadeOut') return s.start_s + s.len_s - s.fade_out_s;
  return s.start_s - s.in_s;
}

export interface Reshaped {
  /** Where the part ended up. */
  at: number;
  patch: Partial<Shape>;
  /** The snap it reached; none when a limit kept it away. */
  target: SnapTarget | null;
}

const reached = (target: SnapTarget | null, t: number) => (target && Math.abs(target.t - t) < NEAR_S ? target : null);

/** The sound with `part` wanted at `wanted`, or at the target that caught it. */
export function reshape(part: Part, s: OpenState, wanted: number, target: SnapTarget | null = null): Reshaped {
  const end = s.start_s + s.len_s;
  const want = target?.t ?? wanted;
  if (part === 'in') {
    const src0 = s.start_s - s.in_s;
    const at = clamp(want, Math.max(src0, 0), end - minLen(s));
    const len_s = end - at;
    return { at, patch: { start_s: at, in_s: at - src0, len_s, ...fadesAt(s, len_s) }, target: reached(target, at) };
  }
  if (part === 'out') {
    const len_s = clamp(want - s.start_s, minLen(s), maxLen(s));
    const at = s.start_s + len_s;
    return { at, patch: { len_s, ...fadesAt(s, len_s) }, target: reached(target, at) };
  }
  if (part === 'fadeIn') {
    const fade_in_s = clamp(want - s.start_s, 0, s.len_s - s.fade_out_s);
    return { at: s.start_s + fade_in_s, patch: { fade_in_s }, target: null };
  }
  if (part === 'fadeOut') {
    const fade_out_s = clamp(end - want, 0, s.len_s * FADE_MAX);
    return { at: end - fade_out_s, patch: { fade_out_s, fade_in_s: Math.min(s.fade_in_s, s.len_s - fade_out_s), auto_fade: false }, target: null };
  }
  if (part === 'move') {
    const r = timeDrag([s], s.id, wanted, target);
    const at = s.start_s + r.delta;
    return { at, patch: { start_s: at }, target: r.target };
  }
  const in_s = clamp(s.start_s - want, 0, Math.max(0, s.asset_len_s > 0 ? s.asset_len_s - s.len_s : Infinity));
  return { at: s.start_s - in_s, patch: { in_s }, target: null };
}

const sameAnchor = (a: Anchor | null, b: Anchor) => !!a && a.line_id === b.line_id && a.word === b.word && a.edge === b.edge;
const ms = (x: number) => Math.round(x * 1000) / 1000;
const changed = (a: number, b: number) => Math.abs(a - b) >= NEAR_S / 2;

/** What a finished gesture sends, empty when nothing changed. A start trimmed onto a target
 *  hangs on it (M7); one trimmed anywhere else keeps its anchor, so the audio stays where it
 *  was against the line. `keep`: a key press, which keeps the anchor as all key moves do. */
export function reshapeCommands(part: Part, s: OpenState, r: Reshaped, keep = false): Cmd[] {
  const placement_id = s.id;
  if (part === 'in') {
    if (!changed(r.at, s.start_s)) return [];
    const trim: Cmd = { cmd: 'trim_placement', placement_id, edge: 'start', delta_s: ms(r.at - s.start_s) };
    return r.target && !s.unglued ? [trim, { cmd: 'move_sound', placement_id, anchor: r.target.anchor, offset_s: r.target.offset_s ?? 0 }] : [trim];
  }
  if (part === 'out') {
    const delta = r.at - (s.start_s + s.len_s);
    return changed(delta, 0) ? [{ cmd: 'trim_placement', placement_id, edge: 'end', delta_s: ms(delta) }] : [];
  }
  if (part === 'fadeIn') {
    const f = r.patch.fade_in_s ?? s.fade_in_s;
    return changed(f, s.fade_in_s) ? [{ cmd: 'set_sound_fades', placement_id, fade_in_s: ms(f) }] : [];
  }
  if (part === 'fadeOut') {
    const f = r.patch.fade_out_s ?? s.fade_out_s;
    return changed(f, s.fade_out_s) ? [{ cmd: 'set_sound_fades', placement_id, fade_out_s: ms(f) }] : [];
  }
  if (part === 'move') {
    const delta = r.at - s.start_s;
    if (!changed(delta, 0) && (!r.target || sameAnchor(s.anchor, r.target.anchor))) return [];
    return moveCommands([s], delta, keep ? null : s.id, r.target);
  }
  const in_s = r.patch.in_s ?? s.in_s;
  return changed(in_s, s.in_s) ? [{ cmd: 'slip_sound', placement_id, in_s: ms(in_s) }] : [];
}

// ---- quick buttons ----

type Line = Pick<ItemView, 'kind' | 'start_s' | 'end_s' | 'pieces'>;

/** The length a quick button asks for. With no such line, the rest of the source. */
export function quickLen(kind: Quick, s: Pick<OpenState, 'start_s' | 'len_s' | 'in_s' | 'asset_len_s'>, lines: Line[]): number {
  const max = maxLen(s);
  const full = Number.isFinite(max) ? max : s.len_s;
  if (kind === 'full') return full;
  const spoken = lines.filter((l) => l.kind === 'line' && l.pieces.length && l.end_s > l.start_s);
  const after = s.start_s + NEXT_LINE_S;
  const ends = spoken.map((l) => (kind === 'line' ? l.end_s : l.start_s)).filter((t) => t > after);
  if (!ends.length) return full;
  return clamp(Math.min(...ends) - s.start_s, minLen(s), max);
}

export const quickOn = (kind: Quick, s: Pick<OpenState, 'start_s' | 'len_s' | 'in_s' | 'asset_len_s'>, lines: Line[]) => Math.abs(quickLen(kind, s, lines) - s.len_s) < QUICK_NEAR_S;

/** The Auto fade button: on, the fade out follows the length; off, it stays as it is now. */
export function autoFadeToggle(s: Pick<OpenState, 'id' | 'auto_fade' | 'len_s' | 'fade_out_s'>) {
  const auto = !s.auto_fade;
  const patch: Partial<Shape> = { auto_fade: auto, fade_out_s: auto ? autoFadeOut(s.len_s) : s.fade_out_s };
  return { patch, cmd: { cmd: 'set_sound_fades', placement_id: s.id, auto } as Cmd };
}

// ---- words on the screen ----

/** Level of the sound at a scene time, 0…1, through its fades. */
export function fadeLevel(s: Pick<Shape, 'start_s' | 'len_s' | 'fade_in_s' | 'fade_out_s'>, t: number) {
  const a = t - s.start_s;
  const z = s.start_s + s.len_s - t;
  if (a < 0 || z < 0) return 0;
  const up = s.fade_in_s > 0 ? Math.min(1, a / s.fade_in_s) : 1;
  const down = s.fade_out_s > 0 ? Math.min(1, z / s.fade_out_s) : 1;
  return Math.min(up, down);
}

/** The line under the header's readings. */
export function readout(s: Pick<OpenState, 'len_s' | 'in_s' | 'asset_len_s' | 'fade_in_s' | 'fade_out_s' | 'auto_fade'>) {
  const full = s.asset_len_s > 0 ? ` of ${s.asset_len_s.toFixed(1)}` : '';
  return `${s.len_s.toFixed(1)}${full} s · fade in ${s.fade_in_s.toFixed(2)} · fade out ${s.fade_out_s.toFixed(1)}${s.auto_fade ? ' auto' : ''}`;
}

/** What a part reads while it changes, for the tip and for a screen reader. */
export function partText(part: Part, s: OpenState, r: Reshaped) {
  const next = { ...s, ...r.patch };
  if (part === 'in') return r.target?.start ?? `Starts at ${fmt(next.start_s)} · ${next.len_s.toFixed(1)} s`;
  if (part === 'out') {
    const full = Math.abs(next.len_s - maxLen(next)) < 0.01;
    return `${full ? 'Full length · ' : r.target ? `${r.target.end} · ` : ''}${next.len_s.toFixed(1)} s · fades out ${next.fade_out_s.toFixed(1)} s`;
  }
  if (part === 'fadeIn') return `Fade in ${next.fade_in_s.toFixed(2)} s`;
  if (part === 'fadeOut') return `Fade out ${next.fade_out_s.toFixed(2)} s`;
  if (part === 'move') return startLabel(r.target, next.start_s);
  return `Slipped to ${next.in_s.toFixed(2)} s into the source`;
}
