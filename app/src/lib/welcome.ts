// The welcome screen's logic, without the screen: what's in the two rows, what's selected, and
// where the keys go from there. Selecting never opens anything (D144).
import { colorOf, type Rgb } from '../theme';
import type { RecentEpisode } from './app.svelte';

export const STAGES = ['Import', 'Options', 'Arrange', 'Edit', 'Exported'];
export const IMAGE_EXTENSIONS = ['jpg', 'jpeg', 'png', 'webp'];
/** The group of episodes without a show. */
export const SINGLE = 'single';
const NEW_EPISODE = 'new-episode';
const NEW_SHOW = 'new-show';
/** Soft grey: single episodes have no colour of their own. */
const NEUTRAL: Rgb = [200, 200, 210];

export interface CoverRef {
  path: string;
  stamp: string;
}

export interface EpisodeCard {
  key: string;
  file: string | null;
  dir: string;
  title: string;
  show: string | null;
  show_name: string | null;
  folder: string | null;
  number: number | null;
  stage: number;
  edited: string | null;
  open_notes: number;
  cover: CoverRef | null;
  own_cover: boolean;
  missing: boolean;
}

export interface ShowCard {
  key: string;
  root: string;
  name: string;
  colors: string[];
  cover: CoverRef | null;
  episodes: EpisodeCard[];
  open_notes: number;
  missing: boolean;
}

export interface WelcomeView {
  episodes: EpisodeCard[];
  shows: ShowCard[];
  folder: string;
}

export type Tile =
  | { kind: 'new-episode'; key: string }
  | { kind: 'new-show'; key: string }
  | { kind: 'episode'; key: string; episode: EpisodeCard }
  | { kind: 'show'; key: string; show: ShowCard }
  | { kind: 'single'; key: string; episodes: EpisodeCard[] };

/** What the hero shows. */
export type Item = Exclude<Tile, { kind: 'new-episode' | 'new-show' }>;

export interface Selection {
  /** The first row's content: everything recent (null), a show's key, or `SINGLE`. */
  scope: string | null;
  row: 0 | 1;
  /** The tile the keys are on. */
  key: string;
  /** The item in the hero. It stays when the keys are on a New tile. */
  item: string | null;
}

const singles = (v: WelcomeView) => v.episodes.filter((e) => !e.show);

/** The two rows, for what the first one is showing. */
export function rows(v: WelcomeView, scope: string | null): [Tile[], Tile[]] {
  const show = v.shows.find((s) => s.key === scope);
  const eps = scope === SINGLE ? singles(v) : show ? show.episodes : v.episodes;
  const first: Tile[] = [{ kind: 'new-episode', key: NEW_EPISODE }, ...eps.map((e): Tile => ({ kind: 'episode', key: e.key, episode: e }))];
  const second: Tile[] = [{ kind: 'new-show', key: NEW_SHOW }, ...v.shows.map((s): Tile => ({ kind: 'show', key: s.key, show: s }))];
  const alone = singles(v);
  if (alone.length) second.push({ kind: 'single', key: SINGLE, episodes: alone });
  return [first, second];
}

/** An episode by its key, wherever it's listed. */
export function episodeOf(v: WelcomeView, key: string): EpisodeCard | undefined {
  return v.episodes.find((e) => e.key === key) ?? v.shows.flatMap((s) => s.episodes).find((e) => e.key === key);
}

/** What the selection puts in the hero. */
export function itemOf(v: WelcomeView, s: Selection): Item | null {
  if (!s.item) return null;
  if (s.item === SINGLE) return { kind: 'single', key: SINGLE, episodes: singles(v) };
  const show = v.shows.find((x) => x.key === s.item);
  if (show) return { kind: 'show', key: show.key, show };
  const episode = episodeOf(v, s.item);
  return episode ? { kind: 'episode', key: episode.key, episode } : null;
}

/** Selects a tile. A show or the single episodes also take over the first row. */
export function select(s: Selection, row: 0 | 1, t: Tile): Selection {
  if (t.kind === 'new-episode' || t.kind === 'new-show') return { ...s, row, key: t.key };
  const scope = t.kind === 'episode' ? s.scope : t.key;
  return { scope, row, key: t.key, item: t.key };
}

/** The most recent item, which is what's selected on launch. */
export function first(v: WelcomeView, recents: RecentEpisode[]): Selection {
  const none: Selection = { scope: null, row: 0, key: NEW_EPISODE, item: null };
  const newest = recents[0];
  if (!newest) return none;
  const start = { ...none };
  const [top, bottom] = rows(v, null);
  const show = newest.show ? bottom.find((t) => t.key === newest.path) : undefined;
  if (show) return select(start, 1, show);
  const ep = top.find((t) => t.key === newest.path) ?? top[1];
  if (ep) return select(start, 0, ep);
  return bottom[1] ? select(start, 1, bottom[1]) : none;
}

/** Left and right within a row, up and down between the rows. */
export function move(v: WelcomeView, s: Selection, dx: number, dy: number): Selection {
  const both = rows(v, s.scope);
  if (dy) {
    const row: 0 | 1 = dy > 0 ? 1 : 0;
    if (row === s.row) return s;
    const to = both[row];
    const col = both[s.row].findIndex((t) => t.key === s.key);
    // down from a show's episodes lands on that show, so the row stays what it was
    const home = row === 1 ? to.find((t) => t.key === s.scope) : undefined;
    return select(s, row, home ?? to[Math.min(Math.max(col, 1), to.length - 1)]);
  }
  const row = both[s.row];
  const col = row.findIndex((t) => t.key === s.key);
  const t = row[Math.max(0, Math.min(row.length - 1, col + dx))];
  return t.key === s.key ? s : select(s, s.row, t);
}

