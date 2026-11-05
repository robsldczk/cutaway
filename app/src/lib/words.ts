// What the editor reads: a line's words as they were said (D105). What the recogniser heard,
// or what was typed over it in the mapping editor. A take can say more than the script does,
// or something else, and the card shows it.

/** A word of a take, in any time base (scene time, or take time). */
export interface Heard {
  idx: number;
  text: string;
  start: number;
  end: number;
  deleted?: boolean;
  muted?: boolean;
  part?: number;
}

export interface ShownWord {
  /** Index among the line's words (key). */
  j: number;
  text: string;
  start: number;
  end: number;
  deleted: boolean;
  muted: boolean;
  part: number;
  /** The word a delete or restore acts on. */
  idxs: number[];
}

export function spokenWords(heard: Heard[]): ShownWord[] {
  return [...heard]
    .sort((a, b) => a.start - b.start || a.idx - b.idx)
    .map((w) => ({ j: w.idx, text: w.text, start: w.start, end: w.end, deleted: !!w.deleted, muted: !!w.muted, part: w.part ?? 0, idxs: [w.idx] }));
}

/** A line's words on the scene timeline (from its layout words). */
export function lineWords(it: { words: { idx: number; text: string; start_s: number; end_s: number; deleted: boolean; muted: boolean; part: number }[] }): ShownWord[] {
  return spokenWords(it.words.map((w) => ({ idx: w.idx, text: w.text, start: w.start_s, end: w.end_s, deleted: w.deleted, muted: w.muted, part: w.part })));
}

/** A take's words in take time (for the box editor's take lanes). */
export function takeWords(words: { text: string; t0: number; t1: number }[]): ShownWord[] {
  return spokenWords(words.map((w, i) => ({ idx: i, text: w.text, start: w.t0, end: w.t1 })));
}

/** A line as it plays: what's kept of it, or the script text when nothing plays. */
export function lineText(it: Parameters<typeof lineWords>[0] & { text: string; dropped: boolean }, part?: number): string {
  if (it.dropped || !it.words.length) return part == null ? it.text : '';
  const ws = lineWords(it).filter((w) => !w.deleted && (part == null || w.part === part));
  return ws.map((w) => w.text).join(' ') || (part == null ? it.text : '');
}
