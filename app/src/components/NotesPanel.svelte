<script lang="ts">
  // The Notes tab of the scene sounds panel: this scene's share of the one feedback set, from the
  // same fold the review screen reads, so a resolve on either side shows on the other at once.
  import { tick } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import { filtered, threadsOf, type Note, type NoteFilter } from '../lib/review';
  import NoteCard from './NoteCard.svelte';

  let { onbucket }: { onbucket: (n: Note) => void } = $props();

  let notes = $state<Note[]>([]);
  let by = $state<NoteFilter>('open');
  let list = $state<HTMLDivElement>();

  async function load() {
    if (!app.sceneId) return;
    const r = await app.run(() => call<{ threads: { notes: Note[] }[] }>('list_notes', { scene: app.sceneId }));
    if (r) notes = r.threads.flatMap((t) => t.notes);
  }

  $effect(() => {
    void app.sceneId;
    void app.status?.rev;
    void app.view?.rev;
    if (!app.reviewer) void app.loadReviewer();
    void load();
  });

  const shown = $derived(threadsOf(filtered(notes, by, null)));
  const open = $derived(notes.filter((n) => n.status !== 'resolved').length);

  function jump(n: Note) {
    if (n.line_id) app.select(n.line_id);
    if (n.start_s !== null) app.seek(n.start_s);
  }

  // a marker on the page brings its note into view
  $effect(() => {
    const id = app.noteFocus;
    if (!id) return;
    if (notes.some((n) => n.id === id && n.status === 'resolved')) by = 'all';
    void tick().then(() => {
      list?.querySelector(`[data-note="${CSS.escape(id)}"]`)?.scrollIntoView({ block: 'nearest' });
      setTimeout(() => {
        if (app.noteFocus === id) app.noteFocus = null;
      }, 1800);
    });
  });
</script>

<div class="np" data-testid="notes-panel">
  <div class="seg" role="radiogroup" aria-label="Show">
    {#each [['open', `Open ${open}`], ['all', `All ${notes.length}`]] as [id, label] (id)}
      <button class:on={by === id} role="radio" aria-checked={by === id} onclick={() => (by = id as NoteFilter)} data-testid="notes-{id}">{label}</button>
    {/each}
    <span class="grow"></span>
    <button class="hist" onclick={() => (app.overlay = 'review-history')}>History</button>
  </div>
  <div class="list" bind:this={list}>
    {#each shown as t (t[0].thread)}
      <div class="th" class:hot={t.some((n) => n.id === app.noteFocus)} data-testid="panel-thread">
        {#each t as n, i (n.id)}
          {#if i}<div class="sep"></div>{/if}
          <NoteCard note={n} compact onjump={jump} {onbucket} />
        {/each}
      </div>
    {:else}
      <p class="muted none">{notes.length ? 'Nothing open here.' : 'No notes on this scene yet. The Export page opens scenes for review.'}</p>
    {/each}
  </div>
</div>

<style>
  .np {
    display: flex;
    flex-direction: column;
    gap: 10px;
    flex: 1;
    min-height: 0;
  }
  .seg {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .seg button {
    height: 24px;
    padding: 0 10px;
    border-radius: 7px;
    font-size: 12px;
    color: var(--text-3);
  }
  .seg button.on {
    background: var(--glass-active);
    color: var(--text-strong);
    font-weight: 600;
  }
  .seg .hist {
    font-size: 11.5px;
    color: var(--text-4);
  }
  .grow {
    flex: 1;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .th {
    padding: 10px 11px;
    border-radius: 13px;
    background: var(--glass-fill-2);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .th.hot {
    box-shadow: inset 0 0 0 1.5px rgb(var(--accent-rgb) / 0.8);
  }
  .sep {
    height: 1px;
    background: var(--glass-outline);
  }
  .none {
    font-size: 12.5px;
  }
</style>
