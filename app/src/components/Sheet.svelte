<script lang="ts">
  import type { Snippet } from 'svelte';
  import { app } from '../lib/app.svelte';
  let { title, children, width = 560, closable = true, onclose }: { title: string; children: Snippet; width?: number; closable?: boolean; onclose?: () => void } = $props();
  const close = () => (onclose ? onclose() : (app.overlay = null));
</script>

<div class="scrim" role="presentation" onclick={() => closable && close()}></div>
<div class="glass sheet" role="dialog" aria-modal="true" aria-label={title} style="width: min({width}px, 94%); left: 50%; top: 50%; transform: translate(-50%, -50%)">
  <div class="top"><span class="t">{title}</span>{#if closable}<button class="x" onclick={close} aria-label="Close">×</button>{/if}</div>
  {@render children()}
</div>

<style>
  .sheet {
    max-height: 88vh;
    overflow-y: auto;
    font-size: var(--fs-ui);
  }
  .top {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;
  }
  .t {
    font-weight: 700;
    font-size: 15px;
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
</style>
