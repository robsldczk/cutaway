import { colorOf, mix, MIX_BASE, rgba, shade, solid, type Rgb } from '../theme';
import type { ItemView, SceneView } from './types';

export function charColor(view: SceneView | null, character: string | null) {
  const c = view?.characters.find((x) => x.id === character);
  return colorOf(c?.color_index ?? 0);
}

export function charLabel(view: SceneView | null, character: string | null): string {
  return view?.characters.find((x) => x.id === character)?.label ?? character ?? '';
}

/** The take shade of an item's selected take. */
export function itemShade(view: SceneView | null, it: ItemView, takeIndex = it.take_index ?? 0): Rgb {
  return shade(charColor(view, it.character).rgb, takeIndex);
}

export { mix, MIX_BASE, rgba, shade, solid };
