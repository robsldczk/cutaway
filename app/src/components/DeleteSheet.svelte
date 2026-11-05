<script lang="ts">
  // "Do you really want to delete this?" The button has the focus: Enter says yes, Esc no.
  import { onMount } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { charLabel } from '../lib/colors';
  import Sheet from './Sheet.svelte';

  const it = app.view?.items.find((i) => i.line_id === app.deleting);
  let yes = $state<HTMLButtonElement>();
  const close = () => {
    app.overlay = null;
    app.deleting = null;
  };

  onMount(() => yes?.focus());
</script>

<Sheet title="Delete this {it?.kind === 'spacer' ? 'direction' : 'line'}?" width={460} onclose={close}>
  {#if it}
    <p class="q">{#if it.character}<span class="who">{charLabel(app.view, it.character)}</span>{/if}{it.text}</p>
  {/if}
  <p class="muted note">It leaves the cut. The script and the recordings stay as they are, and <span class="kb">⌘Z</span> brings it back.</p>
  <div class="row">
    <button class="no" onclick={close}>Cancel</button>
    <button class="yes" bind:this={yes} onclick={() => app.deleteLine()} data-testid="delete-line-yes">Delete</button>
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
  .yes:focus-visible {
    outline: 2px solid var(--text-strong);
    outline-offset: 2px;
  }
</style>
