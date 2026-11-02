// Edit page tools with their Logic letters (T opens the tool menu, then the letter).
import type { Tool } from './app.svelte';

export const TOOLS: { id: Tool; label: string; key: string }[] = [
  { id: 'pointer', label: 'Pointer', key: 'T' },
  { id: 'split', label: 'Scissors', key: 'I' },
  { id: 'fade', label: 'Fade', key: 'A' },
  { id: 'gain', label: 'Gain', key: 'G' },
  { id: 'marquee', label: 'Marquee', key: 'R' },
];
