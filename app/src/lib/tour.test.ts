import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { CHAPTERS, chapterFor, holeAround, logTop, nearestMiddle, ownSteps, placeCard, readOwed, readSeen, tourKey, undosFor, writeOwed, writeSeen, type LogRow, type Stop } from './tour';

const key = (k: string, mods: Partial<KeyboardEvent> = {}) => ({ key: k, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...mods });
const row = (seq: number, state = 'done', client = 'ui'): LogRow => ({ seq, state, client });

describe('chapters', () => {
  it('has one for each page that can be toured, and review wins over the page', () => {
    expect(chapterFor('arrange', false)).toBe('arrange');
    expect(chapterFor('import', false)).toBeNull();
    expect(chapterFor('dev', false)).toBeNull();
    expect(chapterFor('options', true)).toBe('review');
  });

  it('never offers a key that writes what a tour couldn’t put back', () => {
    const stops = Object.values(CHAPTERS).flatMap((c) => c.stops);
    const allowed = new Set(['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', ' ', 'Enter', 'f', 's', 'b', 'm', 'n', ',', '.']);
    for (const s of stops) for (const k of s.keys ?? []) expect(allowed.has(k), `${s.id}: ${k}`).toBe(true);
  });

  it('says what to try wherever something can be tried', () => {
    for (const s of Object.values(CHAPTERS).flatMap((c) => c.stops)) if (s.live || s.keys) expect(s.tryIt, s.id).toBeTruthy();
  });
});

describe('keys', () => {
  const look: Stop = { id: 'a', title: '', text: '' };
  const tryArrows: Stop = { ...look, keys: ['ArrowLeft', 'ArrowRight', 'f'] };

  it('walks the stops', () => {
    expect(tourKey(look, key('ArrowRight'), false)).toBe('next');
    expect(tourKey(look, key('Enter'), false)).toBe('next');
    expect(tourKey(look, key('ArrowLeft'), false)).toBe('back');
    expect(tourKey(look, key('Escape'), false)).toBe('end');
  });

  it('gives a stop its own keys first, in either case', () => {
    expect(tourKey(tryArrows, key('ArrowRight'), false)).toBe('pass');
    expect(tourKey(tryArrows, key('f'), false)).toBe('pass');
    expect(tourKey(tryArrows, key('F'), false)).toBe('pass');
    expect(tourKey(tryArrows, key('Enter'), false)).toBe('next');
  });

  it('stops everything else, undo above all', () => {
    expect(tourKey(tryArrows, key('z', { metaKey: true }), false)).toBe('block');
    expect(tourKey(tryArrows, key('f', { metaKey: true }), false)).toBe('block');
    expect(tourKey(look, key('f'), false)).toBe('block');
    expect(tourKey(look, key('1'), false)).toBe('block');
    expect(tourKey(look, key('?', { shiftKey: true }), false)).toBe('block');
  });

  it('leaves typing alone, but Esc still works', () => {
    expect(tourKey(look, key('a'), true)).toBe('pass');
    expect(tourKey(look, key('Enter'), true)).toBe('pass');
    expect(tourKey(look, key('Escape'), true)).toBe('end');
  });
});

describe('putting things back', () => {
  it('finds where the log stands, past anything undone', () => {
    expect(logTop([row(9, 'undone'), row(8, 'undone'), row(7), row(6)])).toBe(7);
    expect(logTop([])).toBe(0);
  });

  it('keeps only this app’s steps since the mark', () => {
    const rows = [row(12), row(11, 'done', 'mcp'), row(10), row(9), row(8, 'undone')];
    expect(ownSteps(rows, 9, [], 'ui')).toEqual([12, 10]);
    expect(ownSteps(rows, 9, [10], 'ui')).toEqual([10, 12]);
  });

  it('takes back the tour’s steps, newest first', () => {
    expect(undosFor([row(12), row(11), row(10)], [11, 12])).toBe(2);
    expect(undosFor([row(12, 'undone'), row(11), row(10)], [11])).toBe(1);
  });

  it('stops at anything else, so someone’s later work is never undone', () => {
    expect(undosFor([row(13, 'done', 'mcp'), row(12), row(11)], [11, 12])).toBe(0);
    expect(undosFor([row(13), row(12, 'done', 'mcp'), row(11)], [11, 13])).toBe(1);
  });

  it('has nothing to take back when nothing was tried', () => {
    expect(undosFor([row(3), row(2)], [])).toBe(0);
  });
});

