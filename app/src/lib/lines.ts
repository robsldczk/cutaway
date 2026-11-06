// Dialogue lines on Arrange: where a moment falls on a block, which lines are selected, and what
// a line can hand on to another. Pure, so the page stays about the page.

import { lineWords } from './words';
import { groupDelta, oneStep } from './sounds/select';
import type { ItemView, PieceView } from './types';

type Cmd = Record<string, unknown>;
type Pieces = { pieces: PieceView[] };

/** The stretch of a line playing at scene time `t`: the last that had started by then. In a
 *  pause between two parts that's the one before it. */
function pieceAt(it: Pieces, t: number) {
  let at: PieceView | undefined;
  for (const p of it.pieces) if (p.start_s <= t && (!at || p.start_s >= at.start_s)) at = p;
  return at ?? it.pieces[0];
}

/** Where in `source` the line is at scene time `t`. Cut words and splits mean a line doesn't run
 *  straight through its take, so this goes through what actually plays. `null` when that moment
 *  comes from another recording (a comp). */
export function sourceAt(it: Pieces, t: number, source: string | undefined): number | null {
  const p = pieceAt(it, t);
  if (!p || p.source_id !== source) return null;
  return p.src_in + Math.max(0, Math.min(t - p.start_s, p.len_s));
}

/** The scene time `s` in `source` plays at: past a cut, where the line goes on. `null` when none
 *  of the line comes from there, or all of it has played by then. */
export function timeAt(it: Pieces, s: number, source: string | undefined): number | null {
  const own = it.pieces.filter((p) => p.source_id === source).sort((a, b) => a.src_in - b.src_in);
  const p = own.find((x) => s < x.src_out);
  if (p) return p.start_s + Math.max(0, s - p.src_in);
  const last = own[own.length - 1];
  return last ? last.start_s + last.len_s : null;
}

/** The word being said at scene time `t`. */
export function wordAt(it: Parameters<typeof lineWords>[0], t: number): string | null {
  return lineWords(it).find((w) => !w.deleted && t >= w.start && t <= w.end)?.text ?? null;
}

/** The word under a moment, of any line that's speaking then. */
export function spokenAt(lines: ItemView[], t: number): string | null {
  for (const l of lines) {
    if (t < l.start_s || t > l.end_s) continue;
    const w = wordAt(l, t);
    if (w) return w;
  }
  return null;
}

// ---- selection ----

/** ⌘ click: a line in or out of the selection, which starts from the current line. */
export function toggleLine(sel: string[], cur: string | null, id: string): string[] {
  const from = sel.length ? sel : cur ? [cur] : [];
  return from.includes(id) ? from.filter((x) => x !== id) : [...from, id];
}

/** ⇧ click: every line from `from` to `id`, either way round. */
export function rangeLines(order: string[], from: string | null, id: string): string[] {
  const a = from ? order.indexOf(from) : -1;
  const b = order.indexOf(id);
  if (b < 0) return [];
  if (a < 0) return [id];
  return order.slice(Math.min(a, b), Math.max(a, b) + 1);
}

// ---- depth ----

/** Named places on the depth scale, for the line's menu. */
export const DEPTH_STOPS = [
  { label: 'Close', depth: 0 },
  { label: 'A little back', depth: 0.25 },
  { label: 'Back', depth: 0.5 },
  { label: 'Far', depth: 0.8 },
];

export const depthText = (d: number) => (d <= 0 ? 'Close' : `Back ${Math.round(d * 100)} %`);

export const depthCommands = (ids: string[], depth: (id: string) => number): Cmd[] => ids.map((line_id) => ({ cmd: 'set_clip_depth', line_id, depth: +depth(line_id).toFixed(3) }));

/** How far a group can go by `delta`: it moves as one and stops at either end. */
export const depthDelta = (depths: number[], delta: number) => groupDelta(depths, delta, 0, 1);

// ---- attributes ----

/** What a line hands on: how it sounds, not what it is or when it comes. */
export interface LineAttrs {
  depth: number;
  gain_db: number;
  fade_in_s: number;
  fade_out_s: number;
}

export const attrsOf = (l: LineAttrs): LineAttrs => ({ depth: l.depth, gain_db: l.gain_db, fade_in_s: l.fade_in_s, fade_out_s: l.fade_out_s });

export function attrsText(a: LineAttrs) {
  const parts = [depthText(a.depth)];
  if (a.gain_db) parts.push(`${a.gain_db > 0 ? '+' : '−'}${Math.abs(a.gain_db).toFixed(1)} dB`);
  if (a.fade_in_s || a.fade_out_s) parts.push('fades');
  return parts.join(' · ');
}

/** The attributes on every line, as one undo step. */
export function pasteCommand(ids: string[], a: LineAttrs): Cmd | null {
  return oneStep(ids.flatMap((line_id) => [
    { cmd: 'set_clip_depth', line_id, depth: a.depth },
    { cmd: 'set_clip_gain', line_id, gain_db: a.gain_db },
    { cmd: 'set_clip_fades', line_id, fade_in_s: a.fade_in_s, fade_out_s: a.fade_out_s },
  ]));
}
