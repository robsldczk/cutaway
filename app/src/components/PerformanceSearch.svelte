<script lang="ts">
  // The bucket's character search: "paul huh" lists where Paul's mic heard it (D101). The bar
  // lives in the bucket; a name picked here goes in the bar.
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import PerformanceHit from './PerformanceHit.svelte';
  import type { PerformanceResults } from '../lib/types';
  import { colorOf, solid } from '../theme';

  let { query, onpick }: { query: string; onpick: (name: string) => void } = $props();

  let found = $state<PerformanceResults | null>(null);
  let shown = $state('');

  $effect(() => {
    const term = query.trim();
    const wait = setTimeout(
      async () => {
        const r = await app.run(() => call<PerformanceResults>('search_performances', { query: term }));
        if (query.trim() !== term || !r) return;
        found = r;
        shown = term;
      },
      term ? 180 : 0,
    );
    return () => clearTimeout(wait);
  });

  const who = $derived(found?.cast.find((c) => c.id === found?.character)?.label ?? 'anyone');
</script>

{#if !query.trim() || !found?.character}
  <div class="cast" role="group" aria-label="Characters">
    {#each found?.cast ?? [] as c (c.id)}
      <button class="pick" onmousedown={(e) => e.preventDefault()} onclick={() => onpick(c.label)} data-testid="cast-pick"><span class="dot" style="background: {solid(colorOf(c.color_index).rgb)}"></span>{c.label}</button>
    {/each}
  </div>
{/if}
<div class="list">
  {#if shown}
    {#each found?.hits ?? [] as h (h.id)}
      <PerformanceHit hit={h} />
    {:else}
      <p class="muted note">{found?.heard ? `Nothing like that from ${who}.` : 'No transcripts to search yet: run the analysis first.'}</p>
    {/each}
    {#if found && found.total > found.hits.length}<p class="muted note" data-testid="hits-more">The best {found.hits.length} of {found.total}.</p>{/if}
  {:else if !query.trim()}
    <p class="muted note">A name and what they said, like <em>{found?.cast[0]?.label ?? 'Paul'} huh</em>. A kind of sound works too: grunt, breath, laugh, sigh.</p>
  {/if}
</div>

<style>
  .cast {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .pick {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 10px;
    border-radius: var(--radius-pill);
    font-size: 11.5px;
    background: var(--glass-fill-1);
  }
  .pick .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .note {
    font-size: 12px;
    margin: 4px 2px;
  }
</style>
