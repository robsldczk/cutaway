// What the editor reads: a line's script words, timed by what the recognizer heard.
// Matched words keep their timing; words it missed or misheard are spread over the gap
// they were spoken in. Display only: edits still address the heard words (`idxs`).

/** A heard word, in any time base (scene time, or take time). */
export interface Heard {
  idx: number;
  start: number;
  end: number;
  script_idx: number | null;
  deleted?: boolean;
  muted?: boolean;
  part?: number;
}

export interface ShownWord {
  /** Script word index (key). */
  j: number;
  text: string;
  start: number;
  end: number;
  deleted: boolean;
  muted: boolean;
  part: number;
  /** The heard words this one stands for (what a delete or restore acts on). */
  idxs: number[];
  /** Placed by interpolation, not heard as such. */
  guessed: boolean;
}

/** Narrowest a word squeezed into a missing gap gets (s). */
const MIN_W = 0.06;

export function scriptWords(tokens: string[], heard: Heard[]): ShownWord[] {
  if (!tokens.length || !heard.length) return [];
  const words = [...heard].sort((a, b) => a.start - b.start || a.idx - b.idx);
  const kept = words.filter((w) => !w.deleted);
  const byJ = new Map<number, ShownWord>();
  const add = (w: Heard, j: number, deleted: boolean) => {
    const s = byJ.get(j);
    if (!s) {
      byJ.set(j, { j, text: tokens[j] ?? '', start: w.start, end: w.end, deleted, muted: !!w.muted, part: w.part ?? 0, idxs: [w.idx], guessed: false });
      return;
    }
    s.start = Math.min(s.start, w.start);
    s.end = Math.max(s.end, w.end);
    s.muted &&= !!w.muted;
    s.idxs.push(w.idx);
  };
  // a script word is heard when any of its words is kept; deleted only when all are
  for (const w of kept) if (w.script_idx != null && w.script_idx < tokens.length) add(w, w.script_idx, false);
  for (const w of words) if (w.deleted && w.script_idx != null && w.script_idx < tokens.length && !byJ.has(w.script_idx)) add(w, w.script_idx, true);
  for (const w of words) {
    const s = w.deleted && w.script_idx != null ? byJ.get(w.script_idx) : undefined;
    if (s?.deleted && !s.idxs.includes(w.idx)) s.idxs.push(w.idx);
  }

  // which script words this recording covers: the matched span, widened by the unmatched
  // words heard before and after it (a misheard first or last word)
  const mapped = words.filter((w) => w.script_idx != null && w.script_idx < tokens.length);
  const loose = kept.filter((w) => w.script_idx == null);
  let lo: number;
  let hi: number;
  if (mapped.length) {
    const first = mapped[0];
    const last = mapped[mapped.length - 1];
    lo = Math.max(0, Math.min(...mapped.map((w) => w.script_idx as number)) - loose.filter((w) => w.start < first.start).length);
    hi = Math.min(tokens.length - 1, Math.max(...mapped.map((w) => w.script_idx as number)) + loose.filter((w) => w.start > last.start).length);
  } else if (kept.length) {
    lo = 0;
    hi = tokens.length - 1;
  } else return [];

  const out: ShownWord[] = [];
  let j = lo;
  while (j <= hi) {
    const got = byJ.get(j);
    if (got) {
      out.push(got);
      j++;
      continue;
    }
    // a run of script words nothing was matched to
    let k = j;
    while (k <= hi && !byJ.has(k)) k++;
    const run = Array.from({ length: k - j }, (_, i) => j + i);
    const prev = out[out.length - 1];
    const next = k <= hi ? byJ.get(k) : undefined;
    const a = prev ? prev.end : -Infinity;
    const b = next ? next.start : Infinity;
    const inGap = (w: Heard) => w.script_idx == null && w.start >= a - 1e-3 && w.end <= b + 1e-3;
    const said = kept.filter(inGap);
    const cut = words.filter((w) => w.deleted && inGap(w));
    let t0: number;
    let t1: number;
    let deleted = false;
    if (said.length) {
      t0 = said[0].start;
      t1 = Math.max(...said.map((w) => w.end));
    } else if (cut.length) {
      // the editor deleted what was heard here
      t0 = t1 = cut[0].start;
      deleted = true;
    } else if (prev && next) {
      // not heard at all: squeeze it in between its neighbours
      t0 = a;
      t1 = b;
      if (t1 - t0 < MIN_W * run.length && !prev.deleted && !next.deleted) {
        t0 = Math.min(t0, (prev.start + prev.end) / 2);
        t1 = Math.max(t1, (next.start + next.end) / 2);
        prev.end = Math.min(prev.end, t0);
        next.start = Math.max(next.start, t1);
      }
    } else {
      j = k;
      continue;
    }
    const weight = run.map((r) => (tokens[r]?.length ?? 1) + 1);
    const total = weight.reduce((x, y) => x + y, 0);
    let at = t0;
    const placed = run.map((r, i) => {
      const len = ((t1 - t0) * weight[i]) / total;
      const w: ShownWord = { j: r, text: tokens[r] ?? '', start: at, end: at + len, deleted, muted: false, part: prev?.part ?? next?.part ?? 0, idxs: [], guessed: true };
      at += len;
      return w;
    });
    // each heard word belongs to the guessed word over its middle
    for (const w of deleted ? cut : said) {
      const m = (w.start + w.end) / 2;
      const p = placed.find((x) => m < x.end) ?? placed[placed.length - 1];
      p.idxs.push(w.idx);
    }
    for (const p of placed) {
      const own = (deleted ? cut : said).filter((w) => p.idxs.includes(w.idx));
      p.muted = own.length ? own.every((w) => w.muted) : !!(prev?.muted && next?.muted);
      if (own.length) p.part = own[0].part ?? p.part;
    }
    out.push(...placed);
    j = k;
  }

  // heard words nothing in the script stands for (a repeat, an ad-lib) ride on the word
  // before them, so the highlight doesn't drop out and an edit still reaches them
  const owned = new Set(out.flatMap((w) => w.idxs));
  for (const w of words) {
    if (owned.has(w.idx) || w.script_idx != null) continue;
    const host = [...out].reverse().find((s) => s.start <= w.start && s.deleted === !!w.deleted) ?? out.find((s) => s.deleted === !!w.deleted);
    if (!host) continue;
    host.idxs.push(w.idx);
    if (!w.deleted) host.end = Math.max(host.end, w.end);
  }
  return out;
}

