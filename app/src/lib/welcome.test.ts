import { describe, expect, it } from 'vitest';
import { ago, back, first, itemOf, kicker, legible, metaOf, move, nameFrom, reach, rows, select, settle, showColor, SINGLE, type EpisodeCard, type Selection, type ShowCard, type WelcomeView } from './welcome';

const ep = (key: string, over: Partial<EpisodeCard> = {}): EpisodeCard => ({
  key,
  file: key,
  dir: `${key}-dir`,
  title: key,
  show: null,
  show_name: null,
  folder: null,
  number: null,
  stage: 2,
  edited: null,
  open_notes: 0,
  cover: null,
  own_cover: false,
  missing: false,
  ...over,
});

const inShow = (key: string, show: string, number: number) => ep(key, { show, show_name: show.toUpperCase(), folder: key, number });

const show = (key: string, episodes: EpisodeCard[]): ShowCard => ({ key, root: key, name: key.toUpperCase(), colors: [], cover: null, episodes, open_notes: episodes.reduce((n, e) => n + e.open_notes, 0), missing: false });

const pilot = inShow('pilot', 'tarmac', 1);
const burrito = inShow('burrito', 'tarmac', 2);
const unopened = inShow('lunch', 'tarmac', 3);
const solo = ep('solo');
const view: WelcomeView = {
  episodes: [burrito, solo, pilot],
  shows: [show('tarmac', [pilot, burrito, unopened]), show('night', [])],
  folder: '/projects',
};
const recents = [{ name: 'b', path: 'burrito' }, { name: 's', path: 'solo' }, { name: 'p', path: 'pilot' }];
const start = (): Selection => first(view, recents);
const keys = (s: Selection, row: 0 | 1) => rows(view, s.scope)[row].map((t) => t.key);

describe('rows', () => {
  it('starts each row with its New tile, and groups the single episodes last', () => {
    const [a, b] = rows(view, null);
    expect(a.map((t) => t.key)).toEqual(['new-episode', 'burrito', 'solo', 'pilot']);
    expect(b.map((t) => t.key)).toEqual(['new-show', 'tarmac', 'night', SINGLE]);
  });

  it('leaves the group out when no episode stands alone', () => {
    const none = { ...view, episodes: [burrito] };
    expect(rows(none, null)[1].map((t) => t.key)).toEqual(['new-show', 'tarmac', 'night']);
  });

  it('lists a show its episodes, those without a project too', () => {
    expect(rows(view, 'tarmac')[0].map((t) => t.key)).toEqual(['new-episode', 'pilot', 'burrito', 'lunch']);
  });

  it('lists only the single episodes for the group', () => {
    expect(rows(view, SINGLE)[0].map((t) => t.key)).toEqual(['new-episode', 'solo']);
  });
});

describe('first', () => {
  it('selects the most recent item', () => {
    expect(start()).toMatchObject({ scope: null, row: 0, key: 'burrito', item: 'burrito' });
  });

  it('selects a show, and its episodes come up with it', () => {
    const s = first(view, [{ name: 't', path: 'tarmac', show: true }]);
    expect(s).toMatchObject({ scope: 'tarmac', row: 1, key: 'tarmac', item: 'tarmac' });
  });

  it('selects nothing on an empty shelf', () => {
    const s = first({ episodes: [], shows: [], folder: '' }, []);
    expect(s).toMatchObject({ item: null, key: 'new-episode' });
  });
});

describe('select', () => {
  it('leaves the hero alone on a New tile', () => {
    const t = rows(view, null)[0][0];
    expect(select(start(), 0, t)).toMatchObject({ key: 'new-episode', item: 'burrito' });
  });

  it('keeps the first row when an episode in it is selected', () => {
    const s = select(start(), 1, rows(view, null)[1][1]);
    const e = select(s, 0, rows(view, s.scope)[0][2]);
    expect(e).toMatchObject({ scope: 'tarmac', item: 'burrito', row: 0 });
  });

  it('puts the group in the hero', () => {
    const s = select(start(), 1, rows(view, null)[1][3]);
    expect(itemOf(view, s)).toMatchObject({ kind: 'single' });
    expect(keys(s, 0)).toEqual(['new-episode', 'solo']);
  });
});

describe('move', () => {
  it('walks a row and stops at its ends', () => {
    let s = start();
    s = move(view, s, 1, 0);
    expect(s.key).toBe('solo');
    s = move(view, move(view, move(view, s, 1, 0), 1, 0), 1, 0);
    expect(s.key).toBe('pilot');
    s = move(view, move(view, move(view, move(view, s, -1, 0), -1, 0), -1, 0), -1, 0);
    expect(s).toMatchObject({ key: 'new-episode', item: 'burrito' });
  });

  it('switches rows, and a show reached by key takes over the first row', () => {
    const s = move(view, start(), 0, 1);
    expect(s).toMatchObject({ row: 1, key: 'tarmac', scope: 'tarmac' });
    const n = move(view, s, 1, 0);
    expect(n).toMatchObject({ key: 'night', scope: 'night' });
  });

  it('comes down on the show the row belongs to', () => {
    const s = move(view, move(view, start(), 0, 1), 1, 0);
    const up = move(view, select(s, 1, rows(view, null)[1][1]), 0, -1);
    expect(up).toMatchObject({ row: 0, key: 'pilot', scope: 'tarmac' });
    const far = move(view, move(view, up, 1, 0), 1, 0);
    expect(far.key).toBe('lunch');
    expect(move(view, far, 0, 1)).toMatchObject({ row: 1, key: 'tarmac' });
  });

  it('stays put at the edge of the rows', () => {
    const s = start();
    expect(move(view, s, 0, -1)).toBe(s);
  });

  it('goes up to a New tile when the show has no episodes', () => {
    const s = select(start(), 1, rows(view, null)[1][2]);
    expect(move(view, s, 0, -1)).toMatchObject({ row: 0, key: 'new-episode', item: 'night' });
  });
});

