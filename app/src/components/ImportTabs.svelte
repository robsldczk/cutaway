<script lang="ts">
  import { app } from '../lib/app.svelte';
</script>

<div class="seg" role="tablist" aria-label="Import">
  <button role="tab" aria-selected={app.importTab === 'files'} class="tab" class:on={app.importTab === 'files'} onclick={() => (app.importTab = 'files')}>Files</button>
  <button role="tab" aria-selected={app.importTab === 'sounds'} class="tab" class:on={app.importTab === 'sounds'} onclick={() => (app.importTab = 'sounds')}>Sounds</button>
  <button role="tab" aria-selected={app.importTab === 'rooms'} class="tab" class:on={app.importTab === 'rooms'} onclick={() => (app.importTab = 'rooms')}>Rooms</button>
</div>
