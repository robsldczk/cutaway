// Keys on the sounds layer. The step sizes live here. Arrows already mean lines and takes on
// Arrange, so they only change meaning while sounds are selected (D78).
import type { Part } from './opened';

export type SoundKey =
  | { kind: 'depth'; delta: number }
  | { kind: 'pan'; delta: number }
  | { kind: 'gain'; delta: number }
  | { kind: 'time'; delta: number }
  | { kind: 'remove' }
  | { kind: 'clear' };

export const DEPTH_STEP = 0.05;
export const PAN_STEP = 0.05;
export const GAIN_STEP_DB = 1;
export const TIME_STEP_S = 0.1;
export const TIME_FINE_S = 0.01;

/** What a key does to the selected sounds, or nothing: the key keeps its usual meaning. */
export function soundKey(e: Pick<KeyboardEvent, 'key' | 'altKey' | 'shiftKey' | 'metaKey' | 'ctrlKey'> & { code?: string }): SoundKey | null {
  if (e.metaKey || e.ctrlKey) return null;
  const k = e.key;
  if (k === 'Escape') return { kind: 'clear' };
  if (k === 'Backspace' || k === 'Delete') return { kind: 'remove' };
  if (k === 'ArrowLeft' || k === 'ArrowRight') {
    if (e.shiftKey) return null;
    const dir = k === 'ArrowLeft' ? -1 : 1;
    return e.altKey ? { kind: 'pan', delta: dir * PAN_STEP } : { kind: 'depth', delta: dir * DEPTH_STEP };
  }
  if ((k === 'ArrowUp' || k === 'ArrowDown') && e.shiftKey) return { kind: 'gain', delta: (k === 'ArrowUp' ? 1 : -1) * GAIN_STEP_DB };
  // what ⇧ gives on these keys depends on the layout (German: ; and :), so go by the key too
  const back = k === ',' || k === '<' || (e.shiftKey && e.code === 'Comma');
  const on = k === '.' || k === '>' || (e.shiftKey && e.code === 'Period');
  if (back || on) return { kind: 'time', delta: (back ? -1 : 1) * (e.shiftKey ? TIME_FINE_S : TIME_STEP_S) };
  return null;
}

type Key = Pick<KeyboardEvent, 'key' | 'altKey' | 'shiftKey' | 'metaKey' | 'ctrlKey'>;

/** ↑ and ↓ on a part of the opened sound, in the view's own direction: up is earlier. ⇧ stays
 *  loudness. ⌥ is the fine step, and on the body it slips, as ⌥ drag does. */
export function partKey(e: Key, part: Part): { part: Part; delta: number } | null {
  if (e.metaKey || e.ctrlKey || e.shiftKey) return null;
  if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return null;
  const dir = e.key === 'ArrowUp' ? -1 : 1;
  if (part === 'move' && e.altKey) return { part: 'slip', delta: dir * TIME_STEP_S };
  return { part, delta: dir * (e.altKey ? TIME_FINE_S : TIME_STEP_S) };
}
