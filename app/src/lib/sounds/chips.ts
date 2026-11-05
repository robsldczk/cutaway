// Walking a row of chips with the arrows: round the ends, from where the name already is.

/** The chip `by` steps on from `from`. With nothing lit, forward starts at the first chip and
 *  back at the last. */
export function stepChip(chips: string[], from: string | null, by: 1 | -1): string | null {
  const n = chips.length;
  if (!n) return null;
  const at = from == null ? -1 : chips.indexOf(from);
  if (at < 0) return by > 0 ? chips[0] : chips[n - 1];
  return chips[(at + by + n) % n];
}
