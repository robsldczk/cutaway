import { describe, expect, it } from 'vitest';
import { stepChip } from './chips';

const chips = ['Ambience', 'Doors', 'Steps'];

describe('walking the chips', () => {
  it('starts at the ends with nothing lit', () => {
    expect(stepChip(chips, null, 1)).toBe('Ambience');
    expect(stepChip(chips, null, -1)).toBe('Steps');
  });

  it('steps on from the chip the name already is', () => {
    expect(stepChip(chips, 'Doors', 1)).toBe('Steps');
    expect(stepChip(chips, 'Doors', -1)).toBe('Ambience');
  });

  it('goes round the ends', () => {
    expect(stepChip(chips, 'Steps', 1)).toBe('Ambience');
    expect(stepChip(chips, 'Ambience', -1)).toBe('Steps');
  });

  it('starts over from a chip that has gone', () => {
    expect(stepChip(chips, 'Gone', 1)).toBe('Ambience');
  });

  it('has nowhere to go without chips', () => {
    expect(stepChip([], null, 1)).toBeNull();
  });
});
