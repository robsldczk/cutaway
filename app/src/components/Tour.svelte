<script lang="ts">
  // The tour's spotlight and card. It only points and asks: every change comes from the page
  // underneath, through the bus, and app.endTour() puts it back.
  import { onMount } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { CHAPTERS, holeAround, nearestMiddle, placeCard, type Box } from '../lib/tour';

  const run = $derived(app.tour);
  const stops = $derived(run ? CHAPTERS[run.chapter].stops : []);
  const stop = $derived(stops[run?.i ?? 0]);
  const last = $derived((run?.i ?? 0) === stops.length - 1);

  let view = $state({ w: 1440, h: 900 });
  let hole = $state<Box | null>(null);
  let cardW = $state(340);
  let cardH = $state(180);
  /** A drag begun in the hole owns the pointer until it lets go: drop targets are found by point. */
  let held = $state(false);
  /** A note being tried: what the stop offered, so it's lit and the card keeps off it. */
  let drafting = $state(false);
  const live = $derived(!!stop?.live || drafting);
  let lit: Element | null = null;
  let cardEl = $state<HTMLDivElement>();

  const at = $derived(placeCard(hole, { w: cardW, h: cardH }, view));
  const blocks = $derived.by(() => {
    if (!hole) return [];
    const { x, y, w, h } = hole;
    return [
      { x: 0, y: 0, w: view.w, h: y },
      { x: 0, y: y + h, w: view.w, h: view.h - y - h },
      { x: 0, y, w: x, h },
      { x: x + w, y, w: view.w - x - w, h },
    ];
  });

  function light(el: Element | null) {
    if (el === lit) return;
    lit?.removeAttribute('data-tour-lit');
    el?.setAttribute('data-tour-lit', '');
    lit = el;
  }

  /** The target moves with the page, so it's found again every frame. */
  function measure() {
    drafting = !!document.querySelector('[data-tour="draft"]');
    const name = drafting ? 'draft' : stop?.target;
    view = view.w === innerWidth && view.h === innerHeight ? view : { w: innerWidth, h: innerHeight };
    let found: { el: Element; rect: Box } | null = null;
    if (name) {
      const items = [...document.querySelectorAll(`[data-tour="${name}"]`)].map((el) => {
        const r = el.getBoundingClientRect();
        return { el, rect: { x: r.x, y: r.y, w: r.width, h: r.height } };
      });
      found = nearestMiddle(items, view);
    }
    light(found?.el ?? null);
    const next = found ? holeAround(found.rect, view) : null;
    const same = next && hole && Math.abs(next.x - hole.x) < 0.5 && Math.abs(next.y - hole.y) < 0.5 && Math.abs(next.w - hole.w) < 0.5 && Math.abs(next.h - hole.h) < 0.5;
    if (!same && (next || hole)) hole = next;
  }

  onMount(() => {
    let frame = requestAnimationFrame(function tick() {
      measure();
      frame = requestAnimationFrame(tick);
    });
    const up = () => (held = false);
    window.addEventListener('mouseup', up, true);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener('mouseup', up, true);
      light(null);
    };
  });

  // the card has the focus, not its buttons: Space on a button would press it, not play
  $effect(() => {
    void stop;
    cardEl?.focus({ preventScroll: true });
  });

  // every change of the document is written down as it happens, for a quit to find
  $effect(() => {
    void app.view?.rev;
    void app.noteTourSteps();
  });

  function down(e: MouseEvent) {
    if (!live || !hole) return;
    const { x, y, w, h } = hole;
    if (e.clientX >= x && e.clientX <= x + w && e.clientY >= y && e.clientY <= y + h) held = true;
  }
</script>

<svelte:window onmousedowncapture={down} />

