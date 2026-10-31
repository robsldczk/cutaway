<script lang="ts">
  import type { Snippet } from 'svelte';
  import { app } from '../lib/app.svelte';
  let { title, children, width = 340 }: { title: string; children: Snippet; width?: number } = $props();
</script>

<aside class="glass drawer" aria-label={title} style="width: min({width}px, 90vw)">
  <div class="top"><span class="t">{title}</span><button class="x" onclick={() => (app.drawer = null)} aria-label="Close">×</button></div>
  <div class="body">{@render children()}</div>
</aside>

<style>
  .drawer {
    position: absolute;
    right: 20px;
    top: 80px;
    bottom: 120px;
    z-index: 15;
    border-radius: var(--radius-sheet);
    background: var(--panel-solid);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .top {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 18px 8px;
  }
  .t {
    font-weight: 700;
    font-size: 14px;
  }
  .x {
    font-size: 20px;
    color: var(--text-3);
    width: 28px;
    height: 28px;
    border-radius: 8px;
  }
  .x:hover {
    background: var(--glass-hover);
  }
  .body {
    flex: 1;
    overflow-y: auto;
    padding: 4px 18px 18px;
    font-size: 12.5px;
  }
</style>
