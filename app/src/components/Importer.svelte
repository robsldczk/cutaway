<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { call, inTauri, pickPath } from '../lib/bridge';
  import type { Job } from '../lib/types';
  import Sheet from './Sheet.svelte';

  interface Source { source_id: string; file: string; role: 'character' | 'room' | 'unknown'; character: string | null; confidence: number }
  let scriptPath = $state('');
  let audioPath = $state('');
  let summary = $state<string | null>(null);
  let sources = $state<Source[]>([]);
  let job = $state<Job | null>(null);
  const st = $derived(app.status);
  const characters = $derived(app.view?.characters ?? []);

  async function loadSources() {
    sources = (await app.run(() => call<Source[]>('roles'))) ?? [];
  }
  onMount(loadSources);

  async function importScript() {
    let p = scriptPath;
    if (!p && inTauri) {
      const r = await pickPath({ title: 'Script (Fountain / Throughline)', filters: [{ name: 'Script', extensions: ['fountain', 'txt'] }] });
      if (!r?.length) return;
      p = r[0];
    }
    if (!p) return;
    const r = await app.run(() => call<{ inserted: unknown[]; changed: unknown[]; deleted: unknown[]; unchanged: number }>('import_script', { path: p }));
    if (r) summary = `${r.inserted.length} new, ${r.changed.length} changed, ${r.deleted.length} deleted, ${r.unchanged} unchanged lines`;
    await app.boot();
  }

  async function importAudio() {
    let ps = audioPath ? audioPath.split(',').map((s) => s.trim()) : [];
    if (!ps.length && inTauri) {
      const r = await pickPath({ multiple: true, title: 'Audio files (role tracks and the room mic)', filters: [{ name: 'Audio', extensions: ['wav', 'bwf', 'aif', 'aiff', 'mp3', 'flac'] }] });
      if (!r?.length) return;
      ps = r;
    }
    if (!ps.length) return;
    await app.run(() => call('import_audio', { paths: ps, copy: true }));
    await loadSources();
    await app.boot();
  }

  async function setRole(s: Source, value: string) {
    const [role, character] = value.startsWith('c:') ? ['character', value.slice(2)] : [value, null];
    await app.run(() => call('confirm_roles', { assignments: [{ source_id: s.source_id, role, character }] }));
    await loadSources();
  }

  async function analyze() {
    const r = await app.run(() => call<{ job_id: string }>('start_analysis'));
    if (!r) return;
    const tick = async () => {
      const jobs = (await call<Job[]>('job_status')) ?? [];
      job = jobs.find((j) => j.id === r.job_id) ?? null;
      if (job?.state === 'running') setTimeout(tick, 400);
      else await app.boot();
    };
    void tick();
  }
</script>

<Sheet title="Import" width={660}>
  <ol class="steps">
    <li class:done={(st?.scenes ?? 0) > 0}>
      <div class="h">1 · Script</div>
      <div class="row">
        {#if !inTauri}<input class="field" placeholder="Path to the Fountain file" bind:value={scriptPath} />{/if}
        <button class="btn" onclick={importScript}>{(st?.scenes ?? 0) > 0 ? 'Import a new version…' : 'Choose script…'}</button>
      </div>
      {#if st && st.scenes > 0}<p class="muted">v{String(st.script_version).padStart(3, '0')} · {st.scenes} scenes · {st.lines} lines{summary ? ` · ${summary}` : ''}</p>{/if}
    </li>
    <li class:done={sources.length > 0}>
      <div class="h">2 · Audio</div>
      <div class="row">
        {#if !inTauri}<input class="field" placeholder="Files or folders, comma separated" bind:value={audioPath} />{/if}
        <button class="btn" onclick={importAudio} disabled={(st?.scenes ?? 0) === 0}>Add audio…</button>
      </div>
      {#if sources.length}
        <table>
          <thead><tr><th>File</th><th>Role</th><th>Sure</th></tr></thead>
          <tbody>
            {#each sources as s (s.source_id)}
              <tr>
                <td class="ellip">{s.file}</td>
                <td>
                  <select class="field" value={s.role === 'character' ? `c:${s.character}` : s.role} onchange={(e) => setRole(s, (e.currentTarget as HTMLSelectElement).value)}>
                    {#each characters as c (c.id)}<option value={`c:${c.id}`}>{c.label}</option>{/each}
                    <option value="room">Room (director)</option>
                    <option value="unknown">Unknown</option>
                  </select>
                </td>
                <td class:low={s.confidence < 0.7}>{Math.round(s.confidence * 100)}%</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </li>
    <li class:done={(st?.takes ?? 0) > 0}>
      <div class="h">3 · Analysis</div>
      <div class="row"><button class="btn primary" onclick={analyze} disabled={!sources.length || job?.state === 'running'}>{(st?.takes ?? 0) > 0 ? 'Analyze again' : 'Analyze'}</button></div>
      {#if job}
        <div class="bar"><span style="width: {Math.round(job.progress * 100)}%"></span></div>
        <p class="muted">{job.state === 'failed' ? `Failed: ${job.message}` : job.message || job.stage}</p>
      {/if}
      {#if st && st.takes > 0}
        <div class="card">
          <div><b>{st.lines_with_takes}</b><span>of {st.recorded_role_lines} lines matched</span></div>
          <div><b>{st.takes}</b><span>takes found</span></div>
          <div><b>{st.picks}</b><span>picks found</span></div>
          <div><b>{st.flagged}</b><span>flagged</span></div>
        </div>
        {#if st.ai_warning}<p class="muted">{st.ai_warning}</p>{/if}
        <button class="btn primary" onclick={() => (app.overlay = null)}>Start editing</button>
      {/if}
    </li>
  </ol>
</Sheet>

<style>
  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .h {
    font-weight: 600;
  }
  li.done .h::after {
    content: ' ✓';
    color: var(--text-3);
  }
  .row {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }
  .row .field {
    flex: 1;
  }
  table {
    width: 100%;
    margin-top: 10px;
    border-collapse: collapse;
    font-size: 12px;
  }
  th {
    text-align: left;
    color: var(--text-4);
    font-weight: 600;
    padding: 4px 6px;
  }
  td {
    padding: 4px 6px;
    max-width: 280px;
  }
  td select {
    height: 28px;
  }
  .low {
    color: var(--warn);
  }
  .bar {
    margin-top: 10px;
    height: 6px;
    border-radius: 3px;
    background: var(--glass-fill-3);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--text-strong);
    transition: width 0.3s;
  }
  .card {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 10px;
    margin: 12px 0;
  }
  .card div {
    display: flex;
    flex-direction: column;
    padding: 12px;
    border-radius: 14px;
    background: var(--glass-fill-2);
  }
  .card b {
    font-size: 22px;
  }
  .card span {
    font-size: 11px;
    color: var(--text-4);
  }
</style>
