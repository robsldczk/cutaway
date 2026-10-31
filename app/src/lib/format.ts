/** mm:ss.s like the prototype's time chip. */
export function fmt(sec: number): string {
  const s = Math.max(0, sec);
  const m = Math.floor(s / 60);
  const r = s - m * 60;
  return `${m < 10 ? '0' : ''}${m}:${r < 10 ? '0' : ''}${r.toFixed(1)}`;
}

/** HH:MM:SS:FF at 25 fps (source timecode). */
export function tc(sec: number, fps = 25): string {
  const f = Math.floor(sec * fps + 1e-6);
  const ff = f % fps;
  const s = Math.floor(f / fps);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${p(Math.floor(s / 3600))}:${p(Math.floor(s / 60) % 60)}:${p(s % 60)}:${p(ff)}`;
}

export const clamp = (v: number, a: number, b: number) => Math.max(a, Math.min(b, v));
