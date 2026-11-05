import { describe, expect, it } from 'vitest';
import { CELL_MAX, CELL_MIN, railFit } from './rail';

describe('railFit', () => {
  it('keeps the full size while there is room', () => {
    expect(railFit(6, 800)).toEqual({ cell: CELL_MAX, gap: 6, paged: false });
  });

  it('tightens until every scene shows', () => {
    const f = railFit(16, 560);
    expect(f.paged).toBe(false);
    expect(f.cell).toBeLessThan(CELL_MAX);
    expect(f.cell).toBeGreaterThanOrEqual(CELL_MIN);
    expect(16 * (f.cell + f.gap) - f.gap).toBeLessThanOrEqual(560);
  });

  it('pages only past the smallest size that reads', () => {
    expect(railFit(16, 300)).toEqual({ cell: CELL_MIN, gap: 2, paged: true });
  });

  it('respects a taller smallest row, for the open list', () => {
    expect(railFit(16, 560, 38).paged).toBe(true);
  });

  it('copes with nothing to measure yet', () => {
    expect(railFit(0, 500).paged).toBe(false);
    expect(railFit(16, 0).paged).toBe(false);
  });
});
