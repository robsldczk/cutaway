<script lang="ts">
  // The Range's boxes: a list to tick, with All, None, Invert and the scene marks above it.
  // A click ticks one box and leaves the rest. ⇧ takes the run from the last box touched, and a
  // press sweeps: every box the pointer crosses becomes what the first one became.
  import { between, inverted, tick, tickable, withStatus, clock, historyTip, type ExportScene } from '../lib/export';
  import { STATUSES, statusOf } from '../lib/rail';
  import type { SceneStatus } from '../lib/types';
  import Icon from './Icon.svelte';

  let { scenes, picked, onpick, label = 'Scenes' }: { scenes: ExportScene[]; picked: string[]; onpick: (next: string[]) => void; label?: string } = $props();

  const open = $derived(tickable(scenes));
  const allOn = $derived(open.length > 0 && open.every((s) => picked.includes(s.id)));
  /** The box a ⇧ click runs from. */
  let anchor = $state<string | null>(null);
  /** What a press in progress makes of the boxes it crosses. */
  let sweep = $state<boolean | null>(null);

  function press(e: PointerEvent, id: string) {
    // on a Mac a control click is a right click
    if (e.button !== 0 || e.ctrlKey) return;
    const on = !picked.includes(id);
    onpick(tick(scenes, picked, e.shiftKey && anchor ? between(open, anchor, id) : [id], on));
    anchor = id;
    sweep = on;
  }

  function cross(id: string) {
    if (sweep === null) return;
    onpick(tick(scenes, picked, [id], sweep));
    anchor = id;
  }

  /** Space or Enter on a focused box; a press has already done the pointer's click. */
  function keyed(e: MouseEvent, id: string) {
    if (e.detail !== 0) return;
    onpick(tick(scenes, picked, [id], !picked.includes(id)));
    anchor = id;
  }

  /** A mark takes the scenes marked so; with ⌘ or ⇧ it adds them to the ticks, or takes them off. */
  function byStatus(e: MouseEvent, status: SceneStatus, on: boolean) {
    const ids = withStatus(scenes, status);
    onpick(e.metaKey || e.ctrlKey || e.shiftKey ? tick(scenes, picked, ids, !on) : ids);
  }

  const tip = (s: ExportScene) => [s.heading, statusOf(s.status)?.label, historyTip(s)].filter(Boolean).join('\n');
</script>

<svelte:window onpointerup={() => (sweep = null)} onpointercancel={() => (sweep = null)} />

<div class="tools" role="group" aria-label="Select {label}">
  <button class="tool" disabled={allOn || !open.length} onclick={() => onpick(open.map((s) => s.id))} data-testid="select-all">All</button>
  <button class="tool" disabled={!picked.length} onclick={() => onpick([])} data-testid="select-none">None</button>
  <button class="tool" disabled={!open.length} onclick={() => onpick(inverted(scenes, picked))} data-testid="select-invert">Invert</button>
</div>
<div class="tools" role="group" aria-label="Select {label} by mark">
  {#each STATUSES as t (t.id)}
    {@const ids = withStatus(scenes, t.id)}
    {@const on = ids.length > 0 && ids.every((id) => picked.includes(id))}
    <button class="tool mark" class:on aria-pressed={on} disabled={!ids.length} title="Tick the scenes marked {t.label}. ⌘ click adds them to the ticks." onclick={(e) => byStatus(e, t.id, on)} data-testid="select-{t.id}">
      <span class="dot" style="background: rgb(var({t.tone}))"></span>{t.label}<span class="n tn">{ids.length}</span>
    </button>
  {/each}
</div>
<div class="boxes" role="group" aria-label={label} data-own-keys>
  {#each scenes as s (s.id)}
    {@const on = picked.includes(s.id)}
    {@const mark = statusOf(s.status)}
    <button class="box" class:on role="checkbox" aria-checked={on} aria-label="Scene {s.number}, {s.place}" disabled={!s.has_audio} title={s.has_audio ? tip(s) : 'Nothing in this scene yet'} onpointerdown={(e) => press(e, s.id)} onpointerenter={() => cross(s.id)} onclick={(e) => keyed(e, s.id)} data-testid="scene-box" data-number={s.number}>
      <span class="tickbox" aria-hidden="true"></span>
      <span class="num tn">{s.number}</span>
      <span class="place">{s.place}</span>
      {#if s.lock}<span class="lk"><Icon name="lock" size={10} /></span>{/if}
      {#if mark}<span class="dot" style="background: rgb(var({mark.tone}))" title={mark.label}></span>{/if}
      <span class="len tn">{s.has_audio ? clock(s.length_s) : 'empty'}</span>
    </button>
  {/each}
</div>

<style>
  .tools {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    margin-bottom: 6px;
  }
  .tool {
    height: 24px;
    padding: 0 9px;
    border-radius: 7px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
    background: var(--glass-hover);
  }
  .tool:hover:not(:disabled) {
    color: var(--text-strong);
  }
  .tool:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .tool:focus-visible,
  .box:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .mark.on {
    color: var(--text-strong);
    box-shadow: inset 0 0 0 1px rgb(var(--veil-rgb) / 0.3);
  }
  .n {
    color: var(--text-5);
  }
  .dot {
    flex: none;
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  /* as high as the window can give it, up to some 16 scenes: the buttons above it stay */
  .boxes {
    max-height: clamp(160px, calc(100vh - 470px), 430px);
    overflow-y: auto;
    border-radius: 9px;
    background: var(--glass-fill-3);
    padding: 3px;
    user-select: none;
    -webkit-user-select: none;
  }
  .box {
    width: 100%;
    height: 26px;
    padding: 0 8px 0 6px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--text-3);
    text-align: left;
    touch-action: none;
  }
  .box:hover:not(:disabled) {
    background: var(--glass-hover);
  }
  .box.on {
    color: var(--text-strong);
  }
  .box:disabled {
    opacity: 0.5;
    cursor: default;
  }
  /* the sheet's own tick, on a button so a click carries its modifiers */
  .tickbox {
    flex: none;
    width: 16px;
    height: 16px;
    border-radius: 5px;
    box-shadow: inset 0 0 0 1.5px rgb(var(--veil-rgb) / 0.3);
    background: center / 11px no-repeat;
  }
  .box.on .tickbox {
    box-shadow: none;
    background-color: var(--accent);
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='%2316131F' stroke-width='3.6' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M5 12l5 5 9-10'/%3E%3C/svg%3E");
  }
  .num {
    flex: none;
    min-width: 15px;
    text-align: right;
    color: var(--text-4);
  }
  .place {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .lk {
    flex: none;
    display: flex;
    color: var(--text-4);
  }
  .len {
    flex: none;
    font-size: 11.5px;
    color: var(--text-4);
  }
</style>
