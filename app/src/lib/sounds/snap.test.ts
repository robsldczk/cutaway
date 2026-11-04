import { describe, expect, it } from 'vitest';
import type { ItemView, LayoutWord } from '../types';
import { nearestTarget, snapTargets, startLabel } from './snap';
import { rowSpans, TimeMap } from './timemap';

const w = (idx: number, text: string, start_s: number, end_s: number, more: Partial<LayoutWord> = {}): LayoutWord => ({ idx, text, take_id: 't', start_s, end_s, deleted: false, script_idx: idx, part: 0, muted: false, ...more });

function line(over: Partial<ItemView>): ItemView {
  return { kind: 'line', line_id: 'a', text: 'That is just time slop.', tokens: ['That', 'is', 'just', 'time', 'slop.'], start_s: 1, end_s: 3, pieces: [{ take_id: 't' }], words: [w(0, 'that', 1, 1.2), w(1, 'is', 1.2, 1.4), w(2, 'just', 1.9, 2.1), w(3, 'time', 2.1, 2.4), w(4, 'slop', 2.4, 3)], ...over } as unknown as ItemView;
}

describe('snap targets', () => {
  it('has the line edges and the pauses, named by their words', () => {
    const t = snapTargets([line({})]);
    expect(t.map((x) => x.t)).toEqual([1, 1.4, 3]);
    expect(t[0].start).toBe('On “That is”');
    expect(t[0].end).toBe('Ends before “That is”');
    expect(t[1].start).toBe('In the pause after “is”');
    expect(t[2].start).toBe('After “slop.”');
    expect(t[2].end).toBe('Ends with “slop.”');
  });

  it('carries the anchor a snap stands for', () => {
    const t = snapTargets([line({})]);
    expect(t[0].anchor).toEqual({ line_id: 'a', word: null, edge: 'start' });
    expect(t[1].anchor).toEqual({ line_id: 'a', word: 1, edge: 'end' });
    expect(t[2].anchor).toEqual({ line_id: 'a', word: null, edge: 'end' });
  });

  it('skips a short breath between words', () => {
    expect(snapTargets([line({})], 0.6).map((x) => x.t)).toEqual([1, 3]);
  });

  it('skips words that were cut, and pauses it could not anchor', () => {
    const words = [w(0, 'that', 1, 1.2), w(1, 'is', 1.2, 1.4, { deleted: true }), w(2, 'just', 1.9, 2.1, { script_idx: null }), w(3, 'time', 2.5, 3)];
    expect(snapTargets([line({ words })]).map((x) => x.t)).toEqual([1, 1.2, 3]);
  });

  it('leaves out lines with no audio on the timeline', () => {
    expect(snapTargets([line({ pieces: [] })])).toEqual([]);
    expect(snapTargets([line({ end_s: 1 })])).toEqual([]);
    expect(snapTargets([line({ kind: 'spacer' })])).toEqual([]);
  });

  it('names a line with no timed words by its text', () => {
    const t = snapTargets([line({ words: [] })]);
    expect(t[0].start).toBe('On “That is”');
    expect(t[1].start).toBe('After “slop.”');
  });
});

describe('snapping', () => {
  const map = new TimeMap(rowSpans([{ start_s: 0, end_s: 10, top: 0, height: 100 }]));
  const targets = snapTargets([line({ start_s: 2, end_s: 6, words: [] })]);

  it('takes the nearest target within 8 px on the spine', () => {
    expect(nearestTarget(targets, 2.5, map)?.t).toBe(2);
    expect(nearestTarget(targets, 5.5, map)?.t).toBe(6);
  });

  it('leaves free time alone', () => {
    expect(nearestTarget(targets, 4, map)).toBe(null);
    expect(nearestTarget(targets, 2.9, map)).toBe(null);
  });

  it('measures on screen, not in seconds', () => {
    // the same time away is near on a stretched row and far on a tight one
    const tight = new TimeMap(rowSpans([{ start_s: 0, end_s: 10, top: 0, height: 20 }]));
    const wide = new TimeMap(rowSpans([{ start_s: 0, end_s: 10, top: 0, height: 400 }]));
    expect(nearestTarget(targets, 3, tight)?.t).toBe(2);
    expect(nearestTarget(targets, 3, wide)).toBe(null);
  });

  it('tells where a free sound starts', () => {
    expect(startLabel(null, 12.34)).toBe('Starts at 00:12.3');
    expect(startLabel(targets[0], 2)).toBe('On “That is”');
  });
});
