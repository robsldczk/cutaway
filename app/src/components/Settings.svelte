<script lang="ts">
  import { app } from '../lib/app.svelte';
  import Sheet from './Sheet.svelte';

  const v = $derived(app.view);
  let breath = $state(app.view?.breath_gap_s ?? 0.3);
  let tight = $state(app.view?.tightness ?? 1);
  let model = $state('');

  async function setBreath() {
    await app.method('set_config', { key: 'pacing.breath_gap_s', value: +breath });
  }
  async function setTight() {
    if (!v) return;
    await app.cmd({ cmd: 'set_scene_tightness', scene_id: v.scene.id, tightness: +tight });
  }
  async function roomTone(on: boolean) {
    await app.method('set_config', { key: 'pacing.room_tone', value: on });
  }
  async function locTone(on: boolean) {
    if (!v) return;
    const tone = { ...v.room_tone, enabled: on };
    if (on && !tone.source_id) {
      // default: the quietest second before the first take of the scene on its source
      const first = v.items.find((i) => i.kind === 'line' && i.takes.length);
      const t = first?.takes[0];
      if (t) Object.assign(tone, { source_id: t.source_id, t0: Math.max(0, t.src_in - 1.2), t1: Math.max(0.5, t.src_in - 0.2) });
    }
    await app.cmd({ cmd: 'set_room_tone', location_id: v.location_id, tone });
  }
  async function setModel() {
    if (model.trim()) await app.method('set_config', { key: 'ai.model', value: model.trim() });
  }
</script>

<Sheet title="Settings">
  <div class="grid">
    <label for="breath">Breath gap</label>
    <div class="ctl"><input id="breath" type="range" min="0" max="1.5" step="0.05" bind:value={breath} onchange={setBreath} /><span>{(+breath * 1000).toFixed(0)} ms</span></div>
    <label for="tight">Scene tightness</label>
    <div class="ctl"><input id="tight" type="range" min="0.3" max="2" step="0.05" bind:value={tight} onchange={setTight} /><span>×{(+tight).toFixed(2)}</span></div>
    <span class="hint2">Scales every default pause and spacer of this scene. Pauses you set by hand are kept.</span>
    <span class="lab">Room tone</span>
    <div class="ctl">
      <label class="chk"><input type="checkbox" checked={app.status?.room_tone ?? true} onchange={(e) => roomTone((e.currentTarget as HTMLInputElement).checked)} /> play room tone under scenes</label>
    </div>
    <span class="lab">This location</span>
    <div class="ctl">
      <label class="chk"><input type="checkbox" checked={v?.room_tone.enabled ?? false} onchange={(e) => locTone((e.currentTarget as HTMLInputElement).checked)} /> {v?.scene.place || 'location'} has a room tone</label>
    </div>
    <span class="lab">AI</span>
    <div class="ctl col">
      <span>Provider: {app.status?.ai_provider}{app.status?.ai_warning ? ' (rules: ' + app.status.ai_warning + ')' : ''}</span>
      <span class="row"><input class="field" placeholder="model, e.g. claude-sonnet-5-5" bind:value={model} /><button class="btn" onclick={setModel}>Set</button></span>
      <span class="hint2">Only text (transcripts and script) leaves this machine, never audio.</span>
    </div>
  </div>
</Sheet>

<style>
  .grid {
    display: grid;
    grid-template-columns: 140px 1fr;
    gap: 14px 18px;
    align-items: center;
  }
  label,
  .lab {
    color: var(--text-3);
  }
  .ctl {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .ctl.col {
    flex-direction: column;
    align-items: flex-start;
  }
  .ctl input[type='range'] {
    flex: 1;
    accent-color: var(--text-strong);
  }
  .ctl span {
    font-variant-numeric: tabular-nums;
    color: var(--text-2);
  }
  .hint2 {
    grid-column: 2;
    font-size: 11px;
    color: var(--text-4) !important;
  }
  .chk {
    display: flex;
    gap: 8px;
    align-items: center;
    color: var(--text-2);
  }
  .row {
    display: flex;
    gap: 8px;
  }
</style>
