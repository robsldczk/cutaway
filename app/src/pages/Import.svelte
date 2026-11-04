<script lang="ts">
  import { app } from '../lib/app.svelte';
  import ImportFiles from './ImportFiles.svelte';
  import ImportRooms from './ImportRooms.svelte';
  import ImportSounds from './ImportSounds.svelte';
</script>

{#if app.importTab === 'files'}<ImportFiles />{:else if app.importTab === 'rooms'}<ImportRooms />{:else}<ImportSounds />{/if}