describe('the spotlight', () => {
  const view = { w: 1000, h: 800 };

  it('stays inside the window', () => {
    expect(holeAround({ x: 100, y: -500, w: 40, h: 3000 }, view)).toEqual({ x: 92, y: 12, w: 56, h: 776 });
  });

  it('has nothing to light when the target is off screen', () => {
    expect(holeAround({ x: 2000, y: 100, w: 40, h: 40 }, view)).toBeNull();
  });

  it('picks what’s nearest the middle, and ignores what’s hidden', () => {
    const items = [
      { id: 'hidden', rect: { x: 500, y: 400, w: 0, h: 0 } },
      { id: 'far', rect: { x: 0, y: 0, w: 50, h: 50 } },
      { id: 'near', rect: { x: 450, y: 350, w: 50, h: 50 } },
      { id: 'gone', rect: { x: 480, y: 2000, w: 50, h: 50 } },
    ];
    expect(nearestMiddle(items, view)?.id).toBe('near');
    expect(nearestMiddle([items[0]], view)).toBeNull();
  });

  it('puts the card in the middle with nothing to point at', () => {
    expect(placeCard(null, { w: 300, h: 200 }, view)).toEqual({ x: 350, y: 300 });
  });

  it('puts the card beside the target, on a side with room', () => {
    expect(placeCard({ x: 20, y: 300, w: 100, h: 100 }, { w: 300, h: 200 }, view).x).toBe(136);
    expect(placeCard({ x: 700, y: 300, w: 290, h: 100 }, { w: 300, h: 200 }, view).x).toBe(384);
  });

  it('goes below or above a target as wide as the window', () => {
    expect(placeCard({ x: 12, y: 10, w: 976, h: 60 }, { w: 300, h: 200 }, view).y).toBe(86);
    expect(placeCard({ x: 12, y: 700, w: 976, h: 60 }, { w: 300, h: 200 }, view).y).toBe(484);
  });

  it('stays in the window when nothing fits', () => {
    const p = placeCard({ x: 12, y: 12, w: 976, h: 776 }, { w: 300, h: 200 }, view);
    expect(p).toEqual({ x: 688, y: 588 });
  });
});

describe('storage', () => {
  const held = new Map<string, string>();
  beforeEach(() => {
    held.clear();
    vi.stubGlobal('localStorage', {
      getItem: (k: string) => held.get(k) ?? null,
      setItem: (k: string, v: string) => void held.set(k, v),
      removeItem: (k: string) => void held.delete(k),
    });
  });
  afterEach(() => vi.unstubAllGlobals());

  it('remembers what’s been seen, and drops what it doesn’t know', () => {
    expect(readSeen()).toEqual([]);
    writeSeen(['options', 'review']);
    expect(readSeen()).toEqual(['options', 'review']);
    held.set('cutaway.tour', JSON.stringify(['edit', 'old', 3]));
    expect(readSeen()).toEqual(['edit']);
    held.set('cutaway.tour', 'not json');
    expect(readSeen()).toEqual([]);
  });

  it('keeps what’s owed until it’s paid', () => {
    writeOwed({ path: '/a', steps: [4, 5] });
    expect(readOwed()).toEqual({ path: '/a', steps: [4, 5] });
    writeOwed(null);
    expect(readOwed()).toBeNull();
    held.set('cutaway.tourOwed', JSON.stringify({ steps: [1] }));
    expect(readOwed()).toBeNull();
  });
});
