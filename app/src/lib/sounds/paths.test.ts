import { describe, expect, it } from 'vitest';
import type { PieceView, TakeView } from '../types';
import { arc, focusBody, linePath, panLine, restOfSource, ringPoint, spinePaths } from './paths';
import { rowSpans, TimeMap } from './timemap';

const map = new TimeMap(rowSpans([{ start_s: 0, end_s: 4, top: 0, height: 80 }]));
const take = { id: 't', src_in: 10, duration: 4, peaks: [0, 1, 1, 0] } as unknown as TakeView;
const piece = (over: Partial<PieceView> = {}): PieceView => ({ take_id: 't', source_id: 's', src_in: 10, src_out: 14, start_s: 0, len_s: 4, part: 0, muted: false, ...over });
const line = (character: string, pieces: PieceView[]) => ({ character, pieces, takes: [take], start_s: 0, end_s: 4 });

const xs = (d: string) => [...d.matchAll(/[ML](-?[\d.]+) /g)].map((m) => +m[1]);

describe('the spine', () => {
  it('is one path per character, however many lines', () => {
    const p = spinePaths([line('PAUL', [piece()]), line('SARA', [piece()]), line('PAUL', [piece()])], map, 100);
    expect([...p.keys()]).toEqual(['PAUL', 'SARA']);
    expect(p.get('PAUL')!.match(/Z/g)).toHaveLength(2);
  });

  it('follows the level of the take', () => {
    const d = spinePaths([line('PAUL', [piece()])], map, 100, 12).get('PAUL')!;
    expect(Math.max(...xs(d))).toBeCloseTo(112);
    expect(Math.min(...xs(d))).toBeCloseTo(88);
  });

  it('shows a muted part as a flat line', () => {
    const d = spinePaths([line('PAUL', [piece({ muted: true })])], map, 100, 12).get('PAUL')!;
    expect(Math.max(...xs(d))).toBeCloseTo(100.4);
  });

  it('reads the piece of the take a comp uses', () => {
    const d = spinePaths([line('PAUL', [piece({ src_in: 13, src_out: 14, len_s: 1 })])], map, 100, 12).get('PAUL')!;
    expect(Math.max(...xs(d))).toBeCloseTo(100.4);
  });

  it('survives a take with no peaks and a piece of no length', () => {
    const none = { ...take, peaks: [] } as TakeView;
    expect(spinePaths([{ ...line('PAUL', [piece()]), takes: [none] }], map, 100).get('PAUL')).toContain('Z');
    expect(spinePaths([line('PAUL', [piece({ len_s: 0 })])], map, 100).size).toBe(0);
  });
});

describe('shapes', () => {
  it('draws a length line', () => {
    expect(linePath(10, 1, 25.25)).toBe('M10 1.0V25.3');
  });

  it('draws the thick line with rounded head', () => {
    expect(focusBody(10, 20, 50)).toMatch(/^M7 23\.0Q/);
  });

  it('shows the rest of the source only when there is one', () => {
    expect(restOfSource(10, 20, 21)).toBe('');
    expect(restOfSource(10, 20, 60)).toContain('V60.0');
  });

  it('has no arc for a ring at zero', () => {
    expect(arc(0, 0, 10, 0)).toBe('');
    expect(arc(0, 0, 10, 0.75)).toContain(' 1 1 ');
  });

  it('puts the ring knob on the ring', () => {
    const p = ringPoint(0, 0, 10, 0.25);
    expect(Math.hypot(p.x, p.y)).toBeCloseTo(10);
    expect(p.x).toBeCloseTo(10, 0);
  });

  it('bends the pan line with the pan', () => {
    const q = (d: string) => +(/Q(-?[\d.]+)/.exec(d)?.[1] ?? 0);
    expect(q(panLine(1))).toBeGreaterThan(q(panLine(0.2)));
    expect(q(panLine(-1))).toBeLessThan(50);
  });
});
