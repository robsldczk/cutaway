<script lang="ts">
  // Unlocking asks first, and says what it means. Staying locked has the focus: Enter keeps it.
  import { onMount } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { unlockQuestion } from '../lib/export';
  import Sheet from './Sheet.svelte';

  const id = app.unlocking ?? '';
  const scene = $derived(app.view?.scenes.find((s) => s.id === id));
  const next = $derived(app.exported[id]?.history.length ? app.exported[id].next_version : null);
  let keep = $state<HTMLButtonElement>();
  const close = () => (app.unlocking = null);

  async function unlock() {
    close();
    await app.unlockScene(id);
  }

  onMount(() => keep?.focus());
</script>

<Sheet title="Unlock Sc. {scene?.number ?? ''}?" width={420} onclose={close}>
  <p data-testid="unlock-question">{unlockQuestion(next)}</p>
  <div class="row">
    <button class="btn" bind:this={keep} onclick={close}>Keep locked</button>
    <button class="btn primary" onclick={unlock} data-testid="unlock-confirm">Unlock</button>
  </div>
</Sheet>

<style>
  p {
    margin: 0 0 18px;
    font-size: 13px;
    line-height: 1.5;
    color: var(--text-2);
  }
  .row {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
