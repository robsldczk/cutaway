<script lang="ts">
  // The scenes, always in reach. There's no scrollbar: the rail sits between the header and the
  // transport and tightens to fit that height. Past the smallest size that reads it pages, with
  // steppers at its ends that are only there while something is out of sight.
  import { tick } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { historyTip } from '../lib/export';
  import { CELL_MIN, railFit } from '../lib/rail';
  import Icon from './Icon.svelte';
  let open = $state(false);
  /** The room between the header and the transport. */
  let slotH = $state(0);
  let list = $state<HTMLDivElement>();
  /** Scenes out of sight above and below. */
  let above = $state(false);
  let below = $state(false);
  // the toggle, the rail's padding and its gaps
  const CHROME = 50;
  /** An open row has two lines to fit. */
  const OPEN_MIN = 38;
  const scenes = $derived(app.view?.scenes ?? []);
  // an act label above the first scene of each act (only in the open list)
  const actStart = (i: number) => !!scenes[i].act && (i === 0 || scenes[i - 1].act !== scenes[i].act);
  const history = (id: string) => {
    const e = app.exported[id];
    return e?.history.length ? historyTip(e) : '';
  };
  const tip = (s: (typeof scenes)[number]) => [s.title, s.heading, s.synopsis, history(s.id)].filter(Boolean).join('\n');
  // no scrollbar: the numbers tighten until they all show
  const size = $derived(railFit(scenes.length, slotH - CHROME, open ? OPEN_MIN : CELL_MIN));

  function edges() {
    if (!list) return;
    above = list.scrollTop > 2;
    below = list.scrollTop + list.clientHeight < list.scrollHeight - 2;
  }

  function step(by: 1 | -1) {
    list?.scrollBy({ top: by * list.clientHeight * 0.8, behavior: 'smooth' });
  }

  // the current scene stays in sight, and the steppers follow what's hidden
  $effect(() => {
    void size;
    void open;
    const id = app.sceneId;
    void tick().then(() => {
      list?.querySelector<HTMLElement>(`[data-scene="${CSS.escape(id)}"]`)?.scrollIntoView({ block: 'nearest' });
      edges();
    });
  });
</script>