describe('back', () => {
  it('keeps an episode that is among the recent ones', () => {
    const s = move(view, select(start(), 1, rows(view, null)[1][1]), 0, -1);
    expect(back(view, s, recents)).toMatchObject({ scope: null, row: 0, key: 'pilot', item: 'pilot' });
  });

  it('goes to the newest episode from a show or an unopened episode', () => {
    const s = select(start(), 1, rows(view, null)[1][1]);
    expect(back(view, s, recents)).toMatchObject({ scope: null, key: 'burrito' });
    const lunch = reach(view, s, 'lunch');
    expect(lunch.item).toBe('lunch');
    expect(back(view, lunch, recents)).toMatchObject({ scope: null, key: 'burrito' });
  });

  it('does nothing when the first row is already everything', () => {
    const s = start();
    expect(back(view, s, recents)).toBe(s);
  });
});

describe('reach', () => {
  it('finds what was just made, in the row it belongs to', () => {
    const v = { ...view, episodes: [ep('new'), ...view.episodes] };
    const s = select(start(), 1, rows(view, null)[1][1]);
    expect(reach(v, s, 'new')).toMatchObject({ scope: null, row: 0, item: 'new' });
    expect(reach(v, s, 'lunch')).toMatchObject({ scope: 'tarmac', row: 0, item: 'lunch' });
    expect(reach(v, start(), 'night')).toMatchObject({ scope: 'night', row: 1 });
  });
});

describe('settle', () => {
  it('keeps a selection that still points at something', () => {
    const s = move(view, start(), 1, 0);
    expect(settle(view, s, recents)).toBe(s);
  });

  it('falls back to the newest when the item has gone', () => {
    const s = move(view, start(), 1, 0);
    const v = { ...view, episodes: [burrito, pilot] };
    expect(settle(v, s, recents.filter((r) => r.path !== 'solo'))).toMatchObject({ item: 'burrito' });
  });

  it('leaves a scope that has gone', () => {
    const s = select(start(), 1, rows(view, null)[1][3]);
    const v = { ...view, episodes: [burrito] };
    expect(settle(v, s, [recents[0]])).toMatchObject({ scope: null, item: 'burrito' });
  });
});

describe('words', () => {
  it('says what an episode is', () => {
    expect(kicker(burrito)).toBe('TARMAC · Ep. 2');
    expect(kicker(solo)).toBe('Single episode');
    expect(kicker(ep('x', { show_name: 'TARMAC' }))).toBe('TARMAC');
    expect(kicker(ep('x', { missing: true }))).toBe('Not found');
  });

  it('says how long ago', () => {
    const now = new Date(2026, 10, 6, 15, 0);
    expect(ago(new Date(2026, 10, 6, 14, 59, 30).toISOString(), now)).toBe('just now');
    expect(ago(new Date(2026, 10, 6, 14, 48).toISOString(), now)).toBe('12 min ago');
    expect(ago(new Date(2026, 10, 6, 11, 0).toISOString(), now)).toBe('4 h ago');
    expect(ago(new Date(2026, 10, 5, 23, 0).toISOString(), now)).toBe('yesterday');
    expect(ago(new Date(2026, 10, 3, 10, 0).toISOString(), now)).toBe('Tuesday');
    expect(ago(new Date(2026, 9, 21, 10, 0).toISOString(), now)).toBe('Oct 21');
  });

  it('says where an item stands', () => {
    const now = new Date(2026, 10, 6, 15, 0);
    const edited = new Date(2026, 10, 6, 14, 48).toISOString();
    expect(metaOf({ kind: 'episode', key: 'a', episode: ep('a', { edited }) }, now)).toBe('Arrange · edited 12 min ago');
    expect(metaOf({ kind: 'episode', key: 'a', episode: ep('a', { file: null }) }, now)).toBe('No project yet');
    expect(metaOf({ kind: 'show', key: 't', show: view.shows[0] }, now)).toBe('3 episodes');
    expect(metaOf({ kind: 'single', key: SINGLE, episodes: [solo] }, now)).toBe('1 episode');
  });

  it('takes a name from a file', () => {
    expect(nameFrom('/x/Time_Burrito-v2.fountain')).toBe('Time Burrito v2');
  });
});

describe('colour', () => {
  it('uses the theme, or stays the same for a name', () => {
    expect(showColor({ colors: ['#3f3080'], name: 'x' })).toEqual([63, 48, 128]);
    expect(showColor({ colors: [], name: 'TARMAC' })).toEqual(showColor({ colors: [], name: 'TARMAC' }));
  });

  it('lifts a deep colour on the dark hero and lowers a pale one on the light hero', () => {
    expect(Math.max(...legible([63, 48, 128], 'dark'))).toBeGreaterThan(190);
    expect(Math.max(...legible([200, 200, 210], 'light'))).toBeLessThanOrEqual(96);
  });
});
