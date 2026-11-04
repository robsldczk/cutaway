<script lang="ts">
  // The buckets' own little player. It runs on its own clock, so the timeline's playhead stays
  // put: the main transport is for what sits on the timeline. Leaving it stops the sound.
  import { onDestroy } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { len } from '../lib/format';
  import Icon from './Icon.svelte';
  import Waveform from './Waveform.svelte';

  const p = $derived(app.preview);
  const at = $derived(p && p.duration_s > 0 ? Math.min(100, (p.pos / p.duration_s) * 100) : 0);

  function seek(e: MouseEvent) {
    if (!p) return;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    void app.playSound(p.key, Math.max(0, ((e.clientX - r.left) / r.width) * p.duration_s), p);
  }

  onDestroy(() => {
    if (app.preview?.playing) void app.stopSound();
  });
</script>

{#if p}
  <div class="pv" role="group" aria-label="Preview player" data-testid="preview-player">
    <button class="pl" aria-label="{p.playing ? 'Stop' : 'Play'} preview of {p.name}" onmousedown={(e) => e.preventDefault()} onclick={() => (p.playing ? app.stopSound() : app.playSound(p.key, 0, p))}><Icon name={p.playing ? 'stop' : 'play'} size={11} /></button>
    <div class="mid">
      <span class="ellip nm">{p.name}</span>
      <span class="wv" role="button" tabindex="-1" aria-label="Play {p.name} from here" onclick={seek} onkeydown={() => {}} data-testid="preview-wave">
        <Waveform peaks={p.peaks} fill="var(--sfx)" />
        <span class="ph" style="left: {at}%"></span>
      </span>
    </div>
    <span class="muted tn d" data-testid="preview-time">{len(p.pos)} / {len(p.duration_s)}</span>
  </div>
{/if}

<style>
  .pv {
    flex: none;
    display: grid;
    grid-template-columns: 32px minmax(0, 1fr) auto;
    gap: 10px;
    align-items: center;
    padding: 8px 10px;
    border-radius: 14px;
    background: var(--glass-fill-1);
  }
  .pl {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--sfx);
    color: var(--on-sfx);
  }
  .mid {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .nm {
    font-size: 12px;
    font-weight: 600;
  }
  .wv {
    position: relative;
    height: 18px;
    cursor: pointer;
  }
  .ph {
    position: absolute;
    top: -2px;
    bottom: -2px;
    width: 2px;
    background: var(--playhead);
  }
  .d {
    font-size: 10.5px;
    white-space: nowrap;
  }
</style>
