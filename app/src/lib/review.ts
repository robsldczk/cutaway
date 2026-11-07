// Review mode's shapes and the arithmetic of its screen. The notes themselves are folded in the
// core: a page only arranges what it's given.
import type { ItemView } from './types';

export type NoteTag = 'performance' | 'timing' | 'sound' | 'music' | 'script';
export type NoteStatus = 'open' | 'resolved' | 'reopened';
export type ReviewRole = 'director' | 'producer' | 'writer' | 'actor' | 'client' | 'editor' | 'other';

export const TAGS: { id: NoteTag; label: string }[] = [
  { id: 'performance', label: 'Performance' },
  { id: 'timing', label: 'Timing' },
  { id: 'sound', label: 'Sound' },
  { id: 'music', label: 'Music' },
  { id: 'script', label: 'Script' },
];

/** What a reviewer can say they are. The editor's role comes with the editing pages. */
export const ROLES: { id: ReviewRole; label: string }[] = [
  { id: 'director', label: 'Director' },
  { id: 'producer', label: 'Producer' },
  { id: 'writer', label: 'Writer' },
  { id: 'actor', label: 'Actor' },
  { id: 'client', label: 'Client' },
  { id: 'other', label: 'Other' },
];

export const EDITOR_ROLE: { id: ReviewRole; label: string } = { id: 'editor', label: 'Editor' };

export const tagLabel = (t: NoteTag | null | undefined) => TAGS.find((x) => x.id === t)?.label ?? '';
export const roleLabel = (r: ReviewRole) => [...ROLES, EDITOR_ROLE].find((x) => x.id === r)?.label ?? 'Other';

export interface Reviewer {
  name: string;
  role: ReviewRole;
  initials: string;
}

export interface ReviewerView {
  current: Reviewer | null;
  last: Reviewer | null;
  remembered: boolean;
}

export interface Stamp {
  by: Reviewer;
  at: string;
}

export interface NoteReply {
  id: string;
  author: Reviewer;
  text: string;
  at: string;
}

export interface Note {
  id: string;
  n: number;
  round: string | null;
  round_number: number | null;
  scene_id: string;
  scene_number: number | null;
  author: Reviewer;
  mine: boolean;
  /** Whoever's here may move and remove it: it's theirs, or they're the editor. */
  can_change: boolean;
  tag: NoteTag | null;
  text: string;
  status: NoteStatus;
  on: 'word' | 'words' | 'line' | 'pause' | 'sound' | 'scene';
  quote: string;
  speaker: string;
  line_id: string | null;
  placement_id: string | null;
  start_s: number | null;
  end_s: number | null;
  time: string;
  detached: boolean;
  written_s: number;
  changed: boolean;
  created_at: string;
  resolved: Stamp | null;
  reopened: Stamp | null;
  replies: NoteReply[];
  thread: string;
}

/** A scene's open note, as the editing pages mark it. */
export interface NoteMark {
  id: string;
  n: number;
  line_id: string | null;
  placement_id: string | null;
  start_s: number;
  end_s: number | null;
  tag: NoteTag | null;
  text: string;
  who: string;
  changed: boolean;
}

export interface RoundView {
  id: string;
  number: number;
  created_at: string;
  created_by: Reviewer;
  status: 'open' | 'closed';
  cut: string | null;
  scenes: { id: string; number: number; place: string; version: number | null; changed: boolean }[];
  notes: number;
  open: number;
}

export interface ReviewView {
  reviewer: ReviewerView;
  project: string;
  episode: string;
  package: boolean;
  rounds: RoundView[];
  current_round: string | null;
  round: string | null;
  scenes: { id: string; in_round: boolean; notes: number; open: number }[];
  notes: Note[];
}

export interface Tally {
  open: number;
  resolved: number;
  reopened: number;
}

export interface HistoryRound extends RoundView {
  summary: { all: Tally; scenes: Record<string, Tally>; people: Record<string, Tally> };
  reviewers: string[];
}

export interface Arrival {
  id: string;
  at: string;
  who: string;
  via: 'folder' | 'file';
  notes: number;
  replies: number;
  changes: number;
}

export interface ReviewHistory {
  rounds: HistoryRound[];
  arrivals: Arrival[];
  detached: number;
  folder: string;
  troubles: { file: string; text: string }[];
}

export interface SearchResults {
  scenes: { id: string; number: number; place: string; title: string; text: string; open: number }[];
  lines: { id: string; scene_id: string; scene_number: number; who: string; text: string }[];
  notes: Note[];
}

/** What the note being written hangs on. */
export interface NoteTarget {
  on: Note['on'];
  line_id: string | null;
  placement_id: string | null;
  quote: string;
  speaker: string;
  start_s: number | null;
  end_s: number | null;
  time: string | null;
}

