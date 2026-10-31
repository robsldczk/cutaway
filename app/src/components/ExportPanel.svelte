<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { call, BridgeError } from '../lib/bridge';
  import Sheet from './Sheet.svelte';

  let scope = $state<'scene' | 'all'>('scene');
  let results = $state<{ what: string; path: string }[]>([]);

  async function run(what: string, method: string, params: Record<string, unknown>) {
    const p = { ...params, ...(scope === 'scene' || method === 'export_mix' ? { scene: app.sceneId } : {}) };
    try {
      let r: { path: string };
      try {
        r = await call<{ path: string }>(method, p);
      } catch (e) {
        if (e instanceof BridgeError && e.code === 'needs_confirm' && confirm(`${e.message}. Overwrite?`)) r = await call<{ path: string }>(method, { ...p, confirm: true });
        else throw e;
      }
      results = [{ what, path: r.path }, ...results];
    } catch (e) {
      app.say(e instanceof Error ? e.message : String(e));
    }
  }
</script>

<Sheet title="Export">
  <div class="scope" role="group" aria-label="Scope">
    <button class:on={scope === 'scene'} onclick={() => (scope = 'scene')}>This scene</button>
    <button class:on={scope === 'all'} onclick={() => (scope = 'all')}>Everything</button>
  </div>
  <div class="grid">
    <button class="btn" onclick={() => run('FCPXML (Resolve, Logic)', 'export_fcpxml', {})}>FCPXML</button>
    <button class="btn" onclick={() => run('SRT', 'export_srt', {})}>SRT</button>
    <button class="btn" onclick={() => run('Cut list (CSV)', 'export_cut_list', {})}>Cut list CSV</button>
    <button class="btn" onclick={() => run('As-cut transcript', 'export_cut_list', { format: 'md' })}>As-cut transcript</button>
    <button class="btn" onclick={() => run('WAV', 'export_mix', { format: 'wav' })}>WAV (scene)</button>
    <button class="btn" onclick={() => run('MP3', 'export_mix', { format: 'mp3' })}>MP3 (scene)</button>
  </div>
  <p class="muted">Handles, loudness normalization and the room tone track are project settings (export.* in the CLI: <code>cutaway config export.handles_s 1.0</code>).</p>
  {#each results as r, i (i)}<div class="res"><b>{r.what}</b> <span class="muted">{r.path}</span></div>{/each}
</Sheet>

<style>
  .scope {
    display: inline-flex;
    padding: 3px;
    border-radius: var(--radius-pill);
    background: var(--glass-fill-2);
    margin-bottom: 14px;
  }
  .scope button {
    height: 30px;
    padding: 0 14px;
    border-radius: var(--radius-pill);
    font-size: 12px;
    color: var(--text-3);
  }
  .scope button.on {
    background: var(--button-light-active);
    color: var(--text-on-light);
    font-weight: 600;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }
  .res {
    margin-top: 8px;
    font-size: 12px;
    word-break: break-all;
  }
  code {
    font-size: 11px;
  }
</style>
