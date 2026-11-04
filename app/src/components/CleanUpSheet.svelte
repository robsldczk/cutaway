<script lang="ts">
  // The review step of Clean up: moving a file changes the shared pipeline for every machine
  // and episode, so nothing moves until the list is confirmed here.
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import Sheet from './Sheet.svelte';
  import type { CleanupPlan } from '../lib/types';

  let { plan, onclose, ondone }: { plan: CleanupPlan; onclose: () => void; ondone: () => void } = $props();

  let off = $state<Record<string, boolean>>({});
  let moving = $state(false);
  const chosen = $derived(plan.moves.filter((m) => !off[m.file]));
  const groups = $derived.by(() => {
    const by: Record<string, CleanupPlan['moves']> = {};
    for (const m of plan.moves) (by[m.to] ??= []).push(m);
    return Object.entries(by).sort(([a], [b]) => a.localeCompare(b));
  });

  async function move() {
    moving = true;
    const r = await app.run(() => call<{ moved: number; failed: { file: string; error: string }[] }>('apply_cleanup', { moves: chosen.map((m) => ({ file: m.file, to: m.to })) }));
    moving = false;
    if (!r) return;
    const failed = r.failed.length ? ` · ${r.failed.length} couldn't move: ${r.failed[0].error}` : '';
    app.say(`${r.moved} sound${r.moved === 1 ? '' : 's'} moved${failed}`);
    ondone();
  }
</script>

<Sheet title="Clean up loose sounds" width={640} {onclose}>
  {#if plan.moves.length}
    <p class="muted lead">Claude read the file names. Moved files change in the shared folder, for every machine and episode. Untick what should stay.</p>
    <div class="list">
      {#each groups as [to, moves] (to)}
        <section aria-label={to}>
          <div class="to">Sound/{to}{#if moves[0].new_folder}<span class="chip outline new">new folder</span>{/if}</div>
          {#each moves as m (m.file)}
            <label class="row">
              <input type="checkbox" checked={!off[m.file]} onchange={(e) => (off = { ...off, [m.file]: !e.currentTarget.checked })} aria-label="Move {m.name}" />
              <span class="ellip sound" title={m.name}>{m.name}</span>
              <span class="muted ellip why">{m.reason}</span>
            </label>
          {/each}
        </section>
      {/each}
    </div>
  {:else}
    <p class="muted lead">Claude found no folder it's sure about for these sounds.</p>
  {/if}
  {#if plan.stay.length}<p class="muted stay">{plan.stay.length} sound{plan.stay.length === 1 ? '' : 's'} stay{plan.stay.length === 1 ? 's' : ''} loose: no clear folder.</p>{/if}
  <div class="acts">
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn primary" disabled={!chosen.length || moving} onclick={move}>{moving ? 'Moving…' : `Move ${chosen.length} file${chosen.length === 1 ? '' : 's'}`}</button>
  </div>
</Sheet>

<style>
  .lead {
    margin: 0 0 12px;
    font-size: 12.5px;
    line-height: 1.45;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-height: 52vh;
    overflow-y: auto;
  }
  .to {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    font-weight: 600;
    margin-bottom: 4px;
  }
  .new {
    font-size: 10.5px;
    font-weight: 500;
  }
  .row {
    display: grid;
    grid-template-columns: 18px minmax(0, 3fr) minmax(0, 2fr);
    align-items: center;
    gap: 8px;
    min-height: 28px;
    font-size: 12px;
    cursor: pointer;
  }
  .why {
    font-size: 11.5px;
  }
  .stay {
    margin: 12px 0 0;
    font-size: 12px;
  }
  .acts {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 14px;
  }
</style>