export type NoteFilter = 'all' | 'open' | 'mine';

export function filtered(notes: Note[], by: NoteFilter, kind: NoteTag | null, round: string | null = null) {
  return notes.filter((n) => {
    if (round && n.round !== round) return false;
    if (by === 'open' && n.status === 'resolved') return false;
    if (by === 'mine' && !n.mine) return false;
    return !kind || n.tag === kind;
  });
}

/** Notes that speak of one thing stay together, in the order they sit on the cut. */
export function threadsOf(notes: Note[]): Note[][] {
  const by = new Map<string, Note[]>();
  for (const n of notes) by.set(n.thread, [...(by.get(n.thread) ?? []), n]);
  return [...by.values()];
}

/** `6 Nov` */
export const day = (iso: string) => new Date(iso).toLocaleDateString('en-GB', { day: 'numeric', month: 'short' });

/** `Tue 14:02` */
export const when = (iso: string) => {
  const d = new Date(iso);
  return `${d.toLocaleDateString('en-GB', { weekday: 'short' })} ${d.toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' })}`;
};

const count = (n: number, one: string) => `${n} ${one}${n === 1 ? '' : 's'}`;

/** `Albert, 12 new notes, 3 replies, via shared folder, Tue 14:02` */
export function arrivalLine(a: Arrival) {
  const what = [a.notes ? `${a.notes} new ${a.notes === 1 ? 'note' : 'notes'}` : '', a.replies ? count(a.replies, 'reply').replace('replys', 'replies') : '', a.changes ? count(a.changes, 'change') : ''].filter(Boolean);
  return [a.who, ...what, a.via === 'folder' ? 'via shared folder' : 'added from file', when(a.at)].join(', ');
}

/** `6 Nov · Sc. 2, 3, 5 · v004 · 7 notes` */
export function roundLine(r: RoundView) {
  const scenes = r.scenes.length ? `Sc. ${r.scenes.map((s) => s.number).join(', ')}` : 'no scenes left';
  return [day(r.created_at), scenes, r.cut, count(r.notes, 'note')].filter(Boolean).join(' · ');
}

/** The quote as a note shows it. */
export function quoted(n: Pick<Note, 'on' | 'quote'>) {
  if (!n.quote) return '';
  return n.on === 'pause' || n.on === 'sound' ? n.quote : `“${n.quote}”`;
}

// ---- the column ----

export interface ReviewRow {
  kind: 'line' | 'gap' | 'act';
  key: string;
  it?: ItemView;
  label?: string;
  start_s: number;
  end_s: number;
  top: number;
  height: number;
}

const ROW_GAP = 4;
export const CARD_H = 58;
const ACT_H = 30;

/** The column as the mockup lays it out: a card per line and a pause that's as tall as it's long.
 *  Heights are worked out here, not measured, so the spine and the notes have them at once. */
export function reviewRows(items: ItemView[], scale = 1): { rows: ReviewRow[]; height: number } {
  const rows: ReviewRow[] = [];
  let y = 0;
  let prev = 0;
  const card = Math.round(CARD_H * scale);
  for (const it of items) {
    if (it.kind === 'spacer') {
      const h = Math.round(ACT_H * scale);
      rows.push({ kind: 'act', key: it.line_id, it, start_s: it.start_s, end_s: it.end_s, top: y, height: h });
      y += h + ROW_GAP;
      prev = Math.max(prev, it.end_s);
      continue;
    }
    const g = it.start_s - prev;
    if (g > 0.01) {
      const h = Math.round(Math.min(80, 10 + g * 6) * scale);
      rows.push({ kind: 'gap', key: `g${it.line_id}`, label: g > 1.2 ? `‖ ${g.toFixed(1)} s` : '', start_s: prev, end_s: it.start_s, top: y, height: h });
      y += h + ROW_GAP;
    }
    rows.push({ kind: 'line', key: it.line_id, it, start_s: it.start_s, end_s: it.end_s, top: y, height: card });
    y += card + ROW_GAP;
    prev = Math.max(prev, it.end_s);
  }
  return { rows, height: y };
}

export interface Placed<T> {
  item: T;
  top: number;
}

/** Cards hang beside their spot, and push down the ones below rather than cover them. */
export function placeCards<T>(cards: { item: T; y: number; height: number }[], gap = 10, from = -Infinity): Placed<T>[] {
  const out: Placed<T>[] = [];
  let bottom = from;
  for (const c of [...cards].sort((a, b) => a.y - b.y)) {
    const top = Math.max(c.y - 20, bottom + gap);
    out.push({ item: c.item, top });
    bottom = top + c.height;
  }
  return out;
}
