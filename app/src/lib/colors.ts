import { colorOf, MIX_BASES, mix, rgba, shade, solid, type Rgb } from '../theme';
import { app } from './app.svelte';
import type { ItemView, SceneView } from './types';

/** A character's colour; the label tint follows the theme. */
export function charColor(view: SceneView | null, character: string | null) {
  const c = colorOf(view?.characters.find((x) => x.id === character)?.color_index ?? 0);
  return { rgb: c.rgb, name: app.theme === 'light' ? c.nameLight : c.name };
}

export function charLabel(view: SceneView | null, character: string | null): string {
  return view?.characters.find((x) => x.id === character)?.label ?? character ?? '';
}

/** The take shade of an item's selected take. */
export function itemShade(view: SceneView | null, it: ItemView, takeIndex = it.take_index ?? 0): Rgb {
  return shade(charColor(view, it.character).rgb, takeIndex);
}

/** The colour gradients mix toward in the current theme. */
export function base(): Rgb {
  return MIX_BASES[app.theme];
}

export { mix, rgba, shade, solid };
