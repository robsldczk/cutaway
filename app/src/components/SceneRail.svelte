<script lang="ts">
  import { app } from '../lib/app.svelte';
  let open = $state(false);
  const scenes = $derived(app.view?.scenes ?? []);
  // an act label above the first scene of each act (only in the open list)
  const actStart = (i: number) => !!scenes[i].act && (i === 0 || scenes[i - 1].act !== scenes[i].act);
  const tip = (s: (typeof scenes)[number]) => [s.title, s.heading, s.synopsis].filter(Boolean).join('\n');
</script>

<nav class="glass rail" aria-label="Scenes" class:open>
  <button class="toggle" onclick={() => (open = !open)} aria-label={open ? 'Collapse scene list' : 'Expand scene list'} title="Scene list">
    {open ? '‹' : '›'}
  </button>
  {#each scenes as s, i (s.id)}
    {#if open && actStart(i)}<div class="act ellip">{s.act}</div>{/if}
    <button
      class="sc"
      class:cur={s.id === app.sceneId}
      aria-label="Scene {s.number}{s.title ? `, ${s.title}` : ''}{s.id === app.sceneId ? ', current' : ''}{s.flagged ? `, ${s.flagged} flagged` : ''}"
      title={tip(s)}
      onclick={() => app.loadScene(s.id)}
    >
      <span class="num">{s.number}</span>
      {#if open}
        <span class="info"><span class="ellip h">{s.title || s.place || s.heading}</span><span class="st ellip">{s.title && s.place ? `${s.place} · ` : ''}{s.lines_with_takes}/{s.lines} lines{s.flagged ? ` · ${s.flagged} flagged` : ''}</span></span>
      {/if}
      {#if s.flagged}<span class="fl"></span>{/if}
    </button>
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
