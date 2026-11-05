<script lang="ts">
  // The quiet line over a locked scene. It keeps its word on what changed: gain and pan stay free
  // on a locked scene, and they do make the delivered files out of date.
  import { app } from '../lib/app.svelte';
  import { lockLine } from '../lib/export';
  import Icon from './Icon.svelte';

  const s = $derived(app.sceneLocked);
  const rev = $derived(app.view?.rev);
  const since = $derived(s ? (app.exported[s.id]?.changed_since ?? null) : null);

  $effect(() => {
    if (rev !== undefined) void app.loadExports();
  });
</script>

{#if s?.lock}
  <div class="lock glass" role="status" data-testid="lock-banner" data-keeps-sound>
    <Icon name="lock" size={12} />
    <span class="ellip">{lockLine(s.number, s.lock, since)}</span>
    <button onclick={() => (app.unlocking = s.id)}>Unlock</button>
  </div>
{/if}

<style>
  .lock {
    position: absolute;
    top: 64px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 11;
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: min(760px, 86%);
    height: 30px;
    padding: 0 6px 0 12px;
    border-radius: var(--radius-pill);
    background: var(--panel);
    font-size: 12px;
    color: var(--text-3);
  }
  button {
    flex: none;
    height: 22px;
    padding: 0 10px;
    border-radius: var(--radius-pill);
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text-1);
    background: var(--glass-fill-4);
  }
  button:hover {
    background: var(--glass-hover);
  }
</style>
