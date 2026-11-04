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

/** The nearest target within `px` on the spine. */
export function nearestTarget(targets: SnapTarget[], t: number, map: TimeMap, px = SNAP_PX): SnapTarget | null {
  const y = map.y(t);
  let best: SnapTarget | null = null;
  let bd = px;
  for (const s of targets) {
    const d = Math.abs(map.y(s.t) - y);
    if (d < bd) {
      bd = d;
      best = s;
    }
  }
  return best;
}

export const startLabel = (target: SnapTarget | null, t: number) => target?.start ?? `Starts at ${fmt(t)}`;
