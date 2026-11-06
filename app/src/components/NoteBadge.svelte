<script lang="ts">
  // A line's open notes, on its card on Options and Arrange.
  import { app } from '../lib/app.svelte';

  let { line }: { line: string } = $props();
  const marks = $derived((app.view?.notes ?? []).filter((n) => n.line_id === line));
  const tip = $derived(marks.map((m) => `${m.who}: ${m.text}`).join('\n'));
</script>

{#if marks.length}
  <button class="note-count tn" title={tip} aria-label="{marks.length} open {marks.length === 1 ? 'note' : 'notes'}" onmousedown={(e) => e.stopPropagation()} ondblclick={(e) => e.stopPropagation()} onclick={(e) => { e.stopPropagation(); app.openNote(marks[0].id); }} data-testid="note-badge">{marks.length}</button>
{/if}