/** Back to recent episodes across everything, keeping the item when it's among them. */
export function back(v: WelcomeView, s: Selection, recents: RecentEpisode[]): Selection {
  if (!s.scope) return s;
  const top = rows(v, null)[0];
  const here = top.find((t) => t.key === s.item);
  if (here) return { scope: null, row: 0, key: here.key, item: here.key };
  const fresh = first(v, recents.filter((r) => !r.show));
  return fresh.item ? fresh : { ...fresh, item: s.item, key: top[0].key };
}

/** Selects what was just made, or the same thing after a reload. */
export function reach(v: WelcomeView, s: Selection, key: string): Selection {
  for (const scope of [s.scope, null]) {
    const both = rows(v, scope);
    for (const row of [0, 1] as const) {
      const t = both[row].find((x) => x.key === key);
      if (t) return select({ ...s, scope }, row, t);
    }
  }
  return s;
}

/** After a reload: the selection, when what it pointed at is still there. */
export function settle(v: WelcomeView, s: Selection, recents: RecentEpisode[]): Selection {
  const scoped = s.scope === null || s.scope === SINGLE ? s.scope !== SINGLE || singles(v).length > 0 : v.shows.some((x) => x.key === s.scope);
  const held = scoped ? s : { ...s, scope: null };
  const on = rows(v, held.scope)[held.row].some((t) => t.key === held.key);
  return on && (!held.item || itemOf(v, held)) ? held : first(v, recents);
}

export function episodesText(n: number) {
  return `${n} ${n === 1 ? 'episode' : 'episodes'}`;
}

export function notesText(n: number) {
  return `${n} ${n === 1 ? 'note' : 'notes'}`;
}

/** The small line above an episode's title. */
export function kicker(e: EpisodeCard): string {
  if (e.missing) return 'Not found';
  if (!e.show_name) return 'Single episode';
  return e.number == null ? e.show_name : `${e.show_name} · Ep. ${e.number}`;
}

/** How long ago, in a few words. */
export function ago(iso: string, now = new Date()): string {
  const t = new Date(iso);
  const min = Math.floor((now.getTime() - t.getTime()) / 60000);
  if (min < 1) return 'just now';
  if (min < 60) return `${min} min ago`;
  const day = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((day(now) - day(t)) / 86400000);
  if (days === 0) return `${Math.floor(min / 60)} h ago`;
  if (days === 1) return 'yesterday';
  if (days < 7) return t.toLocaleDateString('en-US', { weekday: 'long' });
  return t.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
}

/** The line under the hero's title. */
export function metaOf(item: Item, now = new Date()): string {
  if (item.kind === 'show') return item.show.missing ? `Not at ${item.show.root}` : episodesText(item.show.episodes.length);
  if (item.kind === 'single') return episodesText(item.episodes.length);
  const e = item.episode;
  if (e.missing) return `Not at ${e.key}`;
  if (!e.file) return 'No project yet';
  const stage = STAGES[e.stage] ?? STAGES[0];
  return e.edited ? `${stage} · edited ${ago(e.edited, now)}` : stage;
}

export function notesOf(item: Item): number {
  if (item.kind === 'show') return item.show.open_notes;
  if (item.kind === 'episode') return item.episode.open_notes;
  return item.episodes.reduce((n, e) => n + e.open_notes, 0);
}

function hash(s: string) {
  let h = 0;
  for (const c of s) h = (h * 31 + c.charCodeAt(0)) | 0;
  return Math.abs(h);
}

function fromHex(h: string): Rgb {
  const n = parseInt(h.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** A show's colour: its theme's, or one that stays the same for its name. */
export function showColor(s: Pick<ShowCard, 'colors' | 'name'>): Rgb {
  return s.colors.length ? fromHex(s.colors[0]) : colorOf(hash(s.name)).rgb;
}

/** The colour an item's wash is mixed from. */
export function itemColor(v: WelcomeView, item: Item): Rgb {
  if (item.kind === 'show') return showColor(item.show);
  if (item.kind === 'single') return NEUTRAL;
  const show = v.shows.find((s) => s.key === item.episode.show);
  return show ? showColor(show) : NEUTRAL;
}

/** A colour that reads on the hero: theme colours can be deep, and the grey of single episodes
 *  is pale. Lifted on the dark veil, lowered on the light one. */
export function legible(c: Rgb, theme: 'dark' | 'light'): Rgb {
  const top = Math.max(...c, 1);
  if (theme === 'light') return c.map((x) => Math.round(x * Math.min(1, 96 / top))) as Rgb;
  const k = Math.max(1, 205 / top);
  return c.map((x) => Math.min(255, Math.round(x * k + (top < 120 ? 40 : 0)))) as Rgb;
}

/** What stands in for a cover: a soft wash of the show's colour. */
export function wash([r, g, b]: Rgb): string {
  return `radial-gradient(ellipse 80% 70% at 30% 20%, rgb(${r} ${g} ${b} / 0.32), transparent 70%), linear-gradient(160deg, rgb(${r} ${g} ${b} / 0.16), rgb(var(--veil-rgb) / 0.03))`;
}

/** A name for a new episode from the file it started with. */
export function nameFrom(path: string): string {
  const base = path.split(/[/\\]/).pop() ?? path;
  return base.replace(/\.[^.]+$/, '').replace(/[_-]+/g, ' ').trim();
}
