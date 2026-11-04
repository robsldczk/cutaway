<script lang="ts">
  // The opened sound: a lean vertical editor beside the spine, zoomed to one sound, with the
  // dialogue of the same stretch next to it. Time runs down, as on the spine. The rules are in
  // lib/sounds/opened; a gesture previews through the layer and sends one command on release.
  import { onMount, untrack } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import { charColor, rgba } from '../lib/colors';
  import { clamp } from '../lib/format';
  import { partKey } from '../lib/sounds/keymap';
  import { autoFadeToggle, fadeLevel, keepWindow, nearestIn, openWindow, partAt, partText, quickLen, quickOn, readout, reshape, reshapeCommands, snapTol, tOf, yOf, type OpenState, type Ops, type Part, type Quick, type Reshaped, type Win } from '../lib/sounds/opened';
  import { clipLines, columnWave, peakAt, spinePaths } from '../lib/sounds/paths';
  import { dbText, panText, ringOf, zone } from '../lib/sounds/select';
  import type { SnapTarget } from '../lib/sounds/snap';
  import type { ItemView } from '../lib/types';

  let { s, assetId, peaks, name, bucket, tone, lines, targets, box, line, ops, onclose }: {
    s: OpenState;
    assetId: string;
    /** The sound's envelope from the in point, until the source's peaks arrive. */
    peaks: number[];
    name: string;
    bucket: string;
    tone: string;
    lines: ItemView[];
    targets: SnapTarget[];
    box: { left: number; top: number; width: number; height: number };
    /** The sound's length line, where the wedge starts. */
    line: { x: number; y0: number; y1: number };
    ops: Ops;
    onclose: (refocus: boolean) => void;
  } = $props();

  const HEAD = 72;
  const FOOT = 90;
  const SOUND_HALF = 62;
  const DIALOGUE_X = 40;
  const DIALOGUE_AMP = 14;
  const PRESS_PX = 2;
  const PEAK_PX = 2;
  const MAX_PEAKS = 2000;
  const QUICKS: { kind: Quick; label: string }[] = [
    { kind: 'line', label: 'End with line' },
    { kind: 'next', label: 'Until next line' },
    { kind: 'full', label: 'Full' },
  ];

  let card = $state<HTMLDivElement>();
  const H = $derived(Math.max(120, box.height - HEAD - FOOT));
  const midX = $derived(Math.round(box.width * 0.5));
  const xl = $derived(midX - SOUND_HALF - 8);
  const xr = $derived(midX + SOUND_HALF + 8);
  const rulerX = $derived(box.width - 56);

  // the view holds still through a gesture and fits itself after one
  let win = $state.raw<Win>(untrack(() => openWindow(s)));
  let busy = $state(false);
  $effect(() => {
    const now = s;
    if (!busy) win = keepWindow(untrack(() => win), now);
  });
  const y = (t: number) => yOf(win, H, t);

  // ---- what is drawn ----

  let source = $state.raw<number[]>([]);
  onMount(() => {
    const full = s.asset_len_s;
    if (full > 0) {
      const buckets = clamp(Math.round(((full / win.len) * H) / PEAK_PX), 64, MAX_PEAKS);
      call<number[][]>('peaks_batch', { ranges: [{ source_id: `asset:${assetId}`, t0: 0, t1: full, buckets, normalize: true }] })
        .then((r) => (source = r[0] ?? []))
        .catch(() => (source = []));
    }
    card?.focus();
  });

  const tIn = $derived(s.start_s);
  const tOut = $derived(s.start_s + s.len_s);
  const src0 = $derived(s.start_s - s.in_s);
  const src1 = $derived(s.asset_len_s > 0 ? src0 + s.asset_len_s : tOut);
  const u0 = $derived(clamp(y(tIn), 0, H));
  const u1 = $derived(clamp(y(tOut), 0, H));
  const s0 = $derived(clamp(y(src0), 0, H));
  const s1 = $derived(clamp(y(src1), 0, H));
  const yFi = $derived(y(tIn + s.fade_in_s));
  const yFo = $derived(y(tOut - s.fade_out_s));

  /** Height follows loudness. */
  const amp = $derived(SOUND_HALF * (0.35 + 0.65 * ringOf(s.gain_db)));
  const levelAt = (t: number) => (source.length ? peakAt(source, (t - src0) / Math.max(0.001, src1 - src0)) : peakAt(peaks, (t - tIn) / Math.max(0.001, s.len_s)));
  const whole = $derived(source.length ? columnWave(midX, amp, s0, s1, (yy) => levelAt(tOf(win, H, yy))) : '');
  const used = $derived(
    columnWave(midX, amp, u0, u1, (yy) => {
      const t = tOf(win, H, yy);
      return levelAt(t) * fadeLevel(s, t);
    }),
  );
  const fadeLines = $derived(`M${xl} ${yFi.toFixed(1)}L${midX} ${y(tIn).toFixed(1)}L${xr} ${yFi.toFixed(1)}M${xl} ${yFo.toFixed(1)}L${midX} ${y(tOut).toFixed(1)}L${xr} ${yFo.toFixed(1)}`);
  const dialogue = $derived(
    [...spinePaths(clipLines(lines, win.t0, win.t0 + win.len), { y }, DIALOGUE_X, DIALOGUE_AMP)].map(([who, d]) => ({ who, d, fill: rgba(charColor(app.view, who).rgb, 0.95) })),
  );

  const ticks = $derived.by(() => {
    const step = win.len > 40 ? 10 : win.len > 16 ? 5 : win.len > 6 ? 2 : 0.5;
    const out: { y: number; t: string }[] = [];
    const first = Math.ceil((win.t0 - tIn) / step);
    for (let k = first; tIn + k * step <= win.t0 + win.len; k++) out.push({ y: y(tIn + k * step), t: `${(k * step).toFixed(1).replace('-', '−')} s` });
    return out;
  });

  const head = $derived(y(app.pos));
  const wedge = $derived.by(() => {
    const top = box.top + HEAD;
    return `M${line.x + 4} ${line.y0.toFixed(1)}L${box.left} ${(top + u0).toFixed(1)}L${box.left} ${(top + u1).toFixed(1)}L${line.x + 4} ${line.y1.toFixed(1)}Z`;
  });
  const reading = $derived(`${bucket} · ${dbText(s.gain_db)} · ${zone(s.depth)} · ${panText(s.pan)}`);
  const body = $derived({ top: u0 + 8, height: Math.max(0, u1 - u0 - 16) });

  // ---- gestures ----

  interface Grab {
    part: Part;
    base: OpenState;
    y0: number;
    moved: boolean;
    /** What it does now: ⌥ turns a move into a slip and back, mid drag. */
    now: Part;
    r: Reshaped | null;
  }

  let grab: Grab | null = null;
  let tip = $state<{ y: number; text: string } | null>(null);
  let snapT = $state<number | null>(null);

  function press(e: MouseEvent, part: Part) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).focus();
    grab = { part, base: { ...s }, y0: e.clientY, moved: false, now: part, r: null };
    busy = true;
    ops.begin();
    window.addEventListener('mousemove', drag);
    window.addEventListener('mouseup', release);
  }

  function drag(e: MouseEvent) {
    const g = grab;
    if (!g) return;
    const dy = e.clientY - g.y0;
    if (!g.moved && Math.abs(dy) < PRESS_PX) return;
    g.moved = true;
    const now: Part = g.part === 'move' && e.altKey ? 'slip' : g.part;
    const wanted = partAt(now, g.base) + (dy / H) * win.len;
    const snaps = now === 'in' || now === 'out' || now === 'move';
    const r = reshape(now, g.base, wanted, snaps ? nearestIn(targets, wanted, snapTol(win, H)) : null);
    g.now = now;
    g.r = r;
    // back to the base first: a drag can turn from a slip into a move halfway
    ops.preview({ start_s: g.base.start_s, in_s: g.base.in_s, ...r.patch });
    snapT = r.target?.t ?? null;
    tip = { y: clamp(y(r.at), 12, H - 12), text: partText(now, g.base, r) };
  }

  function stop() {
    window.removeEventListener('mousemove', drag);
    window.removeEventListener('mouseup', release);
    grab = null;
    tip = null;
    snapT = null;
    busy = false;
  }

  function release() {
    const g = grab;
    if (!g) return;
    stop();
    const cmds = g.moved && g.r ? reshapeCommands(g.now, g.base, g.r) : [];
    ops.finish(cmds, g.r && cmds.length ? partText(g.now, g.base, g.r) : '');
  }

  function cancel() {
    stop();
    ops.cancel();
  }

  onMount(() => () => {
    if (grab) cancel();
  });

  function quick(kind: Quick) {
    const r = reshape('out', s, s.start_s + quickLen(kind, s, lines));
    ops.apply(r.patch, reshapeCommands('out', s, r), partText('out', s, r));
  }

  function toggleAuto() {
    const r = autoFadeToggle(s);
    ops.apply(r.patch, [r.cmd], r.patch.auto_fade ? 'Auto fade on' : 'Auto fade off');
  }

  // ---- keys: the layer asks here first while the view is open ----

  export function key(e: KeyboardEvent): boolean {
    if (e.key === 'Escape') {
      if (grab) cancel();
      else onclose(true);
      return true;
    }
    if (grab) return false;
    const el = e.target as HTMLElement | null;
    if (!el || !card?.contains(el)) return false;
    if (e.key === 'Tab') return wrap(e, el);
    // Enter here is not "play the line"; a button reached with Tab never gets this far
    if (e.key === 'Enter') return true;
    const part = el.dataset.part as Part | undefined;
    const k = part ? partKey(e, part) : null;
    if (!k) return false;
    ops.nudge(k.part, k.delta);
    return true;
  }

  /** Tab stays in the view; Esc is the way out. */
  function wrap(e: KeyboardEvent, el: HTMLElement) {
    const stops = [...(card?.querySelectorAll<HTMLElement>('button, [data-part]') ?? [])];
    const i = stops.indexOf(el);
    const last = stops.length - 1;
    const next = e.shiftKey ? (i <= 0 ? last : -1) : i === last ? 0 : -1;
    if (next < 0) return false;
    stops[next]?.focus();
    return true;
  }

  const secs = (x: number) => +x.toFixed(2);
