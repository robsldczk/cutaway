// The tours: a chapter per page, shown the first time that page is reached and again from the
// help sheet. What's been seen is per viewer, so it lives in this machine's storage and not in
// the project file, which syncs. Everything here is plain: the running tour is in app.svelte.ts.
import type { Page } from './app.svelte';

export type ChapterId = 'options' | 'arrange' | 'edit' | 'export' | 'review';

export interface Stop {
  id: string;
  /** What it points at: the `data-tour` name. None: a card in the middle. */
  target?: string;
  title: string;
  text: string;
  /** What to try, said on the card. Whatever comes of it is put back when the tour ends. */
  tryIt?: string;
  /** Keys that reach the app at this stop (`e.key`, letters in lower case). */
  keys?: string[];
  /** The target takes the pointer. Only for things that can be put back. */
  live?: boolean;
}

export const CHAPTERS: Record<ChapterId, { name: string; stops: Stop[] }> = {
  options: {
    name: 'Options',
    stops: [
      { id: 'hello', title: 'Your session, cut to the script', text: 'Every line already has its takes found and one picked. This is a minute on how to change that. Anything you try on the way is put back at the end.' },
      { id: 'box', target: 'box', title: 'The line you’re on', text: 'It stays on the centre line and the script moves past it.', tryIt: 'Press ↑ and ↓ to walk the script.', keys: ['ArrowUp', 'ArrowDown'] },
      { id: 'takes', target: 'takes', title: 'Its takes', text: 'A pill for every recording of this line. The filled one is in the cut, and a star marks a favorite.', tryIt: 'Press ← and →, click a pill, or press F.', keys: ['ArrowLeft', 'ArrowRight', 'f'], live: true },
      { id: 'room', target: 'room', title: 'In the room', text: 'What was said around this take: direction, feedback and the talk between tries.' },
      { id: 'listen', target: 'transport', title: 'Listen', text: 'Space plays and stops. Return plays this line with the lines around it.', tryIt: 'Press Space or Return.', keys: [' ', 'Enter'] },
      { id: 'scenes', target: 'rail', title: 'Scenes', text: 'A button for every scene. A dot means something there wants a look.' },
      { id: 'pages', target: 'pages', title: 'Four pages, one cut', text: 'Options picks the takes, Arrange shapes the timing, Edit is the full timeline and Export hands it over. Keys 1 to 4 go between them, and each shows you round the first time.' },
      { id: 'help', target: 'help', title: 'Back any time', text: 'The ? holds every key, and starts the tour of the page you’re on.' },
    ],
  },
  arrange: {
    name: 'Arrange',
    stops: [
      { id: 'grip', target: 'grip', title: 'Reorder', text: 'Drag a line by its grip to move it. Drop it on the right half of another line and the two run together.', tryIt: 'Drag the grip.', live: true },
      { id: 'trim', target: 'trim', title: 'Trim', text: 'The handles trim the head and tail, and reach past the take into the recording. They turn amber when they cut into a word.', tryIt: 'Drag a handle, or click to set the locator.', live: true },
      { id: 'gap', target: 'gap', title: 'Pauses', text: 'Drag a gap up or down to change the pause. Below zero, the two lines overlap.', tryIt: 'Drag it, or press , and .', keys: [',', '.'], live: true },
      { id: 'sounds', target: 'sounds', title: 'Sounds', text: 'They sit beside the dialogue as balls: size is length, height is when, and left to right is how deep in the room.', tryIt: 'Press S.', keys: ['s'], live: true },
    ],
  },
  edit: {
    name: 'Edit',
    stops: [
      { id: 'lanes', target: 'timeline', title: 'The full timeline', text: 'Dialogue has a lane per character. Effects, ambience and music run below it, and the script runs along the top.' },
      { id: 'ruler', target: 'ruler', title: 'The ruler', text: 'Drag here to move the playhead without a sound.', tryIt: 'Click or drag in the ruler.', live: true },
      { id: 'clip', target: 'clip', title: 'Clips', text: 'Drag a clip to move it, and its edges to trim it. The corners are fades.', tryIt: 'Drag this clip.', live: true },
      { id: 'bucket', target: 'bucket', title: 'The sound bucket', text: 'Drag a sound onto a lane and it hangs on that moment, so it moves when the dialogue does.', tryIt: 'Press B to open and close it.', keys: ['b'], live: true },
      { id: 'ripple', target: 'ripple', title: 'Ripple', text: 'On, moving a clip moves everything after it. Lock, beside it, keeps the dialogue in script order until you turn it off.', tryIt: 'Click it.', live: true },
    ],
  },
  export: {
    name: 'Export',
    stops: [
      { id: 'format', target: 'format', title: 'What goes out', text: 'Tick the scenes, pick a format, and the cut leaves as stems, clips, a mix or a timeline.' },
      { id: 'review', target: 'for-review', title: 'Or get notes first', text: 'This opens the ticked scenes for review, so someone can write on the cut before it leaves.' },
    ],
  },
  review: {
    name: 'Review',
    stops: [
      { id: 'hello', title: 'You’re reviewing the cut', text: 'Listen, and write down what should change. Only your notes are kept, so nothing you do here can harm it.' },
      { id: 'cards', target: 'card', title: 'The script and its sound', text: 'The lines run down the page beside the wave of what’s being said.', tryIt: 'Scroll, or press ↑ and ↓.', keys: ['ArrowUp', 'ArrowDown'] },
      { id: 'listen', target: 'transport', title: 'Listen', text: 'Space plays and stops. Each pin is a note.', tryIt: 'Press Space.', keys: [' '] },
      { id: 'marker', target: 'spine', title: 'Mark a moment', text: 'Click the wave for a marker, or drag along it for a range.', tryIt: 'Click the wave, or press M. Esc drops it.', keys: ['m'], live: true },
      { id: 'note', target: 'add-note', title: 'Write on a line', text: 'The + puts a note on the whole line. On this scene, above the notes, covers everything.', tryIt: 'Press N. Notes aren’t kept during the tour.', keys: ['n'], live: true },
      { id: 'notes', target: 'notes', title: 'The notes', text: 'Reply, resolve, or drag your own note somewhere else. These filter them, and ⌘K searches.' },
      { id: 'scenes', target: 'rail', title: 'Scenes', text: 'Jump to any scene here.', tryIt: 'Press ← and →.', keys: ['ArrowLeft', 'ArrowRight'] },
      { id: 'finish', target: 'finish', title: 'When you’re done', text: 'Send review saves your notes as one file to hand over. The ? beside it brings this tour back.' },
    ],
  },
};

