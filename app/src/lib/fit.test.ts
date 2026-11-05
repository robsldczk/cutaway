import { describe, expect, it } from 'vitest';
import { firstFit } from './fit';

describe('firstFit', () => {
  it('stays at the top when it fits', () => {
    expect(firstFit(4, () => false)).toBe(0);
  });

  it('stops at the first step that fits, and tries none past it', () => {
    const tried: number[] = [];
    expect(firstFit(5, (l) => (tried.push(l), l < 2))).toBe(2);
    expect(tried).toEqual([0, 1, 2]);
  });

  it('ends on the last step when nothing fits', () => {
    expect(firstFit(3, () => true)).toBe(3);
  });
});
