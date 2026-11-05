// The mapping editor's arithmetic: which stretch of a recording shows, what's between the marks.
import type { RecordingWord } from './types';

/** A stretch of a recording, in source-file seconds. */
export interface Span {
  t0: number;
  t1: number;
}

/** Closest and furthest the view zooms, in seconds across. */
export const MIN_SPAN_S = 1.5;
export const MAX_SPAN_S = 240;
/** What a recording opens on when nothing is marked. */
export const OPEN_SPAN_S = 14;
/** Shortest stretch that makes a take. */
export const MIN_MARK_S = 0.05;

/** The view kept inside the file, keeping its width when it can. */
export function fit(v: Span, duration: number): Span {
  const span = Math.min(Math.max(v.t1 - v.t0, MIN_SPAN_S), MAX_SPAN_S, Math.max(duration, MIN_SPAN_S));
  const t0 = Math.max(0, Math.min(v.t0, duration - span));
  return { t0, t1: t0 + span };
}

/** The view around a stretch, with the audio either side to look at. */
export function around(t0: number, t1: number, duration: number): Span {
  const span = Math.max(OPEN_SPAN_S, (t1 - t0) * 2.5);
  const mid = (t0 + t1) / 2;
  return fit({ t0: mid - span / 2, t1: mid + span / 2 }, duration);
}

export function pan(v: Span, by: number, duration: number): Span {
  return fit({ t0: v.t0 + by, t1: v.t1 + by }, duration);
}

/** Zooms around `at`, which stays under the mouse. `factor` below 1 zooms in. */
export function zoom(v: Span, at: number, factor: number, duration: number): Span {
  const span = Math.min(Math.max((v.t1 - v.t0) * factor, MIN_SPAN_S), MAX_SPAN_S);
  const share = (at - v.t0) / (v.t1 - v.t0);
  const t0 = at - share * span;
  return fit({ t0, t1: t0 + span }, duration);
}

/** The words of a stretch: those with most of their sound inside, as the take will hold them. */
export function wordsBetween(words: RecordingWord[], t0: number, t1: number): RecordingWord[] {
  return words.filter((w) => {
    const mid = (w.t0 + w.t1) / 2;
    return mid >= t0 && mid < t1;
  });
}

export function textBetween(words: RecordingWord[], t0: number, t1: number): string {
  return wordsBetween(words, t0, t1)
    .map((w) => w.text.trim())
    .join(' ');
}

/** A mic's words with one take's own over its stretch: what was typed there replaces what the
 *  recogniser heard. A take's words are in take time. */
export function withTake(words: RecordingWord[], take: { src_in: number; src_out: number; words: RecordingWord[] }): RecordingWord[] {
  const own = take.words.map((w) => ({ text: w.text, t0: w.t0 + take.src_in, t1: w.t1 + take.src_in }));
  const rest = words.filter((w) => {
    const mid = (w.t0 + w.t1) / 2;
    return mid < take.src_in || mid >= take.src_out;
  });
  return [...rest, ...own].sort((a, b) => a.t0 - b.t0);
}

export interface PlacedWord extends RecordingWord {
  row: number;
}

/** The words in view on as few rows as keep them apart; what fits on none stays out. `pxPerS` is
 *  the view's scale, `pxPerChar` the width of a letter. */
export function placeWords(words: RecordingWord[], v: Span, pxPerS: number, pxPerChar: number, rows: number): PlacedWord[] {
  const free = new Array<number>(rows).fill(-Infinity);
  const out: PlacedWord[] = [];
  for (const w of words) {
    if (w.t1 < v.t0 || w.t0 > v.t1) continue;
    const x = (w.t0 - v.t0) * pxPerS;
    const row = free.findIndex((end) => x >= end);
    if (row < 0) continue;
    free[row] = x + w.text.length * pxPerChar + 6;
    out.push({ ...w, row });
  }
  return out;
}

const STEPS = [0.5, 1, 2, 5, 10, 15, 30, 60, 120];

/** Times to mark on the ruler, about every `minPx`. */
export function ticks(v: Span, pxPerS: number, minPx = 90): number[] {
  const step = STEPS.find((s) => s * pxPerS >= minPx) ?? 300;
  const out: number[] = [];
  for (let t = Math.ceil(v.t0 / step) * step; t <= v.t1; t += step) out.push(t);
  return out;
}

/** m:ss.cc, the way a stretch of a long recording reads. */
export function clock(t: number): string {
  const cs = Math.round(Math.max(t, 0) * 100);
  const m = Math.floor(cs / 6000);
  const s = Math.floor((cs % 6000) / 100);
  return `${m}:${String(s).padStart(2, '0')}.${String(cs % 100).padStart(2, '0')}`;
}
