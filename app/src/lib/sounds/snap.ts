// Snap targets: where a sound can hang on the dialogue. A target carries the anchor it stands
// for (M7), so a snapped sound follows its line when takes, gaps or the order change.
import type { Anchor, ItemView } from '../types';
import { fmt } from '../format';
import type { TimeMap } from './timemap';

/** Snap distance on the spine. */
export const SNAP_PX = 8;
/** Shortest silence between two words that counts as a pause. */
export const PAUSE_S = 0.12;

export interface SnapTarget {
  t: number;
  anchor: Anchor;
  /** What the tooltip says when a sound starts here, and when one ends here. */
  start: string;
  end: string;
  /** How far past its anchor it sits: an action has no time of its own, so a sound hangs on
   *  it at the place in the pause where the action is written. */
  offset_s?: number;
  /** An action's row: where it sits, and how far from there it catches. */
  y?: number;
  reach?: number;
}

/** An action line as Arrange shows it. */
export interface ActionRow {
  line_id: string;
  text: string;
  start_s: number;
  /** Where the next line starts; an action stays in its pause. */
  until: number;
  y: number;
  height: number;
}

const word = (it: ItemView, script_idx: number | null, text: string) => (script_idx != null ? (it.tokens[script_idx] ?? text) : text);

export function snapTargets(lines: ItemView[], pauseS = PAUSE_S): SnapTarget[] {
  const out: SnapTarget[] = [];
  for (const it of lines) {
    // a line with no audio on the timeline has nothing an anchor could resolve to
    if (it.kind !== 'line' || !it.pieces.length || it.end_s <= it.start_s) continue;
    const kept = it.words.filter((w) => !w.deleted).sort((a, b) => a.start_s - b.start_s);
    const first = kept.slice(0, 2).map((w) => word(it, w.script_idx, w.text));
    const opening = first.length ? first.join(' ') : it.text.split(/\s+/).slice(0, 2).join(' ');
    const last = kept.length ? word(it, kept[kept.length - 1].script_idx, kept[kept.length - 1].text) : (it.text.split(/\s+/).pop() ?? '');
    out.push({
      t: it.start_s,
      anchor: { line_id: it.line_id, word: null, edge: 'start' },
      start: `On “${opening}”`,
      end: `Ends before “${opening}”`,
    });
    out.push({
      t: it.end_s,
      anchor: { line_id: it.line_id, word: null, edge: 'end' },
      start: `After “${last}”`,
      end: `Ends with “${last}”`,
    });
    for (let i = 0; i + 1 < kept.length; i++) {
      const w = kept[i];
      if (w.script_idx == null || kept[i + 1].start_s - w.end_s < pauseS) continue;
      // the anchor resolves to the first kept word with this script index: stay on that one
      if (kept.find((x) => x.script_idx === w.script_idx) !== w) continue;
      const name = word(it, w.script_idx, w.text);
      out.push({
        t: w.end_s,
        anchor: { line_id: it.line_id, word: w.script_idx, edge: 'end' },
        start: `In the pause after “${name}”`,
        end: `Ends in the pause after “${name}”`,
      });
    }
  }
  return out.sort((a, b) => a.t - b.t);
}

/** Action lines as targets, each at the time beside its row. */
export function actionTargets(rows: ActionRow[], map: TimeMap): SnapTarget[] {
  return rows.map((r) => {
    const t = Math.min(Math.max(map.t(r.y), r.start_s), Math.max(r.until, r.start_s));
    const words = r.text.split(/\s+/).slice(0, 4).join(' ');
    return {
      t,
      anchor: { line_id: r.line_id, word: null, edge: 'start' as const },
      offset_s: +(t - r.start_s).toFixed(3),
      start: `On “${words}…”`,
      end: `Ends on “${words}…”`,
      y: r.y,
      reach: r.height / 2,
    };
  });
}

/** The nearest target within `px` on the spine, or within an action's row. `y` is the height
 *  asked about, where a pause with no time in it can't tell heights apart. */
export function nearestTarget(targets: SnapTarget[], t: number, map: TimeMap, px = SNAP_PX, y = map.y(t)): SnapTarget | null {
  let best: SnapTarget | null = null;
  let bd = Infinity;
  for (const s of targets) {
    const d = Math.abs((s.y ?? map.y(s.t)) - y);
    if (d < (s.reach ?? px) && d < bd) {
      bd = d;
      best = s;
    }
  }
  return best;
}

export const startLabel = (target: SnapTarget | null, t: number) => target?.start ?? `Starts at ${fmt(t)}`;
