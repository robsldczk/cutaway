// Dev panel types (views from `dev_sessions` / `dev_session`).

export type TakeOrigin = 'matcher' | 'rescue' | 'manual';
export type MarkKind = 'missed_take' | 'wrong_line' | 'not_a_take' | 'edges' | 'other';
export type MarkStatus = 'open' | 'proposed' | 'applied' | 'discarded';

export interface DevSessionItem {
  source_id: string;
  name: string;
  character: string | null;
  takes: { matcher: number; rescue: number; manual: number };
  marks: number;
}

export interface DevRowStatus {
  kind: 'take' | 'near_miss' | 'unmatched' | 'bleed' | 'director';
  line_id?: string | null;
  take_id?: string | null;
  take_no?: number | null;
  origin?: TakeOrigin | null;
  read_as?: string | null;
  score?: number | null;
  class?: string | null;
}

export interface DevRow {
  id: string;
  source_id: string;
  mic: 'session' | 'director';
  speaker: string;
  text: string;
  /** Source-file seconds of that mic. */
  t0: number;
  t1: number;
  /** Session seconds (ordering). */
  st: number;
  level_db: number;
  status: DevRowStatus;
  marks: string[];
}

export interface DevLineTake {
  id: string;
  take_no: number;
  origin: TakeOrigin;
  t0: number;
  t1: number;
  heard: string;
  read_as: string | null;
}

export interface DevLine {
  id: string;
  label: string;
  character: string | null;
  text: string;
  own: boolean;
  takes: DevLineTake[];
}

export interface DevScene {
  id: string;
  number: number;
  heading: string;
  lines: DevLine[];
}

export interface DevProposalTake {
  id: string;
  line_id: string;
  t0: number;
  t1: number;
  heard: string;
  read_as: string | null;
  origin: TakeOrigin;
}

export interface DevMark {
  id: string;
  source_id: string;
  t0: number;
  t1: number;
  kind: MarkKind;
  line_id: string | null;
  note: string;
  text: string;
  status: MarkStatus;
  proposal: { add: DevProposalTake[]; remove: string[]; summary: string[] } | null;
  created_at: string;
}

export interface DevSession {
  source_id: string;
  character: string;
  name: string;
  director: string[];
  rows: DevRow[];
  scenes: DevScene[];
  marks: DevMark[];
}

export const MARK_KINDS: { id: MarkKind; label: string }[] = [
  { id: 'missed_take', label: 'Missed take' },
  { id: 'wrong_line', label: 'Wrong line' },
  { id: 'not_a_take', label: 'Not a take' },
  { id: 'edges', label: 'Edges wrong' },
  { id: 'other', label: 'Other' },
];

export function kindLabel(k: MarkKind): string {
  return MARK_KINDS.find((m) => m.id === k)?.label ?? k;
}

/** Text split around case-insensitive matches of `q`, for highlighting without HTML. */
export function splitMatches(text: string, q: string): { t: string; hit: boolean }[] {
  if (!q) return [{ t: text, hit: false }];
  const out: { t: string; hit: boolean }[] = [];
  const lower = text.toLowerCase();
  const needle = q.toLowerCase();
  let i = 0;
  for (;;) {
    const j = lower.indexOf(needle, i);
    if (j < 0) break;
    if (j > i) out.push({ t: text.slice(i, j), hit: false });
    out.push({ t: text.slice(j, j + needle.length), hit: true });
    i = j + needle.length;
  }
  if (i < text.length) out.push({ t: text.slice(i), hit: false });
  return out;
}
