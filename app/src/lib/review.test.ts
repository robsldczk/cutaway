import { describe, expect, it } from 'vitest';
import { arrivalLine, filtered, placeCards, quoted, reviewRows, roundLine, threadsOf, type Note, type RoundView } from './review';
import type { ItemView } from './types';

const note = (over: Partial<Note>): Note =>
  ({ id: 'n', status: 'open', mine: false, tag: 'timing', round: 'r1', thread: 'n', on: 'word', quote: 'huge', ...over }) as Note;

describe('the notes shown', () => {
  const notes = [note({ id: 'a', status: 'resolved' }), note({ id: 'b', mine: true, tag: 'sound' }), note({ id: 'c', status: 'reopened', round: 'r2' })];
  const ids = (x: Note[]) => x.map((n) => n.id).join('');

  it('open takes in the reopened and leaves out the resolved', () => {
    expect(ids(filtered(notes, 'open', null))).toBe('bc');
  });
  it('mine is the reviewer’s own', () => {
    expect(ids(filtered(notes, 'mine', null))).toBe('b');
  });
  it('by kind and by round', () => {
    expect(ids(filtered(notes, 'all', 'sound'))).toBe('b');
    expect(ids(filtered(notes, 'all', null, 'r2'))).toBe('c');
  });
  it('a thread stays together', () => {
    const t = threadsOf([note({ id: 'a', thread: 'a' }), note({ id: 'b', thread: 'b' }), note({ id: 'c', thread: 'a' })]);
    expect(t.map(ids)).toEqual(['ac', 'b']);
  });
});

describe('what a note says of itself', () => {
  it('words are quoted, a pause and a sound are named', () => {
    expect(quoted({ on: 'word', quote: 'huge' })).toBe('“huge”');
    expect(quoted({ on: 'pause', quote: 'after huge' })).toBe('after huge');
    expect(quoted({ on: 'scene', quote: '' })).toBe('');
  });
  it('an arrival reads as a sentence', () => {
    const a = { id: 'x', at: '2026-11-03T14:02:00Z', who: 'Albert', via: 'folder' as const, notes: 12, replies: 3, changes: 0 };
    expect(arrivalLine(a)).toMatch(/^Albert, 12 new notes, 3 replies, via shared folder, Tue \d\d:\d\d$/);
    expect(arrivalLine({ ...a, who: 'Anna', via: 'file', notes: 1, replies: 1 })).toMatch(/^Anna, 1 new note, 1 reply, added from file/);
  });
  it('a round says what it covered', () => {
    const r = { created_at: '2026-11-06T10:00:00Z', cut: 'v004', notes: 7, scenes: [{ number: 2 }, { number: 3 }] } as RoundView;
    expect(roundLine(r)).toBe('6 Nov · Sc. 2, 3 · v004 · 7 notes');
    expect(roundLine({ ...r, cut: null, notes: 1 })).toBe('6 Nov · Sc. 2, 3 · 1 note');
  });
});

describe('the column', () => {
  const line = (id: string, start_s: number, end_s: number) => ({ kind: 'line', line_id: id, start_s, end_s }) as ItemView;

  it('a pause is as tall as it is long, up to a point', () => {
    const { rows } = reviewRows([line('a', 2, 3), line('b', 3.3, 4), line('c', 60, 61)]);
    const gaps = rows.filter((r) => r.kind === 'gap');
    expect(gaps[0].height).toBe(22);
    expect(gaps[0].label).toBe('‖ 2.0 s');
    expect(gaps[1].label).toBe('');
    expect(gaps[2].height).toBe(80);
  });

  it('two lines that overlap have no pause between them', () => {
    const { rows } = reviewRows([line('a', 0, 3), line('b', 2, 4)]);
    expect(rows.map((r) => r.kind)).toEqual(['line', 'line']);
  });

  it('cards push down instead of covering each other', () => {
    const p = placeCards([{ item: 'b', y: 110, height: 80 }, { item: 'a', y: 100, height: 80 }]);
    expect(p.map((x) => [x.item, x.top])).toEqual([['a', 80], ['b', 170]]);
  });
});
