// Cover images on their way from the core. The webview has no file access, so an image comes as
// a data URL, asked for once per size and kept for as long as the app runs.
import { SvelteMap } from 'svelte/reactivity';
import { call } from './bridge';
import type { CoverRef } from './welcome';

/** Tiles, and the hero. */
export const TILE_PX = 480;
export const HERO_PX = 1800;

const urls = new SvelteMap<string, string>();
// written while a page renders, so it mustn't be reactive
// eslint-disable-next-line svelte/prefer-svelte-reactivity
const asked = new Set<string>();

/** The image's data URL, or null until it has arrived. An image that can't be read stays null:
 *  its item keeps its colour wash. */
export function coverUrl(ref: CoverRef, px: number): string | null {
  const key = `${ref.path}|${ref.stamp}|${px}`;
  if (!asked.has(key)) {
    asked.add(key);
    void call<{ url: string }>('cover_image', { path: ref.path, px })
      .then((r) => urls.set(key, r.url))
      .catch(() => {});
  }
  return urls.get(key) ?? null;
}
