import { describe, expect, it } from 'vitest';
import { DEPTH_STEP, GAIN_STEP_DB, PAN_STEP, soundKey, TIME_FINE_S, TIME_STEP_S } from './keymap';

const k = (key: string, mods: Partial<{ altKey: boolean; shiftKey: boolean; metaKey: boolean; ctrlKey: boolean }> = {}) => soundKey({ key, altKey: false, shiftKey: false, metaKey: false, ctrlKey: false, ...mods });

describe('keys on the selected sounds', () => {
  it('←/→ push back and bring forward', () => {
    expect(k('ArrowRight')).toEqual({ kind: 'depth', delta: DEPTH_STEP });
    expect(k('ArrowLeft')).toEqual({ kind: 'depth', delta: -DEPTH_STEP });
  });

  it('⌥←/→ pan', () => {
    expect(k('ArrowLeft', { altKey: true })).toEqual({ kind: 'pan', delta: -PAN_STEP });
  });

  it('⇧↑/↓ set loudness', () => {
    expect(k('ArrowUp', { shiftKey: true })).toEqual({ kind: 'gain', delta: GAIN_STEP_DB });
    expect(k('ArrowDown', { shiftKey: true })).toEqual({ kind: 'gain', delta: -GAIN_STEP_DB });
  });

  it(', and . move in time, finer with ⇧', () => {
    expect(k('.')).toEqual({ kind: 'time', delta: TIME_STEP_S });
    expect(k(',')).toEqual({ kind: 'time', delta: -TIME_STEP_S });
    expect(k('>', { shiftKey: true })).toEqual({ kind: 'time', delta: TIME_FINE_S });
  });

  it('⌫ removes and Esc clears', () => {
    expect(k('Backspace')).toEqual({ kind: 'remove' });
    expect(k('Delete')).toEqual({ kind: 'remove' });
    expect(k('Escape')).toEqual({ kind: 'clear' });
  });

  it('leaves the rest of the map alone', () => {
    for (const key of ['ArrowUp', 'ArrowDown', ' ', 'Enter', 's', 'f']) expect(k(key)).toBe(null);
    expect(k('ArrowUp', { altKey: true })).toBe(null);
    expect(k('z', { metaKey: true })).toBe(null);
    expect(k('Backspace', { metaKey: true })).toBe(null);
    expect(k('ArrowLeft', { shiftKey: true })).toBe(null);
  });
});
