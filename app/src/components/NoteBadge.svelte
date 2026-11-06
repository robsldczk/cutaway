<script lang="ts">
  // A line's open notes, on its card on Options and Arrange.
  import { app } from '../lib/app.svelte';

  let { line }: { line: string } = $props();
  const marks = $derived((app.view?.notes ?? []).filter((n) => n.line_id === line));
  const tip = $derived(marks.map((m) => `${m.who}: ${m.text}`).join('\n'));
</script>

{#if marks.length}
  <button class="nb tn" title={tip} aria-label="{marks.length} open {marks.length === 1 ? 'note' : 'notes'}" onmousedown={(e) => e.stopPropagation()} ondblclick={(e) => e.stopPropagation()} onclick={(e) => { e.stopPropagation(); app.openNote(marks[0].id); }} data-testid="note-badge">{marks.length}</button>
{/if}

<style>
  .nb {
    flex: none;
    min-width: 17px;
    height: 17px;
    padding: 0 5px;
    box-sizing: border-box;
    border-radius: 9px 9px 9px 2px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    font-weight: 700;
    background: var(--accent);
    color: var(--text-on-pill);
  }
</style>
