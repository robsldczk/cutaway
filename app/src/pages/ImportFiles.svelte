<script lang="ts">
  // Import › Files (Round 2): script and audio drops, the cast (recording → character,
  // director room → recordings), "Needs you" fixes and Start analysis.
  import { app } from '../lib/app.svelte';
  import { call, inTauri, pickPath } from '../lib/bridge';
  import { ago, len } from '../lib/format';
  import { colorOf } from '../theme';
  import { solid } from '../lib/colors';
  import Icon from '../components/Icon.svelte';
  import Sheet from '../components/Sheet.svelte';
  import Waveform from '../components/Waveform.svelte';
  import ImportTabs from '../components/ImportTabs.svelte';
  import type { CastCard, Fix, ImportView, Job } from '../lib/types';

  interface LineChange { line_id: string; character: string | null; old_text: string | null; new_text: string | null }
  interface ScriptSummary { version: number; unchanged: number; changed: LineChange[]; inserted: LineChange[]; deleted: LineChange[]; scenes_added: string[]; scenes_removed: string[]; takes_to_rematch: number; takes_to_unmatched: number }

  let v = $state<ImportView | null>(null);
  let remember = $state(true);
  let job = $state<Job | null>(null);
  let diff = $state<ScriptSummary | null>(null);
  let suggestion = $state<Record<string, { character: string | null; confidence: number; reason: string }>>({});
  let linking = $state<string | null>(null);
  let linkSel = $state<string[]>([]);
  let pathScript = $state('');
  let pathAudio = $state('');
  let now = $state(Date.now());

  async function load() {
    v = (await app.run(() => call<ImportView>('import_view'))) ?? v;
  }
  $effect(() => {
    void app.status?.rev;
    void load();
  });
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 30000);
    return () => clearInterval(t);
  });

  const cast = $derived(v?.cast ?? []);
  const label = (id: string | null | undefined) => cast.find((c) => c.id === id)?.label ?? id ?? '';
  const col = (c: CastCard) => solid(colorOf(c.color_index).rgb);
  const characters = $derived(cast.map((c) => ({ id: c.id, label: c.label })));
  const recordings = $derived(cast.flatMap((c) => c.recordings.map((r) => ({ ...r, label: c.label }))));
  const canAnalyze = $derived(!!v?.script && (v?.files.recordings ?? 0) > 0 && job?.state !== 'running');

  async function chooseScript() {
    let p = pathScript.trim();
    if (!p && inTauri) {
      const r = await pickPath({ title: 'Script (Fountain)', filters: [{ name: 'Script', extensions: ['fountain', 'txt'] }] });
      if (!r?.length) return;
      p = r[0];
    }
    if (!p) return;
    const r = await app.method<ScriptSummary>('import_script', { path: p });
    pathScript = '';
    if (r && (r.version > 1 || r.changed.length || r.deleted.length)) diff = r;
    await app.boot();
    await load();
  }

  async function addAudio(forCharacter?: string) {
    let ps = pathAudio ? pathAudio.split(',').map((s) => s.trim()).filter(Boolean) : [];
    if (!ps.length && inTauri) {
      const r = await pickPath({ multiple: true, title: forCharacter ? `Recording for ${label(forCharacter)}` : 'Recordings and director room files', filters: [{ name: 'Audio', extensions: ['wav', 'bwf', 'aif', 'aiff', 'mp3', 'flac'] }] });
      if (!r?.length) return;
      ps = r;
    }
    if (!ps.length) {
      app.say('Type a path in the field first');
      return;
    }
    const r = await app.method<{ source_id: string }[]>('import_audio', { paths: ps, copy: true }, false);
    pathAudio = '';
    if (r && forCharacter) {
      await app.run(() => call('confirm_roles', { assignments: r.map((s) => ({ source_id: s.source_id, role: 'character', character: forCharacter })), remember }));
    }
    await app.run(() => call('match_files'));
    await load();
  }

  async function setRole(sourceId: string, role: 'character' | 'room' | 'unknown', character: string | null = null) {
    await app.run(() => call('confirm_roles', { assignments: [{ source_id: sourceId, role, character }], remember }));
    delete suggestion[sourceId];
    await load();
  }

  async function suggest(f: Fix) {
    if (!f.source_id) return;
    const r = await app.run(() => call<{ character: string | null; confidence: number; reason: string }>('suggest_character', { source_id: f.source_id }));
    if (!r) return;
    if (!r.character) app.say(`No suggestion for ${f.file}`);
    else suggestion = { ...suggestion, [f.source_id]: r };
  }

  async function redoMatch() {
    await app.run(() => call('match_files'));
    await load();
    app.say('Matched again');
  }

  function openLink(roomId: string) {
    linking = linking === roomId ? null : roomId;
    linkSel = [];
  }

  async function link() {
    if (!linking || !linkSel.length) return;
    await app.run(() => call('link_room', { room_id: linking, recording_ids: linkSel }));
    linking = null;
    await load();
  }

  async function relink() {
    let dir = pathAudio.trim();
    if (!dir && inTauri) {
      const r = await pickPath({ directory: true, title: 'Folder with the missing files' });
      if (!r?.length) return;
      dir = r[0];
    }
    if (!dir) return;
    await app.method('relink_media', { search_dir: dir });
    await load();
  }

  async function analyze() {
    const r = await app.run(() => call<{ job_id: string }>('start_analysis'));
    if (!r) return;
    job = { id: r.job_id, kind: 'analysis', state: 'running', progress: 0, stage: 'start', message: '', result: null };
    const j = await app.waitJob(r.job_id, (x) => (job = x));
    job = j ?? null;
    await app.boot();
    await load();
    if (j?.state === 'done') {
      app.say('Analysis done');
      app.setPage('options');
    }
  }

  function shared(r: { shared_with: string[] }, who: string) {
    return r.shared_with.length ? `Ensemble · shared with ${r.shared_with.length} other${r.shared_with.length > 1 ? 's' : ''}` : `Only ${who}`;
  }
  function sync(s: number | null, by: string) {
    if (s == null) return by === 'timecode' ? 'TC' : '';
    if (Math.abs(s) < 0.0005) return 'ref';
    const ms = Math.round(s * 1000);
    return `${ms > 0 ? '+' : '−'}${Math.abs(ms)} ms`;
  }
  async function undoScript() {
    await app.undo();
    diff = null;
    await load();
  }
