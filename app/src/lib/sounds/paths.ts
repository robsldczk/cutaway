// SVG paths: the spine and the length lines as a few paths, not one element per sound.
import type { ItemView, PieceView, TakeView } from '../types';
import type { TimeMap } from './timemap';

/** Height of a spine sample in px. */
const STEP_PX = 2;
/** The flat line of a silence, either side of the centre. */
const FLOOR = 0.4;

type SpineLine = Pick<ItemView, 'character' | 'pieces' | 'takes' | 'end_s' | 'start_s'>;

/** Level of a take at a source time, 0…1. */
function level(t: TakeView, src: number) {
  if (!t.peaks.length || t.duration <= 0) return 0;
  const i = Math.floor(((src - t.src_in) / t.duration) * t.peaks.length);
  return t.peaks[Math.max(0, Math.min(t.peaks.length - 1, i))] ?? 0;
}

function piecePath(p: PieceView, take: TakeView | undefined, map: Pick<TimeMap, 'y'>, x: number, amp: number) {
  const y0 = map.y(p.start_s);
  const y1 = map.y(p.start_s + p.len_s);
  if (y1 - y0 < 1 || p.len_s <= 0) return '';
  const n = Math.max(2, Math.round((y1 - y0) / STEP_PX));
  const right: string[] = [];
  const left: string[] = [];
  for (let k = 0; k <= n; k++) {
    const f = k / n;
    const t = p.start_s + f * p.len_s;
    const a = Math.max(FLOOR, p.muted || !take ? FLOOR : level(take, p.src_in + f * (p.src_out - p.src_in)) * amp);
    const y = map.y(t).toFixed(1);
    right.push(`${k ? 'L' : 'M'}${(x + a).toFixed(1)} ${y}`);
    left.push(`L${(x - a).toFixed(1)} ${y}`);
  }
  return `${right.join('')}${left.reverse().join('')}Z`;
}

/** The stretch of each line that falls between two times, for a view of that stretch alone. */
export function clipLines<L extends SpineLine>(lines: L[], t0: number, t1: number): L[] {
  const out: L[] = [];
  for (const it of lines) {
    const pieces: PieceView[] = [];
    for (const p of it.pieces) {
      const a = Math.max(p.start_s, t0);
      const z = Math.min(p.start_s + p.len_s, t1);
      if (z <= a || p.len_s <= 0) continue;
      const rate = (p.src_out - p.src_in) / p.len_s;
      pieces.push({ ...p, start_s: a, len_s: z - a, src_in: p.src_in + (a - p.start_s) * rate, src_out: p.src_in + (z - p.start_s) * rate });
    }
    if (pieces.length) out.push({ ...it, pieces });
  }
  return out;
}

/** One path per character. */
export function spinePaths(lines: SpineLine[], map: Pick<TimeMap, 'y'>, x: number, amp = 12): Map<string, string> {
  const by = new Map<string, string>();
  for (const it of lines) {
    const key = it.character ?? '';
    for (const p of it.pieces) {
      const d = piecePath(
        p,
        it.takes.find((t) => t.id === p.take_id),
        map,
        x,
        amp,
      );
      if (d) by.set(key, (by.get(key) ?? '') + d);
    }
  }
  return by;
}

/** A vertical length line. */
export const linePath = (x: number, y0: number, y1: number) => `M${x} ${y0.toFixed(1)}V${y1.toFixed(1)}`;

/** The thick line of the sound in focus, with its fade as a wedge. */
export function focusBody(x: number, y0: number, yFade: number) {
  const y = y0.toFixed(1);
  return `M${x - 3} ${(y0 + 3).toFixed(1)}Q${x - 3} ${y} ${x} ${y}Q${x + 3} ${y} ${x + 3} ${(y0 + 3).toFixed(1)}V${yFade.toFixed(1)}H${x - 3}Z`;
}

export const focusFade = (x: number, yFade: number, y1: number) => `M${x - 3} ${yFade.toFixed(1)}H${x + 3}L${x} ${y1.toFixed(1)}Z`;

/** The dashed rest of the source past the end of the sound. */
export const restOfSource = (x: number, y1: number, yEnd: number) => (yEnd - y1 > 2 ? `M${x - 3} ${(y1 + 2).toFixed(1)}V${yEnd.toFixed(1)}H${x + 3}V${(y1 + 2).toFixed(1)}` : '');

export function circle(x: number, y: number, r: number) {
  return `M${(x - r).toFixed(1)} ${y.toFixed(1)}a${r} ${r} 0 1 0 ${2 * r} 0a${r} ${r} 0 1 0 ${-2 * r} 0`;
}

/** The ring's arc from the top, clockwise, to `g` of the way round. */
export function arc(x: number, y: number, r: number, g: number) {
  if (g <= 0.005) return '';
  const a1 = -Math.PI / 2 + g * Math.PI * 2 * 0.999;
  return `M${x.toFixed(1)} ${(y - r).toFixed(1)}A${r} ${r} 0 ${g > 0.5 ? 1 : 0} 1 ${(x + r * Math.cos(a1)).toFixed(1)} ${(y + r * Math.sin(a1)).toFixed(1)}`;
}

/** Where the ring's knob sits. */
export function ringPoint(x: number, y: number, r: number, g: number) {
  const a = -Math.PI / 2 + g * Math.PI * 2 * 0.999;
  return { x: x + r * Math.cos(a), y: y + r * Math.sin(a) };
}

/** The meridian on a ball: a globe turned by the pan. In a 0…100 box. */
export function panLine(pan: number) {
  const bulge = 50 * Math.sin(pan * 1.25) + (Math.abs(pan) < 0.02 ? 9 : 0);
  return `M50 3Q${(50 + 2 * bulge).toFixed(1)} 50 50 97`;
}

/** A waveform running down a column between heights `y0` and `y1`. `level` gives each height
 *  its 0…1 level. */
export function columnWave(x: number, amp: number, y0: number, y1: number, level: (y: number) => number): string {
  if (y1 - y0 < 1) return '';
  const n = Math.max(2, Math.round((y1 - y0) / STEP_PX));
  const right: string[] = [];
  const left: string[] = [];
  for (let k = 0; k <= n; k++) {
    const y = y0 + (k / n) * (y1 - y0);
    const a = Math.max(FLOOR, level(y) * amp);
    right.push(`${k ? 'L' : 'M'}${(x + a).toFixed(1)} ${y.toFixed(1)}`);
    left.push(`L${(x - a).toFixed(1)} ${y.toFixed(1)}`);
  }
  return `${right.join('')}${left.reverse().join('')}Z`;
}

/** A peak at a share of its stretch, 0…1. */
export const peakAt = (peaks: number[], f: number) => (peaks.length && f >= 0 && f <= 1 ? (peaks[Math.min(peaks.length - 1, Math.floor(f * peaks.length))] ?? 0) : 0);