/** A line's script words on the scene timeline (from its layout words). */
export function lineWords(it: { tokens: string[]; words: { idx: number; start_s: number; end_s: number; script_idx: number | null; deleted: boolean; muted: boolean; part: number }[] }): ShownWord[] {
  return scriptWords(
    it.tokens,
    it.words.map((w) => ({ idx: w.idx, start: w.start_s, end: w.end_s, script_idx: w.script_idx, deleted: w.deleted, muted: w.muted, part: w.part })),
  );
}

/** A take's script words in take time (for the box editor's take lanes). */
export function takeWords(tokens: string[], words: { t0: number; t1: number; script_idx: number | null }[]): ShownWord[] {
  return scriptWords(
    tokens,
    words.map((w, i) => ({ idx: i, start: w.t0, end: w.t1, script_idx: w.script_idx })),
  );
}

/** A line as it plays, in script words: what's kept of it, or the script text when nothing plays. */
export function lineText(it: Parameters<typeof lineWords>[0] & { text: string; dropped: boolean }, part?: number): string {
  if (it.dropped || !it.words.length) return part == null ? it.text : '';
  const ws = lineWords(it).filter((w) => !w.deleted && (part == null || w.part === part));
  return ws.map((w) => w.text).join(' ') || (part == null ? it.text : '');
}
