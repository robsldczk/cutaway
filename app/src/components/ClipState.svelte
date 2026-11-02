<script lang="ts">
  // Clip mute and ripple delete as the simple pages show them: a badge that undoes it.
  import { app } from '../lib/app.svelte';
  import type { ItemView } from '../lib/types';

  let { it }: { it: ItemView } = $props();
  const partly = $derived(!it.muted && !it.dropped && it.parts.some((p) => p.muted || p.dropped));

  function restore(e: MouseEvent) {
    e.stopPropagation();
    if (it.dropped) void app.cmd({ cmd: 'set_clip_dropped', line_id: it.line_id, dropped: false });
    else void app.cmd({ cmd: 'set_clip_muted', line_id: it.line_id, muted: false });
  }
</script>

{#if it.dropped}
  <button class="cs" onclick={restore} title="Ripple-deleted on the Edit page: click to restore" data-testid="clip-state">Cut · restore</button>
{:else if it.muted}
  <button class="cs" onclick={restore} title="Muted: kept in place but silent (⌃M). Click to unmute" data-testid="clip-state">Muted</button>
{:else if partly}
  <button class="cs" onclick={() => app.setPage('edit')} title="Part of this line is muted or cut on the Edit page" data-testid="clip-state">Part muted</button>
{/if}

<style>
  .cs {
    flex: none;
    height: 18px;
    padding: 0 7px;
    border-radius: 9px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--text-on-pill);
    background: var(--warn);
    white-space: nowrap;
  }
</style>
