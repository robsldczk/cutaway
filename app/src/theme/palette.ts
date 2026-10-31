// Character colors of the default skin: traditional Japanese colors, matched in lightness.
// Each has a lighter "name" tint for labels. Takes are told apart by shades of the color.

export type Rgb = [number, number, number];

export interface CharacterColor {
  key: string;
  rgb: Rgb;
  /** Label tint on the dark stage. */
  name: string;
  /** Label tint on light paper (same hue, enough contrast). */
  nameLight: string;
}

export const CHARACTER_COLORS: CharacterColor[] = [
  { key: 'kaki', rgb: [224, 138, 90], name: '#EDAA82', nameLight: '#A9512A' },
  { key: 'asagi', rgb: [94, 184, 176], name: '#8FD3CC', nameLight: '#23766F' },
  { key: 'matcha', rgb: [157, 180, 106], name: '#BFD192', nameLight: '#56722A' },
  { key: 'fuji', rgb: [160, 143, 208], name: '#C3B7E6', nameLight: '#5F4BA0' },
  { key: 'ai', rgb: [96, 120, 190], name: '#A9B8E6', nameLight: '#34509A' },
  { key: 'kurenai', rgb: [204, 92, 104], name: '#E9A6AE', nameLight: '#9E2F3B' },
  { key: 'yamabuki', rgb: [222, 172, 72], name: '#EFCB8A', nameLight: '#8A5F06' },
];

/** Shade steps per take index: lighter, darker, lighter still, darker still. */
export const TAKE_SHADES = [0, 0.34, -0.32, 0.58, -0.52];

export type ThemeMode = 'dark' | 'light';

/** What shades and gradients mix toward: the stage colour of each theme. */
export const MIX_BASES: Record<ThemeMode, Rgb> = { dark: [25, 27, 35], light: [246, 243, 236] };
export const MIX_BASE: Rgb = MIX_BASES.dark;
const SHADE_DARK = 18;

export function colorOf(index: number): CharacterColor {
  return CHARACTER_COLORS[((index % CHARACTER_COLORS.length) + CHARACTER_COLORS.length) % CHARACTER_COLORS.length];
}

export function shade(rgb: Rgb, k: number): Rgb {
  const t = TAKE_SHADES[((k % TAKE_SHADES.length) + TAKE_SHADES.length) % TAKE_SHADES.length];
  return rgb.map((v) => Math.round(t >= 0 ? v + (255 - v) * t : v + (SHADE_DARK - v) * -t)) as Rgb;
}

export function rgba(c: Rgb, a: number): string {
  return `rgba(${c[0]},${c[1]},${c[2]},${a})`;
}

export function mix(c: Rgb, base: Rgb, a: number): Rgb {
  return [0, 1, 2].map((i) => Math.round(base[i] + (c[i] - base[i]) * a)) as Rgb;
}

export function solid(c: Rgb): string {
  return `rgb(${c[0]},${c[1]},${c[2]})`;
}
