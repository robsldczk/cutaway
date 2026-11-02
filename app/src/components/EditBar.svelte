<script lang="ts">
  // The Edit page's control bar, modelled on Logic: inspector and mixer toggles, tools,
  // transport with cycle and a time display, snap, ripple, lock, zoom and the bucket toggle.
  import { app } from '../lib/app.svelte';
  import { fmt } from '../lib/format';
  import { TOOLS } from '../lib/tools';
  import Icon from './Icon.svelte';

  let { onZoom, lcdDetail }: { onZoom: (px: number) => void; lcdDetail: string } = $props();

  const WAVE = [1, 2, 4, 8];

  function toStart() {
    if (app.playing) void app.stop();
    app.seek(app.cycle.on ? app.cycle.from : 0);
  }
</script>

<section class="glass cbar" aria-label="Control bar">
  <button class="ib" class:on={app.editInspector} aria-label="Inspector (I)" title="Inspector (I)" aria-pressed={app.editInspector} onclick={() => (app.editInspector = !app.editInspector)}><Icon name="inspector" size={17} stroke={1.8} /></button>
  <button class="ib" class:on={app.mixer} aria-label="Mixer (X)" title="Mixer (X)" aria-pressed={app.mixer} onclick={() => (app.mixer = !app.mixer)}><Icon name="mixer" size={17} stroke={1.8} /></button>
  <span class="sep" aria-hidden="true"></span>
  <div class="tools" role="group" aria-label="Tools">
    {#each TOOLS as t (t.id)}
      <button class="ib" class:on={app.tool === t.id} aria-label={t.label} title="{t.label} (T {t.key})" aria-pressed={app.tool === t.id} onclick={() => (app.tool = t.id)}><Icon name={t.id} stroke={1.8} /></button>
    {/each}
  </div>
  <span class="flex"></span>
  <button class="ib" aria-label="Go to start" title="Go to start" onclick={toStart}><Icon name="start" /></button>
  <button class="play" aria-label="Play (Space)" title="Play (Space)" onclick={() => app.togglePlay()}><Icon name={app.playing ? 'stop' : 'play'} /></button>
  <button class="ib" class:on={app.cycle.on} aria-label="Cycle (C)" title="Cycle (C)" aria-pressed={app.cycle.on} onclick={() => (app.cycle = { ...app.cycle, on: !app.cycle.on })}><Icon name="cycle" size={16} /></button>
  <div class="lcd" aria-label="Time">
    <span class="tn big">{fmt(app.pos).replace(/^0/, '')}</span>
    <span class="muted ellip small">{lcdDetail}</span>
  </div>
  <span class="flex"></span>
  <button class="btn" class:act={app.snapWords} aria-pressed={app.snapWords} onclick={() => (app.snapWords = !app.snapWords)} title="Snap to words and action lines">Snap: {app.snapWords ? 'words' : 'off'}</button>
  <button class="btn" class:act={app.ripple} aria-pressed={app.ripple} onclick={() => (app.ripple = !app.ripple)} title="Ripple moves everything after a clip (⌥-drag inverts); ⌫ then ripple-deletes dialogue">Ripple</button>
  <button class="btn" class:act={app.lockScript} aria-pressed={app.lockScript} onclick={() => (app.lockScript = !app.lockScript)} title="Locked: dialogue follows the script order and pauses. Unlocked: dialogue moves freely in time"><Icon name={app.lockScript ? 'lock' : 'unlock'} size={13} />Lock</button>
  <button class="btn tn" onclick={() => (app.waveGain = WAVE[(WAVE.indexOf(app.waveGain) + 1) % WAVE.length])} title="Waveform height" aria-label="Waveform height">Wave ×{app.waveGain}</button>
  <input type="range" min={Math.log(2)} max={Math.log(1000)} step="0.01" value={Math.log(app.editPx || 50)} oninput={(e) => onZoom(Math.exp(+(e.currentTarget as HTMLInputElement).value))} aria-label="Zoom" title="Zoom (⌘← ⌘→, Z)" class="zoom" />
  <button class="btn" class:act={app.editBucket} aria-pressed={app.editBucket} onclick={() => (app.editBucket = !app.editBucket)}>Bucket <span class="kb">B</span></button>
</section>

<style>
  .cbar {
    position: absolute;
    top: 66px;
    left: 20px;
    right: 20px;
    height: 56px;
    box-sizing: border-box;
    border-radius: 20px;
    padding: 8px 10px;
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--bar);
    z-index: 9;
  }
  .ib {
    width: 34px;
    height: 34px;
  }
  .sep {
    width: 1px;
    height: 24px;
    background: var(--glass-outline);
    margin: 0 6px;
  }
  .tools {
    display: flex;
    gap: 2px;
    padding: 3px;
    border-radius: 12px;
    background: var(--glass-fill-2);
  }
  .flex {
    flex: 1;
  }
  .play {
    width: 40px;
    height: 40px;
    border-radius: 50%;
    background: var(--button-light);
    color: var(--text-on-light);
    display: flex;
    align-items: center;
    justify-content: center;
    flex: none;
  }
  .lcd {
    display: flex;
    flex-direction: column;
    justify-content: center;
    height: 40px;
    min-width: 150px;
    max-width: 260px;
    padding: 0 16px;
    margin: 0 6px;
    border-radius: 12px;
    background: var(--lcd);
    box-shadow: inset 0 0 0 1px var(--glass-outline);
  }
  .big {
    font-size: 17px;
    font-weight: 600;
    line-height: 1;
  }
  .small {
    font-size: 10.5px;
    margin-top: 3px;
  }
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 12px;
    font-size: 12px;
    white-space: nowrap;
    box-shadow: inset 0 0 0 1px var(--glass-outline);
  }
  .btn.act {
    background: rgb(var(--ink-rgb) / 0.14);
  }
  .zoom {
    width: 90px;
    accent-color: var(--button-light);
  }
  @media (max-width: 1100px) {
    .lcd {
      min-width: 0;
    }
    .zoom {
      display: none;
    }
  }
</style>