<div class="slot" bind:clientHeight={slotH}>
<nav class="glass rail" aria-label="Scenes" class:open style="--cell: {size.cell}px; --cell-gap: {size.gap}px">
  <button class="toggle" onclick={() => (open = !open)} aria-label={open ? 'Collapse scene list' : 'Expand scene list'} title="Scene list">
    {open ? '‹' : '›'}
  </button>
  {#if above}<button class="stepper" aria-label="Earlier scenes" title="Earlier scenes" onclick={() => step(-1)} data-testid="rail-up"><Icon name="chevron-up" size={13} stroke={2.4} /></button>{/if}
  <div class="list" class:above class:below bind:this={list} onscroll={edges}>
  {#each scenes as s, i (s.id)}
    {#if open && actStart(i)}<div class="act ellip">{s.act}</div>{/if}
    <div class="row" data-scene={s.id}>
    <button
      class="sc"
      class:cur={s.id === app.sceneId}
      aria-label="Scene {s.number}{s.title ? `, ${s.title}` : ''}{s.id === app.sceneId ? ', current' : ''}{s.flagged ? `, ${s.flagged} flagged` : ''}{s.lock ? ', locked' : ''}"
      title={tip(s)}
      onclick={() => app.loadScene(s.id)}
    >
      <span class="num">{s.number}</span>
      {#if open}
        <span class="info"><span class="ellip h">{s.title || s.place || s.heading}</span><span class="st ellip">{s.title && s.place ? `${s.place} · ` : ''}{s.lines_with_takes}/{s.lines} lines{s.flagged ? ` · ${s.flagged} flagged` : ''}</span></span>
      {/if}
      {#if s.flagged}<span class="fl"></span>{/if}
    </button>
    {#if s.lock || open}
      <button class="lk" class:on={!!s.lock} aria-label={s.lock ? `Unlock scene ${s.number}` : `Lock scene ${s.number}`} aria-pressed={!!s.lock} title={s.lock ? 'Unlock this scene' : "Lock this scene's timing"} onclick={() => (s.lock ? (app.unlocking = s.id) : app.lockScene(s.id))} data-testid="rail-lock"><Icon name={s.lock ? 'lock' : 'unlock'} size={11} /></button>
    {/if}
    </div>
  {/each}
  </div>
  {#if below}<button class="stepper" aria-label="Later scenes" title="Later scenes" onclick={() => step(1)} data-testid="rail-down"><Icon name="chevron" size={13} stroke={2.4} /></button>{/if}
</nav>
</div>

<style>
  /* between the header and the transport, centred in what's there */
  .slot {
    position: absolute;
    left: 20px;
    top: 84px;
    bottom: 116px;
    display: flex;
    flex-direction: column;
    justify-content: center;
    pointer-events: none;
    z-index: 12;
  }
  .rail {
    box-sizing: border-box;
    max-height: 100%;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
    border-radius: var(--radius-rail);
    background: var(--glass-fill-2);
    pointer-events: auto;
  }
  .list {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: var(--cell-gap);
    overflow-y: auto;
    scrollbar-width: none;
    overscroll-behavior: contain;
  }
  .list::-webkit-scrollbar {
    display: none;
  }
  /* what's cut off fades out, so it reads as more, not as an edge */
  .list.above {
    -webkit-mask-image: linear-gradient(to bottom, transparent, #000 18px);
    mask-image: linear-gradient(to bottom, transparent, #000 18px);
  }
  .list.below {
    -webkit-mask-image: linear-gradient(to top, transparent, #000 18px);
    mask-image: linear-gradient(to top, transparent, #000 18px);
  }
  .list.above.below {
    -webkit-mask-image: linear-gradient(to bottom, transparent, #000 18px, #000 calc(100% - 18px), transparent);
    mask-image: linear-gradient(to bottom, transparent, #000 18px, #000 calc(100% - 18px), transparent);
  }
  .stepper {
    flex: none;
    height: 18px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 7px;
    color: var(--text-4);
  }
  .stepper:hover {
    background: var(--glass-hover);
    color: var(--text-1);
  }
  .rail.open {
    background: var(--panel-solid);
    width: 260px;
  }
  .rail.open .fl {
    top: 8px;
  }
  .toggle {
    height: 22px;
    font-size: 14px;
    color: var(--text-4);
    border-radius: 8px;
  }
  .toggle:hover {
    background: var(--glass-hover);
  }
  .row {
    position: relative;
  }
  .row .sc {
    width: 100%;
  }
  .lk {
    position: absolute;
    right: 2px;
    bottom: 2px;
    width: 18px;
    height: 18px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
    color: var(--text-5);
  }
  .rail.open .lk {
    right: 8px;
    bottom: 11px;
  }
  .lk.on {
    color: var(--text-2);
  }
  .lk:hover {
    background: var(--glass-hover);
    color: var(--text-1);
  }
  .sc {
    position: relative;
    min-width: 40px;
    height: var(--cell);
    border-radius: var(--radius-btn);
    font-size: var(--fs-ui);
    color: var(--text-4);
    display: flex;
    align-items: center;
    gap: 10px;
    text-align: left;
  }
  .sc:hover {
    background: var(--glass-fill-4);
  }
  .sc.cur {
    background: var(--glass-active);
    color: var(--text-1);
    font-weight: 600;
  }
  .num {
    width: 40px;
    flex: none;
    text-align: center;
  }
  .info {
    display: flex;
    flex-direction: column;
    min-width: 0;
    padding-right: 10px;
  }
  .h {
    font-size: 12px;
    color: var(--text-2);
  }
  .st {
    font-size: 10.5px;
    color: var(--text-4);
    font-weight: 400;
  }
  .act {
    margin: 6px 10px 0;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: var(--name-tracking);
    text-transform: uppercase;
    color: var(--text-4);
  }
  .fl {
    position: absolute;
    top: 4px;
    left: 30px;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--warn);
  }
</style>
