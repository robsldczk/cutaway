<script lang="ts">
  // The sound on its way across the window, until a drop zone shows it itself.
  import { drag } from '../lib/drag.svelte';
</script>

{#if drag.item && !drag.shown}
  <div class="pill ellip" style="transform: translate({drag.x + 14}px, {drag.y + 10}px)" aria-hidden="true" data-testid="drag-ghost">{drag.item.name}</div>
{/if}

<style>
  :global(body.sound-drag),
  :global(body.sound-drag *) {
    cursor: grabbing !important;
  }
  .pill {
    position: fixed;
    left: 0;
    top: 0;
    z-index: 60;
    max-width: 260px;
    pointer-events: none;
    padding: 6px 12px;
    border-radius: var(--radius-pill);
    background: var(--panel-solid);
    font-size: 12px;
    box-shadow: var(--glass-shadow);
  }
</style>