/** The chapter of a page. Import and Dev have none. */
export function chapterFor(page: Page, inReview: boolean): ChapterId | null {
  if (inReview) return 'review';
  return page === 'options' || page === 'arrange' || page === 'edit' || page === 'export' ? page : null;
}

// ---- keys ----

export type TourKey = 'pass' | 'next' | 'back' | 'end' | 'block';

/** What a key does during a tour. A stop's own keys go first, so arrows can be tried. */
export function tourKey(stop: Stop, e: Pick<KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>, typing: boolean): TourKey {
  if (e.key === 'Escape') return 'end';
  if (typing || e.key === 'Tab') return 'pass';
  if (e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return 'block';
  const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  if (stop.keys?.includes(k)) return 'pass';
  if (k === 'ArrowRight' || k === 'Enter') return 'next';
  if (k === 'ArrowLeft') return 'back';
  return 'block';
}

// ---- putting things back ----

/** A row of the command log, as `history` gives it: newest first. */
export interface LogRow {
  seq: number;
  client: string;
  state: string;
}

/** Where the log stood: the step an undo would take back. */
export function logTop(rows: LogRow[]): number {
  return rows.find((r) => r.state === 'done')?.seq ?? 0;
}

/** The steps made since `mark` by this app, added to those already known. */
export function ownSteps(rows: LogRow[], mark: number, known: number[], client: string): number[] {
  const made = rows.filter((r) => r.seq > mark && r.state === 'done' && r.client === client).map((r) => r.seq);
  return [...new Set([...known, ...made])];
}

/** How many undos take back the tour's steps and nothing else: it stops at the first that isn't one. */
export function undosFor(rows: LogRow[], steps: number[]): number {
  let n = 0;
  for (const r of rows) {
    if (r.state !== 'done') continue;
    if (!steps.includes(r.seq)) break;
    n++;
  }
  return n;
}

// ---- the spotlight ----

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Around the target, and inside the window: a target can be far taller than the screen. */
export function holeAround(r: Box, view: { w: number; h: number }, pad = 8, edge = 12): Box | null {
  const x0 = Math.max(edge, r.x - pad);
  const y0 = Math.max(edge, r.y - pad);
  const x1 = Math.min(view.w - edge, r.x + r.w + pad);
  const y1 = Math.min(view.h - edge, r.y + r.h + pad);
  if (x1 - x0 < 4 || y1 - y0 < 4) return null;
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

/** Of what's on screen, the one nearest the middle: on the column pages that's the current line. */
export function nearestMiddle<T extends { rect: Box }>(items: T[], view: { w: number; h: number }): T | null {
  let best: T | null = null;
  let bestD = Infinity;
  for (const it of items) {
    const { x, y, w, h } = it.rect;
    if (w < 1 || h < 1 || x + w < 0 || y + h < 0 || x > view.w || y > view.h) continue;
    const d = Math.hypot(x + w / 2 - view.w / 2, y + h / 2 - view.h / 2);
    if (d < bestD) {
      best = it;
      bestD = d;
    }
  }
  return best;
}

/** The card goes on the side with room, never over the target when it can help it. */
export function placeCard(hole: Box | null, card: { w: number; h: number }, view: { w: number; h: number }, gap = 16, edge = 12): { x: number; y: number } {
  const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(v, Math.max(lo, hi)));
  if (!hole) return { x: Math.round((view.w - card.w) / 2), y: Math.round((view.h - card.h) / 2) };
  const midX = clamp(hole.x + hole.w / 2 - card.w / 2, edge, view.w - card.w - edge);
  const midY = clamp(hole.y + hole.h / 2 - card.h / 2, edge, view.h - card.h - edge);
  const sides = [
    { room: view.w - (hole.x + hole.w) - gap - edge, need: card.w, x: hole.x + hole.w + gap, y: midY },
    { room: hole.x - gap - edge, need: card.w, x: hole.x - gap - card.w, y: midY },
    { room: view.h - (hole.y + hole.h) - gap - edge, need: card.h, x: midX, y: hole.y + hole.h + gap },
    { room: hole.y - gap - edge, need: card.h, x: midX, y: hole.y - gap - card.h },
  ];
  const fits = sides.find((s) => s.room >= s.need);
  if (fits) return { x: Math.round(fits.x), y: Math.round(fits.y) };
  // nothing fits: a corner of the window, over as little of the target as there is
  return { x: view.w - card.w - edge, y: view.h - card.h - edge };
}

// ---- storage ----

const SEEN = 'cutaway.tour';
const OWED = 'cutaway.tourOwed';

/** What a running tour has to put back, kept so a quit in the middle doesn't leave it in the cut. */
export interface Owed {
  path: string;
  steps: number[];
}

function read(key: string): unknown {
  try {
    return JSON.parse(localStorage.getItem(key) ?? 'null');
  } catch {
    return null;
  }
}

function write(key: string, value: unknown) {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* storage blocked: it lasts for this session */
  }
}

export function readSeen(): ChapterId[] {
  const v = read(SEEN);
  return Array.isArray(v) ? v.filter((c): c is ChapterId => typeof c === 'string' && c in CHAPTERS) : [];
}

export function writeSeen(seen: ChapterId[]) {
  write(SEEN, seen);
}

export function readOwed(): Owed | null {
  const v = read(OWED) as Partial<Owed> | null;
  if (!v || typeof v.path !== 'string' || !Array.isArray(v.steps)) return null;
  return { path: v.path, steps: v.steps.filter((s) => typeof s === 'number') };
}

export function writeOwed(owed: Owed | null) {
  write(OWED, owed);
}
