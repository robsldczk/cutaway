import { describe, expect, it } from 'vitest';
import { DEPTH_STEP, GAIN_STEP_DB, PAN_STEP, partKey, soundKey, TIME_FINE_S, TIME_STEP_S } from './keymap';

const k = (key: string, mods: Partial<{ altKey: boolean; shiftKey: boolean; metaKey: boolean; ctrlKey: boolean; code: string }> = {}) => soundKey({ key, altKey: false, shiftKey: false, metaKey: false, ctrlKey: false, ...mods });

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

  it('the fine move works on a German layout, where ⇧ gives ; and :', () => {
    expect(k(';', { shiftKey: true, code: 'Comma' })).toEqual({ kind: 'time', delta: -TIME_FINE_S });
    expect(k(':', { shiftKey: true, code: 'Period' })).toEqual({ kind: 'time', delta: TIME_FINE_S });
  });

  it('a ; or : typed on another key is left alone', () => {
    expect(k(';', { code: 'Semicolon' })).toBe(null);
    expect(k(':', { shiftKey: true, code: 'Semicolon' })).toBe(null);
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

describe('keys on a part of the opened sound', () => {
  const key = (k: string, mods: { altKey?: boolean; shiftKey?: boolean; metaKey?: boolean } = {}) => ({ key: k, altKey: false, shiftKey: false, metaKey: false, ctrlKey: false, ...mods });

  it('steps 100 ms, up being earlier', () => {
    expect(partKey(key('ArrowUp'), 'in')).toEqual({ part: 'in', delta: -0.1 });
    expect(partKey(key('ArrowDown'), 'fadeOut')).toEqual({ part: 'fadeOut', delta: 0.1 });
  });

  it('⌥ is the fine step on an edge or a fade', () => {
    expect(partKey(key('ArrowDown', { altKey: true }), 'out')).toEqual({ part: 'out', delta: 0.01 });
  });

  it('⌥ on the body slips, as ⌥ drag does', () => {
    expect(partKey(key('ArrowDown', { altKey: true }), 'move')).toEqual({ part: 'slip', delta: 0.1 });
  });

  it('leaves ⇧, ⌘ and other keys to the layer', () => {
    expect(partKey(key('ArrowUp', { shiftKey: true }), 'in')).toBeNull();
    expect(partKey(key('ArrowUp', { metaKey: true }), 'in')).toBeNull();
    expect(partKey(key('ArrowLeft'), 'in')).toBeNull();
  });
});
