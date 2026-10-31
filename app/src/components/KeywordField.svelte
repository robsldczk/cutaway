<script lang="ts">
  // Keyword chips (the search query of a bucket). Enter or comma adds, ⌫ on empty removes.
  let { keywords = $bindable([]), onchange }: { keywords: string[]; onchange?: (k: string[]) => void } = $props();
  let draft = $state('');

  function set(k: string[]) {
    keywords = k;
    onchange?.(k);
  }
  function add() {
    const w = draft.trim().toLowerCase().replace(/,$/, '');
    draft = '';
    if (w && !keywords.includes(w)) set([...keywords, w]);
  }
  function key(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ',') {
      e.preventDefault();
      add();
    } else if (e.key === 'Backspace' && !draft && keywords.length) set(keywords.slice(0, -1));
  }
</script>

<div class="kwfield" aria-label="Keywords">
  {#each keywords as k (k)}<span class="chip">{k}<button class="rm" aria-label="Remove {k}" onclick={() => set(keywords.filter((x) => x !== k))}>×</button></span>{/each}
  <input placeholder="Add keyword" aria-label="Add keyword" bind:value={draft} onkeydown={key} onblur={add} />
</div>