</script>

<main class="imp">
  <div class="tabs"><ImportTabs /></div>
  <div class="drops">
    <div class="drop" aria-label="Script drop zone">
      <span class="tile ftn">FTN</span>
      <div class="grow">
        {#if v?.script}
          <div class="ellip big">v{String(v.script.number).padStart(3, '0')} {v.script.name}</div>
          <div class="muted sub">Script · {v.script.characters} characters</div>
        {:else}
          <div class="big">Drop the script</div>
          <div class="muted sub">A Fountain file</div>
        {/if}
        {#if !inTauri}<input class="field path" placeholder="Path to the .fountain file" bind:value={pathScript} />{/if}
      </div>
      <button class="btn" onclick={chooseScript}>{v?.script ? 'Replace' : 'Choose'}</button>
    </div>
    <div class="drop" aria-label="Audio drop zone">
      <span class="tile"><svg width="24" height="18" viewBox="0 0 28 20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><line x1="2" y1="8" x2="2" y2="12" /><line x1="7" y1="4" x2="7" y2="16" /><line x1="12" y1="1" x2="12" y2="19" /><line x1="17" y1="5" x2="17" y2="15" /><line x1="22" y1="7" x2="22" y2="13" /><line x1="26" y1="9" x2="26" y2="11" /></svg></span>
      <div class="grow">
        {#if v && v.files.total > 0}
          <div class="big">{v.files.total} audio file{v.files.total === 1 ? '' : 's'}</div>
          <div class="muted sub">{v.files.recordings} recording{v.files.recordings === 1 ? '' : 's'} · {v.files.rooms} director room file{v.files.rooms === 1 ? '' : 's'}</div>
        {:else}
          <div class="big">Drop the recordings</div>
          <div class="muted sub">WAV or MP3 files, or a whole folder</div>
        {/if}
        {#if !inTauri}<input class="field path" placeholder="Files or folders, comma separated" bind:value={pathAudio} />{/if}
      </div>
      <button class="btn" onclick={() => addAudio()} disabled={!v?.script}>Add files</button>
    </div>
  </div>

  {#if v && v.files.total > 0}
    <div class="casthead">
      <div class="legend">
        <h1>Cast</h1>
        <span class="rec"><Icon name="mic" size={11} stroke={2.4} />Recording</span>
        <span class="muted s">the actor’s mic</span>
        <span class="dir"><Icon name="speaker" size={11} stroke={2.4} />Director room</span>
        <span class="muted s">talkback and notes</span>
      </div>
      <div class="legend">
        {#if v.matched_at}<span class="muted s">Matched {ago(v.matched_at, now)}</span>{/if}
        <button class="btn" onclick={redoMatch}><Icon name="sparkle" size={14} />Redo match</button>
      </div>
    </div>

    {#if v.fixes.length}
      <section class="needs" aria-label="Needs you">
        <div class="nh"><span class="bang">!</span><span>{v.fixes.length} need{v.fixes.length === 1 ? 's' : ''} you</span></div>
        {#each v.fixes as f, i (`${f.kind}:${f.source_id ?? f.character}:${i}`)}
          <div class="fix">
            {#if f.kind === 'role'}
              <span class="ellip tn name">{f.file}</span>
              <span class="muted s">{f.message}</span>
              <button class="btn sm" onclick={() => setRole(f.source_id!, 'character')}>Recording</button>
              <button class="btn sm" onclick={() => setRole(f.source_id!, 'room')}>Director room</button>
            {:else if f.kind === 'character'}
              <span class="rec small">Recording</span>
              <span class="ellip tn name">{f.file}</span>
              {#if suggestion[f.source_id!]}
                {@const sg = suggestion[f.source_id!]}
                <span class="muted s">Suggested: {label(sg.character)} · {sg.reason}</span>
                <button class="btn sm primary" onclick={() => setRole(f.source_id!, 'character', sg.character)}>Use {label(sg.character)}</button>
              {:else}
                <span class="muted s">{f.message}</span>
              {/if}
              <label class="btn sm sel">Assign<Icon name="chevron" size={11} stroke={2.4} />
                <select aria-label="Assign {f.file}" onchange={(e) => setRole(f.source_id!, 'character', (e.currentTarget as HTMLSelectElement).value)}>
                  <option value="" selected disabled>Character</option>
                  {#each characters as c (c.id)}<option value={c.id}>{c.label}</option>{/each}
                </select>
              </label>
              <button class="btn sm" onclick={() => suggest(f)}><Icon name="sparkle" size={12} />Suggest</button>
            {:else if f.kind === 'link'}
              <span class="dir small">Director room</span>
              <span class="ellip tn name">{f.file}</span>
              <span class="muted s">{f.message}</span>
              <button class="btn sm" onclick={() => openLink(f.source_id!)}>Link to<Icon name="chevron" size={11} stroke={2.4} /></button>
              {#if linking === f.source_id}
                <div class="linkpop glass" role="dialog" aria-label="Link {f.file}">
                  {#each recordings as r (r.source_id)}
                    <label class="lp"><input type="checkbox" value={r.source_id} bind:group={linkSel} /><span class="ellip">{r.label} · {r.file}</span></label>
                  {/each}
                  <div class="lpb"><button class="btn sm" onclick={() => (linking = null)}>Cancel</button><button class="btn sm primary" disabled={!linkSel.length} onclick={link}>Link</button></div>
                </div>
              {/if}
            {:else if f.kind === 'no_recording'}
              {@const c = cast.find((x) => x.id === f.character)}
              <span class="name who"><span class="dot" style="background: {c ? col(c) : 'var(--text-4)'}"></span>{f.label}</span>
              <span class="muted s">{f.message}</span>
              <button class="btn sm" onclick={() => addAudio(f.character)}>Pick file</button>
            {:else}
              <span class="ellip tn name">{f.file}</span>
              <span class="muted s">{f.message}</span>
              <button class="btn sm" onclick={relink}>Find…</button>
            {/if}
          </div>
        {/each}
      </section>
    {/if}

    <section class="cast" aria-label="Characters">
      {#each cast.filter((c) => c.recordings.length) as c (c.id)}
        <article class="ccard" data-testid="cast-card">
          <div><span class="ctag" style="background: {col(c)}">{c.label}</span></div>
          {#each c.recordings as r (r.source_id)}
            <div class="grp">
              <div class="frow main">
                <button class="rec" aria-label="Change type of {r.file}" title="Make this a director room file" onclick={() => setRole(r.source_id, 'room')}><Icon name="mic" size={11} stroke={2.4} />Recording<Icon name="chevron" size={9} stroke={3} /></button>
                <div class="mid"><div class="ellip tn fn">{r.file}</div><div class="wv"><Waveform peaks={r.peaks} fill={col(c)} /></div></div>
                <span class="muted tn len">{len(r.duration_s)}</span>
              </div>
              {#each r.rooms as rm (rm.source_id)}
                <div class="frow room">
                  <span class="conn" aria-hidden="true"></span>
                  <button class="dir" aria-label="Change type of {rm.file}" title="Make this a recording" onclick={() => setRole(rm.source_id, 'character')}><Icon name="speaker" size={11} stroke={2.4} />Director room<Icon name="chevron" size={9} stroke={3} /></button>
                  <div class="mid"><div class="ellip tn muted fn2">{rm.file}</div><div class="ellip muted tiny">{shared(r, c.label)}{rm.locked ? ' · linked by hand' : ''}</div></div>
                  <span class="muted tn len">{sync(rm.sync_s, rm.by)}</span>
                </div>
              {/each}
            </div>
          {/each}
        </article>
      {/each}
    </section>

    <div class="foot">
      <label class="rem"><input type="checkbox" bind:checked={remember} />Remember my fixes for the whole show</label>
      {#if job?.state === 'running'}
        <div class="prog" aria-label="Analysis progress"><div class="bar"><span style="width: {Math.round(job.progress * 100)}%"></span></div><span class="muted s ellip">{job.message || job.stage}</span></div>
      {:else}
        {#if job?.state === 'failed'}<span class="muted s">Analysis failed: {job.message}</span>{/if}
        <button class="btn primary big" onclick={analyze} disabled={!canAnalyze}>{v.analyzed ? 'Analyze again' : 'Start analysis'}</button>
      {/if}
    </div>
  {/if}
</main>

{#if diff}
  <Sheet title="Script v{String(diff.version).padStart(3, '0')}" width={620} onclose={() => (diff = null)}>
    <p class="muted">{diff.unchanged} lines unchanged · {diff.changed.length} changed · {diff.inserted.length} new · {diff.deleted.length} deleted{diff.takes_to_rematch ? ` · ${diff.takes_to_rematch} takes to re-match` : ''}{diff.takes_to_unmatched ? ` · ${diff.takes_to_unmatched} takes to the unmatched bin` : ''}</p>
    <div class="diff">
      {#each diff.changed as c (c.line_id)}
        <div class="dl"><span class="nm">{label(c.character)}</span><span class="old script">{c.old_text}</span><span class="new script">{c.new_text}</span></div>
      {/each}
      {#each diff.inserted as c (c.line_id)}
        <div class="dl"><span class="nm">{label(c.character)}</span><span class="new script">+ {c.new_text}</span></div>
      {/each}
      {#each diff.deleted as c (c.line_id)}
        <div class="dl"><span class="nm">{label(c.character)}</span><span class="old script">− {c.old_text}</span></div>
      {/each}
    </div>
    <div class="sb"><button class="btn" onclick={undoScript}>Undo this version</button><button class="btn primary" onclick={() => (diff = null)}>Keep</button></div>
  </Sheet>
{/if}

<style>
  .imp {
    position: absolute;
    inset: 0;
    overflow-y: auto;
    padding: 84px max(4%, calc((100% - 1160px) / 2)) 32px;
    display: flex;
    flex-direction: column;
    gap: 22px;
    box-sizing: border-box;
  }
  .tabs {
    align-self: flex-start;
  }
  .drops {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 14px;
  }
  .drop {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 18px 20px;
    border-radius: 22px;
    border: 1.5px dashed rgb(var(--ink-rgb) / 0.2);
    background: var(--glass-fill-1);
  }
  .tile {
    width: 46px;
    height: 46px;
    border-radius: 12px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--glass-fill-4);
  }
  .tile.ftn {
    background: var(--button-light-active);
    color: var(--text-on-light);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .big {
    font-size: 15px;
    font-weight: 600;
  }
  .sub {
    font-size: 12.5px;
    margin-top: 3px;
  }
  .path {
    margin-top: 8px;
    width: 100%;
    box-sizing: border-box;
  }
  .casthead {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }
  .legend {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
  }
  h1 {
    margin: 0;
    font-size: 22px;
    font-weight: 600;
  }
  .s {
    font-size: 12.5px;
  }
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    white-space: nowrap;
  }
  .btn.sm {
    height: 32px;
    padding: 0 14px;
  }
  .btn.big {
    height: 38px;
    padding: 0 18px;
    font-size: 13px;
  }
  .needs {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 14px;
    border-radius: 22px;
    box-shadow: inset 0 0 0 1.5px rgb(var(--ink-rgb) / 0.35);
  }
  .nh {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 4px 4px;
    font-size: 14px;
    font-weight: 600;
  }
  .bang {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--button-light);
    color: var(--text-on-light);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    font-weight: 800;
  }
  .fix {
    position: relative;
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 44px;
    padding: 0 6px 0 14px;
    border-radius: 14px;
    background: var(--glass-fill-2);
    flex-wrap: wrap;
  }
  .fix .name {
    flex: 1;
    min-width: 120px;
    font-size: 13px;
  }
  .who {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .rec.small,
  .dir.small {
    height: 22px;
  }
  .sel {
    position: relative;
  }
  .sel select {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }
  .linkpop {
    position: absolute;
    right: 6px;
    top: 46px;
    z-index: 5;
    width: 320px;
    max-height: 300px;
    overflow: auto;
    padding: 10px;
    border-radius: 16px;
    background: var(--panel-solid);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .lp {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    min-height: 28px;
  }
  .lpb {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding-top: 6px;
  }
  .cast {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
    align-items: start;
  }
  .ccard {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 16px;
    border-radius: 22px;
    background: var(--glass-fill-1);
  }
  .ctag {
    display: inline-flex;
    align-items: center;
    height: 26px;
    padding: 0 11px;
    border-radius: var(--radius-pill);
    font-size: 12.5px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: #16110d;
  }
  .grp {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .frow {
    display: grid;
    grid-template-columns: 118px minmax(0, 1fr) 54px;
    gap: 8px;
    align-items: center;
    min-height: 34px;
    padding: 0 8px 0 4px;
    border-radius: 10px;
  }
  .frow.main {
    background: var(--glass-fill-2);
  }
  .frow.room {
    margin-left: 18px;
    position: relative;
  }
  .conn {
    position: absolute;
    left: -12px;
    top: -6px;
    width: 9px;
    height: 22px;
    border-left: 1.5px solid rgb(var(--ink-rgb) / 0.35);
    border-bottom: 1.5px solid rgb(var(--ink-rgb) / 0.35);
    border-bottom-left-radius: 6px;
  }
  .frow .rec,
  .frow .dir {
    justify-content: center;
  }
  .mid {
    min-width: 0;
  }
  .fn {
    font-size: 12.5px;
    font-weight: 600;
  }
  .fn2 {
    font-size: 12.5px;
  }
  .tiny {
    font-size: 10.5px;
  }
  .wv {
    position: relative;
    height: 12px;
  }
  .len {
    font-size: 11px;
    text-align: right;
  }
  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
    padding-top: 16px;
    border-top: 1px solid var(--glass-outline);
  }
  .rem {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
  }
  .rem input {
    width: 16px;
    height: 16px;
    accent-color: var(--button-light);
  }
  .prog {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 320px;
  }
  .bar {
    flex: none;
    width: 200px;
    height: 6px;
    border-radius: 3px;
    background: var(--glass-fill-4);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--button-light);
    transition: width 0.2s;
  }
  .diff {
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: 50vh;
    overflow: auto;
    margin: 10px 0 16px;
  }
  .dl {
    display: grid;
    grid-template-columns: 90px 1fr;
    gap: 2px 10px;
    font-size: 13px;
  }
  .dl .old {
    grid-column: 2;
    text-decoration: line-through;
    color: var(--text-4);
  }
  .dl .new {
    grid-column: 2;
  }
  .dl .nm {
    grid-row: span 2;
    color: var(--text-3);
    padding-top: 2px;
  }
  .sb {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }
  @media (max-width: 1000px) {
    .drops,
    .cast {
      grid-template-columns: 1fr;
    }
  }
</style>
