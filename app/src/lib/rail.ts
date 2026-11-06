// How the scene rail fits the window, with no scrollbar: the scenes tighten until they all show.
// Only when even the tightest can't hold them does the rail page.

import type { SceneStatus } from './types';

/** What a scene can be marked as, and the colour that says so. */
export const STATUSES: { id: SceneStatus; label: string; tone: string }[] = [
  { id: 'in_progress', label: 'In progress', tone: '--status-progress-rgb' },
  { id: 'review', label: 'Review', tone: '--status-review-rgb' },
  { id: 'finished', label: 'Finished', tone: '--status-finished-rgb' },
];

export const statusOf = (id: SceneStatus | null | undefined) => STATUSES.find((s) => s.id === id) ?? null;

export const CELL_MAX = 40;
export const CELL_MIN = 26;
const GAP = 6;
const GAP_TIGHT = 2;

export interface RailFit {
  cell: number;
  gap: number;
  /** Some scenes are out of sight: the rail needs its steppers. */
  paged: boolean;
}

/** `height` is what the list of `scenes` has to itself; `min` the smallest row that still reads. */
export function railFit(scenes: number, height: number, min = CELL_MIN): RailFit {
  if (scenes <= 0 || height <= 0) return { cell: CELL_MAX, gap: GAP, paged: false };
  if (scenes * (CELL_MAX + GAP) - GAP <= height) return { cell: CELL_MAX, gap: GAP, paged: false };
  const cell = Math.floor((height + GAP_TIGHT) / scenes) - GAP_TIGHT;
  if (cell >= min) return { cell: Math.min(cell, CELL_MAX), gap: GAP_TIGHT, paged: false };
  return { cell: min, gap: GAP_TIGHT, paged: true };
}
