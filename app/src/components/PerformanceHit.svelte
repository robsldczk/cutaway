<script lang="ts">
  // One place a character said it: already cut to the sound, with the audio either side to trim
  // into. The edges drag, or step with the arrow keys; the row drags onto the timeline.
  import { app } from '../lib/app.svelte';
  import { grab } from '../lib/drag.svelte';
  import Icon from './Icon.svelte';
  import Waveform from './Waveform.svelte';
  import type { PerformanceHit } from '../lib/types';
  import { colorOf, solid } from '../theme';

  /** Shortest clip the edges leave. */
  const MIN_S = 0.05;
  const STEP_S = 0.01;
  const BIG_STEP_S = 0.1;

  let { hit }: { hit: PerformanceHit } = $props();

  let trim = $state<{ t0: number; t1: number } | null>(null);
  let strip = $state<HTMLElement>();
  const t0 = $derived(trim?.t0 ?? hit.t0);
  const t1 = $derived(trim?.t1 ?? hit.t1);
  const span = $derived(Math.max(hit.w1 - hit.w0, MIN_S));
  const pct = (t: number) => ((t - hit.w0) / span) * 100;
  /** To the hundredth: a grunt is under a second, and a trim moves it by less. */
  const secs = (s: number) => `${s.toFixed(2)} s`;
  const col = $derived(solid(colorOf(hit.color_index).rgb));
  const said = $derived(hit.text || hit.tag || 'Sound');
  const clip = $derived({ source_id: hit.source_id, t0: +t0.toFixed(3), t1: +t1.toFixed(3), text: said });

  /** The waveform of what's between the edges, for the preview player. */
  function face() {
    const n = hit.peaks.length;
    const part = hit.peaks.slice(Math.floor((pct(t0) / 100) * n), Math.ceil((pct(t1) / 100) * n));
    return { name: `${hit.name}: ${said}`, duration_s: t1 - t0, peaks: part };
  }

  const key = () => ({ clip: { ...clip, id: hit.id } });

  function setEdge(side: 'start' | 'end', t: number) {
    const start = side === 'start' ? Math.min(Math.max(t, hit.w0), t1 - MIN_S) : t0;
    const end = side === 'end' ? Math.max(Math.min(t, hit.w1), t0 + MIN_S) : t1;
    trim = { t0: start, t1: end };
  }

  function dragEdge(e: MouseEvent, side: 'start' | 'end') {
    if (e.button !== 0 || !strip) return;
    e.preventDefault();
    const box = strip.getBoundingClientRect();
    const playing = app.previewing(hit.id);
    const move = (ev: MouseEvent) => setEdge(side, hit.w0 + ((ev.clientX - box.left) / box.width) * span);
    const up = () => {
      window.removeEventListener('mousemove', move);
      window.removeEventListener('mouseup', up);
      // heard as it now stands
      if (playing) void app.playSound(key(), 0, face());
    };
    window.addEventListener('mousemove', move);
    window.addEventListener('mouseup', up);
  }

  function edgeKey(e: KeyboardEvent, side: 'start' | 'end') {
    const by = e.shiftKey ? BIG_STEP_S : STEP_S;
    const d = e.key === 'ArrowLeft' ? -by : e.key === 'ArrowRight' ? by : 0;
    if (!d) return;
    e.preventDefault();
    e.stopPropagation();
    setEdge(side, (side === 'start' ? t0 : t1) + d);
  }
</script>

{#snippet edge(side: 'start' | 'end', t: number)}
  <button class="edge" style="left: {pct(t)}%; color: {col}" role="slider" data-own-arrows aria-label="{side === 'start' ? 'Start' : 'End'} of {said}" aria-valuemin={hit.w0} aria-valuemax={hit.w1} aria-valuenow={+t.toFixed(2)} aria-valuetext={secs(t - hit.w0)} onmousedown={(e) => dragEdge(e, side)} onkeydown={(e) => edgeKey(e, side)} data-testid="clip-{side}"></button>
{/snippet}

<!-- svelte-ignore a11y_no_noninteractive_element_interactions (dragging is the mouse's; D82) -->
<div class="hit" role="listitem" onmousedown={(e) => grab(e, { kind: 'bucket', clip, character: hit.character, name: said, duration_s: t1 - t0 })} data-testid="performance-hit">
  <div class="top">
    <span class="grip"><Icon name="grip" /></span>
    <button class="play" style="background: {col}" aria-label="Play {said}" onmousedown={(e) => e.preventDefault()} onclick={() => app.toggleSound(key(), face())}><Icon name={app.previewing(hit.id) ? 'stop' : 'play'} size={10} /></button>
    <span class="said ellip"><span class="ctx">{hit.before ? `${hit.before} ` : ''}</span><b>{said}</b><span class="ctx">{hit.after ? ` ${hit.after}` : ''}</span></span>
    <span class="muted tn d" data-testid="clip-length">{secs(t1 - t0)}</span>
  </div>
  <div class="strip" bind:this={strip}>
    <Waveform peaks={hit.peaks} fill={col} />
    <span class="out" style="left: 0; width: {pct(t0)}%"></span>
    <span class="out" style="left: {pct(t1)}%; right: 0"></span>
    {@render edge('start', t0)}
    {@render edge('end', t1)}
  </div>
  <div class="at muted ellip">{hit.name} · {hit.at}{hit.close ? ' · sounds close' : ''}</div>
</div>

<style>
  .hit {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px;
    border-radius: 14px;
    background: var(--glass-fill-1);
    cursor: grab;
  }
  .top {
    display: grid;
    grid-template-columns: 12px 26px minmax(0, 1fr) auto;
    gap: 8px;
    align-items: center;
  }
  .grip {
    color: rgb(var(--ink-rgb) / 0.45);
    display: flex;
  }
  .play {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #16110d;
  }
  .said {
    font-size: 12.5px;
    min-width: 0;
  }
  .said b {
    font-weight: 600;
  }
  .ctx {
    color: var(--text-4);
    white-space: pre;
  }
  .d {
    font-size: 11px;
  }
  .strip {
    position: relative;
    height: 36px;
    margin: 0 5px;
  }
  .out {
    position: absolute;
    top: 0;
    bottom: 0;
    background: var(--bar-strong);
    opacity: 0.78;
    pointer-events: none;
  }
  .edge {
    position: absolute;
    top: -2px;
    bottom: -2px;
    width: 11px;
    margin-left: -5.5px;
    cursor: ew-resize;
  }
  .edge::after {
    content: '';
    position: absolute;
    left: 4px;
    top: 0;
    bottom: 0;
    width: 3px;
    border-radius: 2px;
    background: currentColor;
  }
  .edge:focus-visible {
    outline: 2px solid var(--text-strong);
    outline-offset: 1px;
    border-radius: 4px;
  }
  .at {
    font-size: 11px;
  }
</style>