</script>

{#snippet handle(part: Part, label: string, cls: string, style: string, now: number, min: number, max: number, text: string, title: string)}
  <div class="grip {cls}" role="slider" tabindex="0" aria-label={label} aria-orientation="vertical" aria-valuemin={secs(min)} aria-valuemax={secs(max)} aria-valuenow={secs(now)} aria-valuetext={text} {title} {style} data-part={part} data-testid="opened-{part}" onmousedown={(e) => press(e, part)}></div>
{/snippet}

<svg class="wedge" aria-hidden="true"><path d={wedge} style="fill: rgb(var({tone}) / 0.08); stroke: rgb(var({tone}) / 0.45)" data-testid="opened-wedge" /></svg>

<div class="glass card" bind:this={card} role="dialog" aria-modal="false" aria-label="Opened sound: {name}" tabindex="-1" style="left: {box.left}px; top: {box.top}px; width: {box.width}px; height: {box.height}px; --c: var({tone})" data-testid="opened-sound" data-own-keys>
  <div class="hd">
    <span class="dot"></span>
    <span class="title ellip">{name}</span>
    <button class="close muted" onclick={() => onclose(true)} data-testid="opened-close"><span class="kb">Esc</span>close</button>
  </div>
  <div class="loud muted tn ellip" data-testid="opened-reading">{reading}</div>
  <span class="col muted" style="left: 14px; top: {HEAD - 16}px">Dialogue</span>
  <span class="col muted" style="left: {xl}px; top: {HEAD - 16}px">Sound</span>

  <svg class="view" style="top: {HEAD}px; height: {H}px" aria-hidden="true" data-testid="opened-view">
    {#each dialogue as d (d.who)}<path d={d.d} fill={d.fill} />{/each}
    <path d={whole} class="whole" data-testid="opened-source" />
    <path d={used} style="fill: rgb(var(--c))" data-testid="opened-used" />
    {#if u0 > s0}<rect x={xl} y={s0} width={xr - xl} height={u0 - s0} class="dim" />{/if}
    {#if s1 > u1}<rect x={xl} y={u1} width={xr - xl} height={s1 - u1} class="dim" />{/if}
    <path d={fadeLines} class="fades" />
    {#each ticks as t (t.t)}<path d="M{rulerX} {t.y.toFixed(1)}h8" class="tick" />{/each}
    {#if snapT != null}<path d="M{DIALOGUE_X - 14} {y(snapT).toFixed(1)}H{xr + 4}" class="snap" />{/if}
    {#if head >= 0 && head <= H}<path d="M12 {head.toFixed(1)}H{box.width - 12}" class="head" data-testid="opened-playhead" />{/if}
  </svg>
  <div class="over" style="top: {HEAD}px; height: {H}px">
    {#each ticks as t (t.t)}<span class="rl tn" style="left: {rulerX + 12}px; top: {t.y.toFixed(1)}px">{t.t}</span>{/each}
    {@render handle('in', 'Start of the sound', 'bar', `left: ${xl}px; width: ${xr - xl}px; top: ${(y(tIn) - 6).toFixed(1)}px`, s.start_s, Math.max(0, src0), tOut, `${s.start_s.toFixed(2)} seconds`, 'Trim the start')}
    {@render handle('move', 'Place of the sound, ⌥ arrows slip the audio inside', 'body', `left: ${xl}px; width: ${xr - xl}px; top: ${body.top.toFixed(1)}px; height: ${body.height.toFixed(1)}px`, s.start_s, 0, Math.max(s.start_s, app.view?.duration_s ?? 0), `Starts at ${s.start_s.toFixed(2)} seconds, ${s.in_s.toFixed(2)} into the source`, 'Drag to move · ⌥ drag to slip the sound inside')}
    {@render handle('out', 'End of the sound', 'bar', `left: ${xl}px; width: ${xr - xl}px; top: ${(y(tOut) - 6).toFixed(1)}px`, tOut, tIn, src1, `${tOut.toFixed(2)} seconds, ${s.len_s.toFixed(2)} long`, 'Trim the end')}
    {@render handle('fadeIn', 'Fade in', 'knob', `left: ${xr}px; top: ${Math.max(yFi, y(tIn) + 10).toFixed(1)}px`, tIn + s.fade_in_s, tIn, tOut, `${s.fade_in_s.toFixed(2)} seconds`, 'Fade in')}
    {@render handle('fadeOut', 'Fade out', 'knob', `left: ${xr}px; top: ${Math.min(yFo, y(tOut) - 10).toFixed(1)}px`, tOut - s.fade_out_s, tIn, tOut, `${s.fade_out_s.toFixed(2)} seconds${s.auto_fade ? ', automatic' : ''}`, 'Fade out')}
    <span class="lab tn" style="left: {xr + 12}px; top: {(y(tIn) - 22).toFixed(1)}px">In</span>
    <span class="lab tn" style="left: {xr + 12}px; top: {(y(tOut) + 6).toFixed(1)}px">Out · {s.len_s.toFixed(1)} s</span>
    {#if tip}<span class="tip tn" style="left: {xr + 14}px; top: {(tip.y - 12).toFixed(1)}px" data-testid="opened-tip">{tip.text}</span>{/if}
  </div>

  <div class="read muted tn ellip" data-testid="opened-readout">{readout(s)} · ⌥ drag slips</div>
  <div class="quick" role="group" aria-label="Quick lengths and fades">
    {#each QUICKS as q (q.kind)}
      <button class="sc" class:on={quickOn(q.kind, s, lines)} aria-pressed={quickOn(q.kind, s, lines)} onclick={() => quick(q.kind)} onmousedown={(e) => e.preventDefault()}>{q.label}</button>
    {/each}
    <button class="sc" class:on={s.auto_fade} aria-pressed={s.auto_fade} onclick={toggleAuto} onmousedown={(e) => e.preventDefault()}>Auto fade</button>
  </div>
</div>

<style>
  .wedge {
    position: absolute;
    left: 0;
    top: 0;
    width: 1px;
    height: 1px;
    overflow: visible;
    pointer-events: none;
    z-index: 8;
    animation: fade 0.3s ease;
  }
  .card {
    position: absolute;
    box-sizing: border-box;
    border-radius: 22px;
    background: var(--panel);
    box-shadow:
      inset 0 0 0 1px rgb(var(--c) / 0.35),
      0 30px 80px rgb(var(--shadow-rgb) / 0.55);
    outline: none;
    z-index: 9;
    animation: fade 0.3s ease;
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  .hd {
    position: absolute;
    left: 18px;
    right: 14px;
    top: 12px;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .dot {
    flex: none;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: radial-gradient(circle at 34% 30%, color-mix(in srgb, rgb(var(--c)) 50%, rgb(var(--highlight-rgb))), rgb(var(--c)) 60%);
  }
  .title {
    flex: 1;
    min-width: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--text-strong);
  }
  .close {
    flex: none;
    font-size: 11px;
    padding: 3px 6px;
    border-radius: 7px;
  }
  .close:hover {
    color: var(--text-strong);
  }
  .loud {
    position: absolute;
    left: 42px;
    right: 18px;
    top: 34px;
    font-size: 11.5px;
  }
  .col {
    position: absolute;
    font-size: 9.5px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .view {
    position: absolute;
    left: 0;
    width: 100%;
    overflow: hidden;
    pointer-events: none;
  }
  .whole {
    fill: rgb(var(--ink-rgb) / 0.15);
  }
  .dim {
    fill: var(--trim-shade);
  }
  .fades,
  .tick,
  .head,
  .snap {
    fill: none;
  }
  .fades {
    stroke: var(--playhead);
    stroke-width: 1.5;
  }
  .tick {
    stroke: rgb(var(--ink-rgb) / 0.3);
  }
  .head {
    stroke: var(--playhead);
    stroke-width: 1.5;
    opacity: 0.85;
  }
  .snap {
    stroke: var(--playhead);
    stroke-width: 2.5;
    stroke-linecap: round;
  }
  .over {
    position: absolute;
    left: 0;
    width: 100%;
  }
  .rl {
    position: absolute;
    transform: translateY(-50%);
    font-size: 10px;
    color: var(--text-4);
    pointer-events: none;
  }
  .grip {
    position: absolute;
    outline: none;
  }
  .grip:focus-visible {
    box-shadow: 0 0 0 2px var(--playhead);
  }
  .body {
    border-radius: 6px;
    cursor: grab;
  }
  .body:active {
    cursor: grabbing;
  }
  .bar {
    height: 12px;
    border-radius: 6px;
    cursor: ns-resize;
  }
  .bar::after {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    top: 5px;
    height: 2px;
    border-radius: 1px;
    background: var(--playhead);
  }
  .knob {
    width: 12px;
    height: 12px;
    margin: -6px 0 0 -6px;
    border-radius: 50%;
    background: var(--playhead);
    box-shadow: 0 0 0 2px var(--stage-bg);
    cursor: ns-resize;
  }
  .knob:focus-visible {
    box-shadow:
      0 0 0 2px var(--stage-bg),
      0 0 0 4px var(--playhead);
  }
  .lab {
    position: absolute;
    padding: 1px 6px;
    border-radius: 6px;
    font-size: 10.5px;
    font-weight: 600;
    white-space: nowrap;
    pointer-events: none;
    color: var(--stage-bg);
    background: var(--playhead);
  }
  .tip {
    position: absolute;
    max-width: 150px;
    padding: 4px 8px;
    border-radius: 8px;
    font-size: 11px;
    font-weight: 600;
    pointer-events: none;
    color: var(--playhead);
    background: var(--chip);
    box-shadow: inset 0 0 0 1px rgb(var(--ink-rgb) / 0.35);
    z-index: 2;
  }
  .read {
    position: absolute;
    left: 18px;
    right: 18px;
    bottom: 50px;
    font-size: 11.5px;
  }
  .quick {
    position: absolute;
    left: 14px;
    right: 14px;
    bottom: 14px;
    display: flex;
    flex-wrap: nowrap;
    gap: 6px;
  }
  .sc {
    flex: 1 1 auto;
    min-width: 0;
    padding: 5px 8px;
    border-radius: var(--radius-pill);
    font-size: 11.5px;
    white-space: nowrap;
    color: var(--text-2);
    background: var(--glass-fill-3);
  }
  .sc:hover {
    color: var(--text-strong);
  }
  .sc.on {
    color: var(--text-on-pill);
    background: rgb(var(--c));
  }
  .sc:focus-visible,
  .close:focus-visible {
    outline: 2px solid var(--playhead);
    outline-offset: 1px;
  }
  @media (prefers-reduced-motion: reduce) {
    .wedge,
    .card {
      animation: none;
    }
  }
</style>
