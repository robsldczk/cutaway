<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { historyTip } from '../lib/export';
  import Icon from './Icon.svelte';
  let open = $state(false);
  const scenes = $derived(app.view?.scenes ?? []);
  // an act label above the first scene of each act (only in the open list)
  const actStart = (i: number) => !!scenes[i].act && (i === 0 || scenes[i - 1].act !== scenes[i].act);
  const history = (id: string) => {
    const e = app.exported[id];
    return e?.history.length ? historyTip(e) : '';
  };
  const tip = (s: (typeof scenes)[number]) => [s.title, s.heading, s.synopsis, history(s.id)].filter(Boolean).join('\n');
</script>

<nav class="glass rail" aria-label="Scenes" class:open>
  <button class="toggle" onclick={() => (open = !open)} aria-label={open ? 'Collapse scene list' : 'Expand scene list'} title="Scene list">
    {open ? '‹' : '›'}
  </button>
  {#each scenes as s, i (s.id)}
    {#if open && actStart(i)}<div class="act ellip">{s.act}</div>{/if}
    <div class="row">
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
</nav>

<style>
  .rail {
    position: absolute;
    left: 20px;
    top: 50%;
    transform: translateY(-50%);
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
    border-radius: var(--radius-rail);
    background: var(--glass-fill-2);
    z-index: 12;
    max-height: 70vh;
    overflow-y: auto;
  }
  .rail.open {
    background: var(--panel-solid);
    width: 260px;
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
    height: 40px;
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
    top: 8px;
    left: 30px;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--warn);
  }
</style>
