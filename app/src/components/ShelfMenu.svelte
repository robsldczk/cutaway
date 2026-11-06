<script lang="ts">
  // A tile's right click. Nothing here deletes a file.
  import { onMount } from 'svelte';
  import { clamp } from '../lib/format';

  let { x, y, items, onclose }: { x: number; y: number; items: { label: string; run: () => void }[]; onclose: () => void } = $props();

  const MENU_W = 200;
  const left = $derived(clamp(x, 8, window.innerWidth - MENU_W - 8));
  const top = $derived(clamp(y, 8, window.innerHeight - items.length * 34 - 20));
  let el = $state<HTMLDivElement>();

  onMount(() => el?.querySelector('button')?.focus());
</script>

<button class="away" aria-label="Close menu" tabindex="-1" onclick={onclose} oncontextmenu={(e) => { e.preventDefault(); onclose(); }}></button>
<div class="menu glass" role="menu" bind:this={el} style="left: {left}px; top: {top}px; width: {MENU_W}px">
  {#each items as it (it.label)}
    <button role="menuitem" onclick={() => { onclose(); it.run(); }}>{it.label}</button>
  {/each}
</div>

<style>
  .away {
    position: absolute;
    inset: 0;
    z-index: 39;
    cursor: default;
  }
  .menu {
    position: absolute;
    z-index: 40;
    padding: 6px;
    box-sizing: border-box;
    border-radius: var(--radius-btn);
    background: var(--panel-solid);
  }
  button[role='menuitem'] {
    display: block;
    width: 100%;
    padding: 7px 10px;
    border-radius: 9px;
    text-align: left;
    font-size: 12.5px;
    color: var(--text-2);
    outline: none;
  }
  button[role='menuitem']:hover,
  button[role='menuitem']:focus-visible {
    background: var(--glass-hover);
  }
</style>
