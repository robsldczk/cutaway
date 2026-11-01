// Edit page lanes: the mixer keys used by the core's schedule (character id for dialogue,
// `sfx1…`, `ambience`, `music`, `roomtone`), their labels, colours and mix state.
import { LANE_COLORS, colorOf, type Rgb } from '../theme';
import { app } from './app.svelte';
import type { LaneMix, PlacementView } from './types';

export type LaneGroup = 'dialogue' | 'sfx' | 'ambience' | 'music';

export interface LaneRow {
  kind: 'script' | 'group' | 'lane';
  key: string;
  label: string;
  y: number;
  h: number;
  group: LaneGroup;
  rgb: Rgb;
  char?: string;
  layer?: number;
  /** Which sounds the lane takes (drops). */
  lane?: 'sfx' | 'ambience' | 'music' | 'roomtone' | 'dialogue';
}

export const GROUP_LABEL: Record<LaneGroup, string> = { dialogue: 'Dialogue', sfx: 'Sound effects', ambience: 'Ambience', music: 'Music' };

export function placementLane(p: PlacementView): string {
  if (p.lane === 'dialogue') return p.character ?? 'dialogue';
  return p.lane === 'sfx' ? `sfx${p.layer + 1}` : p.lane;
}

export function laneLabel(key: string): string {
  if (key.startsWith('sfx')) return `SFX ${key.slice(3)}`;
  if (key === 'ambience') return 'Ambience';
  if (key === 'music') return 'Music';
  if (key === 'roomtone') return 'Room tone';
  return app.view?.characters.find((c) => c.id === key)?.label ?? key;
}

export function laneRgb(key: string): Rgb {
  if (key.startsWith('sfx')) return LANE_COLORS.sfx;
  if (key === 'ambience' || key === 'roomtone') return LANE_COLORS.room;
  if (key === 'music') return LANE_COLORS.music;
  return colorOf(app.view?.characters.find((c) => c.id === key)?.color_index ?? 0).rgb;
}

export function mixOf(key: string): LaneMix {
  return app.edit?.lane_mix.find((m) => m.id === key) ?? { id: key, volume_db: 0, pan: 0, mute: false, solo: false };
}

export async function setMix(key: string, patch: Partial<LaneMix>) {
  await app.cmd({ cmd: 'set_lane_mix', mix: { ...mixOf(key), ...patch, id: key } });
  await app.loadEdit();
}

export function panLabel(p: number): string {
  if (Math.abs(p) < 0.01) return 'C';
  return `${p < 0 ? 'L' : 'R'}${Math.round(Math.abs(p) * 100)}`;
}

export function db(v: number): string {
  return `${v > 0 ? '+' : v < 0 ? '−' : ''}${Math.abs(v).toFixed(1)} dB`;
}
