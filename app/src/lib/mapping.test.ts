import { describe, expect, it } from 'vitest';
import { around, clock, fit, MIN_SPAN_S, pan, placeWords, textBetween, ticks, zoom } from './mapping';

const w = (text: string, t0: number, t1: number) => ({ text, t0, t1 });

describe('the view of a recording', () => {
  it('stays inside the file and keeps its width', () => {
    expect(pan({ t0: 0, t1: 10 }, -5, 100)).toEqual({ t0: 0, t1: 10 });
    expect(pan({ t0: 80, t1: 90 }, 50, 100)).toEqual({ t0: 90, t1: 100 });
  });

  it('shows a file shorter than the view whole', () => {
    expect(fit({ t0: 0, t1: 30 }, 8)).toEqual({ t0: 0, t1: 8 });
  });

  it('opens with audio either side of a take', () => {
    const v = around(50, 52, 200);
    expect(v.t0).toBeLessThan(50);
    expect(v.t1).toBeGreaterThan(52);
    expect((v.t0 + v.t1) / 2).toBeCloseTo(51);
  });

  it('gives a long take room', () => {
    const v = around(10, 30, 200);
    expect(v.t1 - v.t0).toBeCloseTo(50);
  });

  it('zooms around the point under the mouse', () => {
    const v = zoom({ t0: 10, t1: 20 }, 12, 0.5, 100);
    expect(v.t1 - v.t0).toBeCloseTo(5);
    expect((12 - v.t0) / (v.t1 - v.t0)).toBeCloseTo(0.2);
  });

  it('stops zooming in', () => {
    const v = zoom({ t0: 10, t1: 12 }, 11, 0.1, 100);
    expect(v.t1 - v.t0).toBeCloseTo(MIN_SPAN_S);
  });
});

describe('what is between the marks', () => {
  const words = [w('cut', 1.0, 1.4), w('in', 1.5, 1.9), w('half', 2.0, 2.4)];

  it('counts a word on the side most of it is on', () => {
    expect(textBetween(words, 1.3, 2.1)).toBe('in');
    expect(textBetween(words, 1.1, 2.3)).toBe('cut in half');
  });

  it('is empty where nothing was heard', () => {
    expect(textBetween(words, 5, 6)).toBe('');
  });
});

describe('words on the waveform', () => {
  it('spreads neighbours over the rows and leaves out what does not fit', () => {
    const many = [w('aaaa', 0, 0.1), w('bbbb', 0.1, 0.2), w('cccc', 0.2, 0.3)];
    const placed = placeWords(many, { t0: 0, t1: 10 }, 100, 8, 2);
    expect(placed.map((p) => p.row)).toEqual([0, 1]);
  });

  it('keeps one row when there is room', () => {
    const placed = placeWords([w('a', 0, 0.1), w('b', 5, 5.1)], { t0: 0, t1: 10 }, 100, 8, 3);
    expect(placed.map((p) => p.row)).toEqual([0, 0]);
  });

  it('leaves out what is out of view', () => {
    expect(placeWords([w('a', 20, 21)], { t0: 0, t1: 10 }, 100, 8, 2)).toEqual([]);
  });
});

describe('the ruler', () => {
  it('marks round times, further apart when zoomed out', () => {
    expect(ticks({ t0: 0.2, t1: 3 }, 200)).toEqual([0.5, 1, 1.5, 2, 2.5, 3]);
    expect(ticks({ t0: 0, t1: 100 }, 10)).toEqual([0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100]);
  });

  it('reads as minutes and seconds', () => {
    expect(clock(0)).toBe('0:00.00');
    expect(clock(83.456)).toBe('1:23.46');
    expect(clock(59.999)).toBe('1:00.00');
  });
});
