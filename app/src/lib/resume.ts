// Where Rob left off. Per viewer, so it lives in this machine's storage and not in the project
// file, which syncs to other machines and would count every move of the playhead as a change.
import type { ImportTab, Page } from './app.svelte';

/** A place in one episode. */
export interface Spot {
  page: Page;
  tab: ImportTab;
  scene: string;
  line: string | null;
  /** The playhead, in seconds of the scene. */
  at: number;
  /** The Edit page's scale in px per second; 0 fits the scene. */
  zoom: number;
}

/** What was open at the end: an episode, or a show's home. */
export interface LastOpen {
  name: string;
  path: string;
  show?: boolean;
}

const SPOTS = 'cutaway.spots';
const LAST = 'cutaway.last';
/** Episodes remembered, newest first. */
export const KEPT = 30;
/** How often a moving playhead is written. A quit gives the page no last word, so this is what's at stake. */
export const EVERY_MS = 500;

const PAGES: Page[] = ['import', 'options', 'arrange', 'edit', 'export', 'dev'];
const TABS: ImportTab[] = ['files', 'sounds', 'rooms'];

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
    /* private mode: the spot lasts for this session */
  }
}

/** A stored spot, or null when it can't be trusted: storage outlives app versions. */
function valid(v: unknown): Spot | null {
  const s = v as Partial<Spot> | null;
  if (!s || typeof s.scene !== 'string' || !s.scene) return null;
  if (!PAGES.includes(s.page as Page)) return null;
  const at = Number(s.at);
  const zoom = Number(s.zoom);
  return {
    page: s.page as Page,
    tab: TABS.includes(s.tab as ImportTab) ? (s.tab as ImportTab) : 'files',
    scene: s.scene,
    line: typeof s.line === 'string' ? s.line : null,
    at: Number.isFinite(at) && at > 0 ? at : 0,
    zoom: Number.isFinite(zoom) && zoom > 0 ? zoom : 0,
  };
}

function all(): [string, unknown][] {
  const v = read(SPOTS);
  return Array.isArray(v) ? v.filter((e) => Array.isArray(e) && typeof e[0] === 'string') : [];
}

export function spotOf(path: string): Spot | null {
  return valid(all().find(([p]) => p === path)?.[1]);
}

export function keepSpot(path: string, spot: Spot) {
  write(SPOTS, [[path, spot], ...all().filter(([p]) => p !== path)].slice(0, KEPT));
}

/** The next spot to write. Only the newest counts, and at most one write per `EVERY_MS`. */
let waiting: { path: string; spot: Spot } | null = null;
let timer: ReturnType<typeof setTimeout> | null = null;
let queued = '';

export function queueSpot(path: string, spot: Spot) {
  const key = JSON.stringify([path, spot]);
  if (key === queued) return;
  queued = key;
  waiting = { path, spot };
  timer ??= setTimeout(flushSpot, EVERY_MS);
}

export function flushSpot() {
  if (timer) clearTimeout(timer);
  timer = null;
  if (waiting) keepSpot(waiting.path, waiting.spot);
  waiting = null;
}

export function lastOpen(): LastOpen | null {
  const v = read(LAST) as Partial<LastOpen> | null;
  return v && typeof v.path === 'string' && v.path ? { name: String(v.name ?? v.path), path: v.path, show: !!v.show } : null;
}

export function keepLast(last: LastOpen | null) {
  write(LAST, last);
}

/** Where a spot lands in the scene as it is now: edits since then can have moved its end or its line. */
export function landing(spot: Spot, duration: number, lines: { line_id: string }[]) {
  return {
    at: Math.min(spot.at, Math.max(0, duration)),
    line: lines.some((l) => l.line_id === spot.line) ? spot.line : null,
  };
}
