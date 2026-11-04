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

/** m:ss for lengths of files and sounds. */
export function len(sec: number): string {
  const s = Math.max(0, Math.round(sec));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

/** "2 min ago" for a past ISO time. */
export function ago(iso: string, now = Date.now()): string {
  const d = Math.max(0, (now - Date.parse(iso)) / 1000);
  if (d < 45) return 'just now';
  if (d < 3600) return `${Math.max(1, Math.round(d / 60))} min ago`;
  if (d < 86400) return `${Math.round(d / 3600)} h ago`;
  return `${Math.round(d / 86400)} d ago`;
}

/** The first sentence of a note (the whole note when it has no sentence end). */
export function firstSentence(s: string): string {
  const m = /^.*?[.!?](?=\s|$)/.exec(s.trim());
  return m ? m[0] : s.trim();
}
