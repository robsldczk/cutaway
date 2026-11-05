import { describe, expect, it } from 'vitest';
import { lineText, lineWords, takeWords } from './words';

const lw = (idx: number, text: string, start_s: number, o = {}) => ({ idx, text, start_s, end_s: start_s + 0.3, deleted: false, muted: false, part: 0, ...o });

describe('a line reads as it was said', () => {
  it('shows every word of the take, in or out of the script', () => {
    const it0 = { text: 'The.', dropped: false, words: [lw(0, 'So', 1), lw(1, 'the', 1.4), lw(2, 'machine', 1.8)] };
    expect(lineText(it0)).toBe('So the machine');
  });

  it('is in time order, each word its own', () => {
    const shown = lineWords({ words: [lw(1, 'b', 2), lw(0, 'a', 1)] });
    expect(shown.map((w) => w.text)).toEqual(['a', 'b']);
    expect(shown[0].idxs).toEqual([0]);
  });

  it('leaves out what was deleted, and the parts it is not asked for', () => {
    const it0 = { text: 'x', dropped: false, words: [lw(0, 'keep', 1), lw(1, 'gone', 1.4, { deleted: true }), lw(2, 'later', 2, { part: 1 })] };
    expect(lineText(it0)).toBe('keep later');
    expect(lineText(it0, 1)).toBe('later');
  });

  it('falls back on the script with nothing to play', () => {
    expect(lineText({ text: 'The script.', dropped: false, words: [] })).toBe('The script.');
    expect(lineText({ text: 'The script.', dropped: true, words: [lw(0, 'heard', 1)] })).toBe('The script.');
  });

  it('reads a take by its own words', () => {
    expect(takeWords([{ text: 'hi', t0: 0.1, t1: 0.4 }]).map((w) => [w.text, w.start])).toEqual([['hi', 0.1]]);
  });
});
