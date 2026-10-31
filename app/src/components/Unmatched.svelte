<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { fmt } from '../lib/format';
  import Drawer from './Drawer.svelte';
  const items = $derived(app.view?.unmatched ?? []);

  async function use(id: string) {
    const v = app.view;
    const c = app.cur;
    if (!v) return;
    const idx = c ? v.items.findIndex((i) => i.line_id === c.line_id) + 1 : v.items.length;
    await app.cmd({ cmd: 'use_unmatched', unmatched_id: id, scene_id: v.scene.id, index: idx });
    app.say('Added after the current line');
  }
</script>

<Drawer title="Unmatched bin">
  <p class="muted">Speech on the role tracks that matches no line: ad-libs, questions, chatter. Use one to put it after the current line.</p>
  {#each items as u (u.id)}
    <div class="row" class:used={u.used}>
      <div class="txt"><span class="t">{fmt(u.t0)}</span> {u.text}{#if u.origin === 'deleted_line'} <span class="badge">from a deleted line</span>{/if}</div>
      <button class="btn" disabled={u.used} onclick={() => use(u.id)}>{u.used ? 'Used' : 'Use'}</button>
    </div>
  {:else}
    <p class="muted">Nothing unmatched in this scene.</p>
  {/each}
</Drawer>

<style>
  .row {
    display: flex;
    gap: 10px;
    align-items: center;
    justify-content: space-between;
    padding: 8px 0;
    border-bottom: 1px solid var(--glass-outline);
  }
  .row.used {
    opacity: 0.5;
  }
  .txt {
    color: var(--text-2);
    line-height: 1.4;
  }
  .t {
    color: var(--text-4);
    font-variant-numeric: tabular-nums;
  }
  .badge {
    box-shadow: inset 0 0 0 1px var(--text-5);
    color: var(--text-3);
  }
  .btn {
    height: 28px;
    flex: none;
  }
</style>