{#if run && stop}
  <div class="tour" class:held data-keeps-sound data-testid="tour">
    {#if hole}
      <div class="ring" style="left: {hole.x}px; top: {hole.y}px; width: {hole.w}px; height: {hole.h}px" data-testid="tour-hole"></div>
      {#each blocks as b, i (i)}<div class="block" style="left: {b.x}px; top: {b.y}px; width: {b.w}px; height: {b.h}px"></div>{/each}
      {#if !live}<div class="block" style="left: {hole.x}px; top: {hole.y}px; width: {hole.w}px; height: {hole.h}px"></div>{/if}
    {:else}
      <div class="block dim"></div>
    {/if}
    {#if !run.ending}
    <div class="glass card" bind:this={cardEl} bind:clientWidth={cardW} bind:clientHeight={cardH} style="left: {at.x}px; top: {at.y}px" role="dialog" aria-label="Tour of {CHAPTERS[run.chapter].name}" aria-live="polite" tabindex="-1" data-testid="tour-card">
      <div class="top"><span class="nm chapter">{CHAPTERS[run.chapter].name}</span><span class="tn pos" data-testid="tour-pos">{run.i + 1} / {stops.length}</span></div>
      <h2>{stop.title}</h2>
      <p>{stop.text}</p>
      {#if stop.tryIt}<p class="try" data-testid="tour-try"><b>Try it</b> {stop.tryIt}</p>{/if}
      <div class="foot">
        <button class="plain" onclick={() => app.endTour()} data-testid="tour-skip">{last ? 'Close' : 'Skip'}</button>
        <span class="grow"></span>
        {#if run.i > 0}<button class="plain" onclick={() => app.stepTour(-1)} data-testid="tour-back">Back</button>{/if}
        <button class="next" onclick={() => app.stepTour(1)} data-testid="tour-next">{last ? 'Done' : 'Next'}</button>
      </div>
    </div>
    {/if}
  </div>
{/if}

<style>
  .tour {
    position: fixed;
    inset: 0;
    z-index: 55;
    pointer-events: none;
  }
  .ring {
    position: absolute;
    border-radius: 14px;
    box-shadow:
      0 0 0 2px rgb(var(--accent-rgb) / 0.9),
      0 0 0 200vmax var(--scrim-light);
    transition:
      left 0.22s var(--ease-out),
      top 0.22s var(--ease-out),
      width 0.22s var(--ease-out),
      height 0.22s var(--ease-out);
  }
  .block {
    position: absolute;
    pointer-events: auto;
  }
  .block.dim {
    inset: 0;
    background: var(--scrim-light);
  }
  .held .block {
    pointer-events: none;
  }
  .card {
    position: absolute;
    width: min(340px, calc(100vw - 24px));
    box-sizing: border-box;
    padding: 18px 20px 14px;
    border-radius: var(--radius-sheet);
    background: var(--panel-solid);
    font-size: var(--fs-ui);
    pointer-events: auto;
    outline: none;
    transition:
      left 0.22s var(--ease-out),
      top 0.22s var(--ease-out);
  }
  .top {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    color: var(--text-4);
  }
  .chapter {
    font-size: 11px;
  }
  .pos {
    font-size: 12px;
  }
  h2 {
    margin: 8px 0 6px;
    font-size: 17px;
    font-weight: 700;
    color: var(--text-1);
  }
  p {
    margin: 0;
    line-height: 1.45;
    color: var(--text-2);
  }
  .try {
    margin-top: 10px;
    color: var(--text-3);
  }
  .try b {
    color: var(--link);
    font-weight: 600;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 14px;
  }
  .grow {
    flex: 1;
  }
  .foot button {
    height: 32px;
    padding: 0 14px;
    border-radius: var(--radius-pill);
    font-weight: 600;
  }
  .plain {
    color: var(--text-3);
  }
  .plain:hover {
    background: var(--glass-hover);
  }
  .next {
    background: var(--text-1);
    color: var(--text-on-light);
  }
  @media (prefers-reduced-motion: reduce) {
    .ring,
    .card {
      transition: none;
    }
  }
</style>
