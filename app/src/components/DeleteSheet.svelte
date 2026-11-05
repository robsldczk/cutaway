<script lang="ts">
  // The question before a delete: this take, or the whole line? The smaller one has the focus,
  // so Enter says that, and Esc says neither.
  import { onMount } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { charLabel } from '../lib/colors';
  import Sheet from './Sheet.svelte';

  let { lineId, takeId = null, onclose, ondone }: { lineId: string; takeId?: string | null; onclose: () => void; ondone?: () => void } = $props();

  const it = $derived(app.view?.items.find((i) => i.line_id === lineId));
  const at = $derived(it?.takes.findIndex((t) => t.id === (takeId ?? it.take_id)) ?? -1);
  const take = $derived(at >= 0 ? it?.takes[at] : undefined);
  const noun = $derived(it?.kind === 'spacer' ? 'direction' : 'line');
  let first = $state<HTMLButtonElement>();

  // closing can take the props away: what to delete is read first
  async function dropTake() {
    const id = take?.id;
    onclose();
    if (id) await app.deleteTake(id);
    ondone?.();
  }

  async function dropLine() {
    const id = lineId;
    onclose();
    await app.deleteLine(id);
    ondone?.();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') onclose();
    else if (e.key === 'Enter') (document.activeElement as HTMLElement | null)?.click();
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  onMount(() => {
    first?.focus();
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  });
</script>

<Sheet title={take ? 'Delete the take or the line?' : `Delete this ${noun}?`} width={480} {onclose}>
  {#if it}
    <p class="q">{#if it.character}<span class="who">{charLabel(app.view, it.character)}</span>{/if}{take?.words.map((w) => w.text).join(' ') || it.text}</p>
  {/if}
  <p class="muted note">
    {#if take}Delete take: the line keeps its other takes.{/if}
    Delete {noun}: it leaves the cut. The script and the recordings stay as they are, and <span class="kb">⌘Z</span> undoes either.
  </p>
  <div class="row">
    <button class="no" onclick={onclose}>Cancel</button>
    {#if take}
      <button class="no" onclick={dropLine} data-testid="delete-line-yes">Delete {noun}</button>
      <button class="yes" bind:this={first} onclick={dropTake} data-testid="delete-take-yes">Delete take {at + 1}</button>
    {:else}
      <button class="yes" bind:this={first} onclick={dropLine} data-testid="delete-line-yes">Delete {noun}</button>
    {/if}
  </div>
</Sheet>

<style>
  .q {
    margin: 0 0 10px;
    line-height: 1.45;
    color: var(--text-1);
  }
  .who {
    display: block;
    font-size: 11px;
    font-weight: 700;
    color: var(--text-3);
  }
  .note {
    margin: 0 0 16px;
    font-size: 12.5px;
  }
  .row {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .no,
  .yes {
    height: 36px;
    padding: 0 18px;
    border-radius: var(--radius-pill);
    font-size: var(--fs-ui);
  }
  .no {
    color: var(--text-2);
    background: var(--glass-fill-3);
  }
  .yes {
    font-weight: 600;
    background: var(--button-light);
    color: var(--text-on-light);
  }
  .no:focus-visible,
  .yes:focus-visible {
    outline: 2px solid var(--text-strong);
    outline-offset: 2px;
  }
</style>
