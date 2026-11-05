<script lang="ts">
  // The Export page: one calm sheet, labels on the left, one control per row, details in popups.
  // The usual export is one look and one click. From the show home it's the show export: the
  // Range row lists episodes and their scenes, and it saves to the pipeline folder alone.
  import { onDestroy, onMount, untrack } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { call, BridgeError, inTauri, pickPath } from '../lib/bridge';
  import {
    fitFormat,
    footer,
    formatKey,
    formatLabel,
    formatsFor,
    LOUDNESS,
    MODES,
    rangeCaption,
    savedCaption,
    showsFormat,
    showsTracks,
    TRACKS,
    unchangedCaption,
    version,
    type ExportPlan,
    type ExportResult,
    type ExportScene,
    type ExportSettings,
    type ExportStatus,
    type Pipeline,
    type ShowExportStatus,
  } from '../lib/export';
  import Icon from '../components/Icon.svelte';

  let { show = false, onclose }: { show?: boolean; onclose?: () => void } = $props();

  let status = $state<ExportStatus | null>(null);
  let showStatus = $state<ShowExportStatus | null>(null);
  let local = $state<ExportSettings | null>(null);
  /** Whole episode, or the scenes picked. */
  let whole = $state(false);
  let picked = $state<string[]>([]);
  /** A show export: the scenes picked of each episode. */
  let shown = $state<Record<string, string[]>>({});
  let plan = $state<ExportPlan | null>(null);
  let planError = $state('');
  let running = $state(false);
  let progress = $state(0);
  let stage = $state('');
  let failed = $state('');
  let done = $state<{ text: string; folder: string; notes: string[] } | null>(null);
  let naming = $state(false);
  let presetName = $state('');
  let pipelinePath = $state('');
  let pipelineOpen = $state(false);

  const s = $derived(show ? local : app.exportSettings);
  const pipeline = $derived<Pipeline | null>(show ? (showStatus?.pipeline ?? null) : (status?.pipeline ?? null));
  const ffmpeg = $derived((show ? showStatus?.ffmpeg : status?.ffmpeg) ?? false);
  const scenes = $derived(status?.scenes ?? []);
  const pickedScenes = $derived(whole ? scenes : scenes.filter((x) => picked.includes(x.id)));
  const unpicked = $derived(scenes.filter((x) => !picked.includes(x.id)));
  const needsFolder = $derived(s?.save_to === 'pipeline' && !pipeline?.path);
  const formats = $derived(s ? formatsFor(s.mode, ffmpeg) : []);
  const format = $derived(s ? fitFormat(s, ffmpeg) : null);
  const mode = $derived(MODES.find((m) => m.id === s?.mode) ?? MODES[0]);
  const history = $derived(pickedScenes.flatMap((x) => x.history.map((r) => ({ scene: x, r }))).reverse());
  const showScenes = $derived(Object.values(shown).reduce((a, v) => a + v.length, 0));
  const showEpisodes = $derived(Object.values(shown).filter((v) => v.length).length);
  const canExport = $derived(!!s && !running && !needsFolder && (show ? showScenes > 0 : !!plan?.scenes.length));
  const stack = Array.from({ length: 7 }, (_, i) => `M6 ${(3 + i * 5.6).toFixed(1)} H58`).join(' ');

  async function load() {
    if (show) {
      const r = await app.run(() => call<ShowExportStatus>('show_export_status'));
      if (!r) return;
      showStatus = r;
      local ??= { ...r.defaults };
      pipelinePath = r.pipeline.proposal ?? '';
      return;
    }
    const r = await app.run(() => call<ExportStatus>('export_status'));
    if (!r) return;
    status = r;
    app.exportSettings ??= { ...r.defaults, tracks: { ...r.defaults.tracks } };
    app.exported = Object.fromEntries(r.scenes.map((x) => [x.id, x]));
    pipelinePath = r.pipeline?.proposal ?? '';
    if (!picked.length) picked = [app.sceneId].filter(Boolean);
  }

  // the footer and captions follow the choices; only the newest answer counts
  let asked = 0;
  $effect(() => {
    if (show || !s || !status) return;
    const body = { scenes: whole ? undefined : [...picked], settings: $state.snapshot(s) };
    if (!whole && !picked.length) {
      plan = null;
      return;
    }
    const n = ++asked;
    untrack(() => {
      planError = '';
      void call<ExportPlan>('export_plan', body)
        .then((p) => {
          if (n === asked) plan = p;
        })
        .catch((e: unknown) => {
          if (n !== asked) return;
          plan = null;
          planError = e instanceof BridgeError ? e.message : String(e);
        });
    });
  });

  // a format that no longer fits the mode gives way to one that does
  $effect(() => {
    if (!s || !format) return;
    if (showsFormat(s.mode) && (s.codec !== format.codec || s.sample_rate !== format.rate || s.bit_depth !== format.bits)) {
      s.codec = format.codec;
      s.sample_rate = format.rate;
      s.bit_depth = format.bits;
    }
  });

  function setFormat(key: string) {
    const f = formats.find((x) => formatKey(x) === key);
    if (!f || !s) return;
    s.codec = f.codec;
    s.sample_rate = f.rate;
    s.bit_depth = f.bits;
  }

  function setLoudness(v: string) {
    if (s) s.loudness = v === 'off' ? null : Number(v);
  }

  const addScene = (id: string) => {
    if (id) picked = scenes.filter((x) => picked.includes(x.id) || x.id === id).map((x) => x.id);
  };

  function toggleEpisode(folder: string, all: ExportScene[], on: boolean) {
    const next = { ...shown };
    if (on) next[folder] = all.filter((x) => x.has_audio).map((x) => x.id);
    else delete next[folder];
    shown = next;
  }

  function dropShown(folder: string, id: string) {
    shown = { ...shown, [folder]: shown[folder].filter((x) => x !== id) };
  }

  function addShown(folder: string, all: ExportScene[], id: string) {
    if (id) shown = { ...shown, [folder]: all.filter((x) => shown[folder].includes(x.id) || x.id === id).map((x) => x.id) };
  }

  async function confirmFolder(path: string) {
    if (!path) return;
    const r = await app.run(() => call<Pipeline>('set_pipeline_folder', { path, confirm: true }));
    if (!r) return;
    pipelineOpen = false;
    if (app.show) await app.loadShow();
    await load();
  }

  async function chooseFolder() {
    const r = await pickPath({ directory: true, title: 'The pipeline folder for deliverables' });
    if (r?.length) await confirmFolder(r[0]);
  }

  async function savePreset() {
    const name = presetName.trim();
    if (!name || !s) return;
    const r = await app.run(() => call('save_export_preset', { name, settings: $state.snapshot(s) }));
    if (!r) return;
    naming = false;
    presetName = '';
    await load();
    app.say(`Preset "${name}" saved`);
  }

  function usePreset(name: string) {
    const p = status?.presets.find((x) => x.name === name);
    if (p) app.exportSettings = structuredClone($state.snapshot(p.settings));
  }

  function sayDone(r: ExportResult) {
    const saved = r.scenes.map((x) => `Sc. ${x.number} ${version(x.version)}`).join(', ');
    const notes = [
      ...r.unchanged.map((u) => `Sc. ${u.number} is unchanged since ${version(u.since)}.`),
      ...(r.empty.length ? [`Left out, nothing in them: Sc. ${r.empty.join(', ')}.`] : []),
      ...r.warnings,
    ];
    done = { text: `Saved ${saved}`, folder: r.episode?.folder ?? r.scenes[0]?.folder ?? '', notes };
  }

  async function run() {
    if (!canExport || !s) return;
    running = true;
    failed = '';
    done = null;
    progress = 0;
    stage = 'Starting';
    const settings = $state.snapshot(s);
    const start = show
      ? call<{ job_id: string }>('start_show_export', { episodes: Object.entries(shown).filter(([, v]) => v.length).map(([episode, scenes]) => ({ episode, scenes })), settings })
      : call<{ job_id: string }>('start_export', { scenes: whole ? undefined : picked, settings });
    try {
      const { job_id } = await start;
      const j = await app.waitJob(job_id, (x) => {
        progress = x.progress;
        stage = x.message || stage;
      });
      if (j?.state === 'done') {
        const result = j.result as unknown;
        if (show) {
          const eps = (result as { episodes: { episode: string; result: ExportResult }[] }).episodes;
          const first = eps[0]?.result.scenes[0]?.folder ?? '';
          const n = eps.reduce((a, e) => a + e.result.scenes.length, 0);
          done = { text: `Saved ${n} ${n === 1 ? 'scene' : 'scenes'} to the pipeline folder`, folder: first, notes: eps.flatMap((e) => e.result.warnings) };
        } else {
          sayDone(result as ExportResult);
        }
      } else {
        failed = j?.message || 'The export failed.';
      }
    } catch (e) {
      failed = e instanceof BridgeError ? e.message : String(e);
    }
    running = false;
    await load();
    if (!show) await app.refresh();
  }

  onMount(() => {
    void load();
    if (!show) app.exportUi = { run: () => void run() };
  });
  onDestroy(() => {
    if (!show) app.exportUi = null;
  });
</script>

{#snippet chevrons()}
  <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7 9l5-5 5 5M7 15l5 5 5-5" /></svg>
{/snippet}

{#snippet tick(on: boolean, set: (v: boolean) => void, name: string, sub: string, color: string, testid: string)}
  <label class="chk">
    <input type="checkbox" checked={on} onchange={(e) => set(e.currentTarget.checked)} data-testid={testid} />
    {#if color}<span class="dot" style="background: {color}"></span>{/if}
    <span class:off={!on}>{name}</span>
    {#if sub}<span class="dim">{sub}</span>{/if}
  </label>
{/snippet}

<div class="page" class:over={show}>
  <section class="sheet-x glass" aria-label="Export">
    <div class="head">
      <h1>Export</h1>
      <span class="muted">{show ? showStatus?.name : pipeline ? `${pipeline.show} · ${status?.name ?? ''}` : (status?.name ?? '')}</span>
      {#if !show && status?.presets.length}
        <label class="pop small">
          Preset{@render chevrons()}
          <select aria-label="Preset" value="" onchange={(e) => usePreset(e.currentTarget.value)}>
            <option value="" disabled>Preset</option>
            {#each status.presets as p (p.name)}<option value={p.name}>{p.name}</option>{/each}
          </select>
        </label>
      {/if}
      {#if onclose}<button class="icb closeb" aria-label="Close" onclick={onclose}><Icon name="x" size={14} /></button>{/if}
    </div>

    {#if s}
      <div class="form">
        <div class="lab">Range</div>
        <div>
          {#if show}
            {#each showStatus?.episodes ?? [] as e (e.folder)}
              {@const on = !!shown[e.folder]}
              <div class="ep">
                <label class="chk">
                  <input type="checkbox" checked={on} disabled={!e.scenes.length} onchange={(ev) => toggleEpisode(e.folder, e.scenes, ev.currentTarget.checked)} data-testid="show-episode" />
                  <span class:off={!on}>{e.label}</span>
                  {#if !e.scenes.length}<span class="dim">no project</span>{/if}
                </label>
                {#if on}
                  <div class="chips">
                    {#each e.scenes.filter((x) => shown[e.folder].includes(x.id)) as x (x.id)}
                      <span class="pick"><span class="dim tn">Sc. {x.number}</span>{x.place}{#if x.lock}<Icon name="lock" size={10} />{/if}<button aria-label="Remove scene {x.number}" onclick={() => dropShown(e.folder, x.id)}><Icon name="x" size={12} stroke={2.4} /></button></span>
                    {/each}
                    {#if e.scenes.some((x) => !shown[e.folder].includes(x.id))}
                      <label class="pick add">
                        Add scene
                        <select aria-label="Add a scene of {e.label}" value="" onchange={(ev) => addShown(e.folder, e.scenes, ev.currentTarget.value)}>
                          <option value="" disabled>Add scene</option>
                          {#each e.scenes.filter((x) => !shown[e.folder].includes(x.id)) as x (x.id)}<option value={x.id}>Sc. {x.number} · {x.place}</option>{/each}
                        </select>
                      </label>
                    {/if}
                  </div>
                {/if}
              </div>
            {/each}
            <div class="cap tn">{showScenes} {showScenes === 1 ? 'scene' : 'scenes'} of {showEpisodes} {showEpisodes === 1 ? 'episode' : 'episodes'}</div>
          {:else}
            <div class="segx" role="group" aria-label="Range">
              <button class:on={whole} aria-pressed={whole} onclick={() => (whole = true)}>Whole episode</button>
              <button class:on={!whole} aria-pressed={!whole} onclick={() => (whole = false)}>Scenes</button>
            </div>
            {#if !whole}
              <div class="chips">
                {#each pickedScenes as x (x.id)}
                  <span class="pick" title={x.heading}><span class="dim tn">Sc. {x.number}</span>{x.place}{#if x.lock}<Icon name="lock" size={10} />{/if}<button aria-label="Remove scene {x.number}" onclick={() => (picked = picked.filter((id) => id !== x.id))}><Icon name="x" size={12} stroke={2.4} /></button></span>
                {/each}
                {#if unpicked.length}
                  <label class="pick add">
                    Add scene
                    <select aria-label="Add a scene" value="" onchange={(e) => addScene(e.currentTarget.value)} data-testid="add-scene">
                      <option value="" disabled>Add scene</option>
                      {#each unpicked as x (x.id)}<option value={x.id}>Sc. {x.number} · {x.place}</option>{/each}
                    </select>
                  </label>
                {/if}
              </div>
            {/if}
            <div class="cap tn" data-testid="range-caption">{rangeCaption(pickedScenes, scenes.length)}</div>
          {/if}
          <div class="lockrow">
            {@render tick(s.lock_after, (v) => s && (s.lock_after = v), 'Lock these scenes after export', "timing can't change", '', 'lock-after')}
          </div>
        </div>

        <div class="line"></div>

        <div class="lab">Export as</div>
        <div>
          <label class="pop">
            {mode.label}{@render chevrons()}
            <select aria-label="Export as" value={s.mode} onchange={(e) => s && (s.mode = e.currentTarget.value as ExportSettings['mode'])} data-testid="export-as">
              {#each MODES as m (m.id)}<option value={m.id}>{m.label}</option>{/each}
            </select>
          </label>
          <div class="as">
            <svg width="64" height="40" viewBox="0 0 64 40" aria-hidden="true"><path d={stack} stroke-width="3" stroke-linecap="round" fill="none" stroke="var(--text-3)" /><path d="M2 1 V39 M62 1 V39" stroke="var(--accent)" stroke-width="1.5" stroke-dasharray="2 2" /></svg>
            <span class="cap flat">{mode.caption}</span>
          </div>
        </div>

        {#if showsTracks(s.mode)}
          <div class="lab">Tracks</div>
          <div>
            <div class="two">
              {#each TRACKS as t (t.key)}
                {@render tick(s.tracks[t.key], (v) => s && (s.tracks[t.key] = v), t.name, t.sub, t.color, `track-${t.key}`)}
              {/each}
            </div>
            <div class="cap">Voices and effects stay dry. Their room reverb goes to the two room tracks.</div>
          </div>
        {/if}

        <div class="line"></div>

        {#if showsFormat(s.mode)}
          <div class="lab">Format</div>
          <div class="row">
            <label class="pop">
              {format ? formatLabel(format) : ''}{@render chevrons()}
              <select aria-label="Format" value={format ? formatKey(format) : ''} onchange={(e) => setFormat(e.currentTarget.value)} data-testid="format">
                {#each formats as f (formatKey(f))}<option value={formatKey(f)}>{formatLabel(f)}</option>{/each}
              </select>
            </label>
            {#if s.mode !== 'clips' || s.tracks.mix}
              <label class="pop">
                {LOUDNESS.find((l) => l.value === s?.loudness)?.label ?? `Mix at ${s.loudness} LUFS`}{@render chevrons()}
                <select aria-label="Loudness of the mix" value={String(s.loudness ?? 'off')} onchange={(e) => setLoudness(e.currentTarget.value)} data-testid="loudness">
                  {#each LOUDNESS as l (l.label)}<option value={String(l.value ?? 'off')}>{l.label}</option>{/each}
                </select>
              </label>
            {/if}
          </div>
        {/if}

        <div class="lab">Also export</div>
        <div class="col">
          {#if s.mode === 'fcpxml'}
            <label class="chk"><input type="checkbox" checked disabled /><span>FCPXML</span><span class="dim">for Resolve and Logic</span></label>
          {:else}
            <div class="inline">
              {@render tick(s.fcpxml, (v) => s && (s.fcpxml = v), 'FCPXML', 'for Resolve and Logic', '', 'also-fcpxml')}
              {#if s.fcpxml && s.mode === 'stems'}
                <label class="pop small">
                  {s.fcpxml_source === 'stems' ? 'From the stems' : 'From the recordings'}{@render chevrons()}
                  <select aria-label="FCPXML source" value={s.fcpxml_source} onchange={(e) => s && (s.fcpxml_source = e.currentTarget.value as ExportSettings['fcpxml_source'])}>
                    <option value="original">From the recordings</option>
                    <option value="stems">From the stems</option>
                  </select>
                </label>
              {/if}
            </div>
          {/if}
          {@render tick(s.subtitles, (v) => s && (s.subtitles = v), 'Subtitles', 'SRT', '', 'also-srt')}
          <div class="inline">
            {@render tick(s.cut_list !== null, (v) => s && (s.cut_list = v ? 'csv' : null), 'Cut list', s.cut_list === null ? 'CSV' : '', '', 'also-cutlist')}
            {#if s.cut_list}
              <label class="pop small">
                {s.cut_list === 'md' ? 'Markdown' : 'CSV'}{@render chevrons()}
                <select aria-label="Cut list format" value={s.cut_list} onchange={(e) => s && (s.cut_list = e.currentTarget.value as 'csv' | 'md')}>
                  <option value="csv">CSV</option>
                  <option value="md">Markdown</option>
                </select>
              </label>
            {/if}
          </div>
        </div>

        <div class="lab">Save to</div>
        <div>
          {#if show}
            <span class="pathline">{pipeline?.path ?? 'Pipeline folder'}</span>
          {:else}
            <label class="pop">
              {s.save_to === 'pipeline' ? `Pipeline folder · ${status?.episode ?? ''}` : `Episode exports · ${status?.episode ?? ''}`}{@render chevrons()}
              <select aria-label="Save to" value={s.save_to} onchange={(e) => s && (s.save_to = e.currentTarget.value as ExportSettings['save_to'])} data-testid="save-to">
                <option value="episode">Episode exports · {status?.episode ?? ''}</option>
                {#if pipeline}<option value="pipeline">Pipeline folder · {status?.episode ?? ''}</option>{/if}
              </select>
            </label>
          {/if}
          {#if needsFolder || pipelineOpen}
            <div class="confirm" data-testid="pipeline-confirm">
              <p>{pipeline?.proposal ? 'Is this where deliverables go? Nothing is written until you confirm it.' : 'Choose the folder deliverables go to. It is asked once, and kept in the show.'}</p>
              {#if inTauri}
                {#if pipeline?.proposal}<code>{pipeline.proposal}</code>{/if}
              {:else}
                <input class="field" aria-label="Pipeline folder" bind:value={pipelinePath} placeholder="/path/to/the/pipeline/Sound" data-testid="pipeline-path" />
              {/if}
              <div class="row">
                {#if inTauri}<button class="btnx" onclick={chooseFolder}>Choose…</button>{/if}
                {#if inTauri ? pipeline?.proposal : pipelinePath}<button class="btnx pri" onclick={() => confirmFolder(inTauri ? (pipeline?.proposal ?? '') : pipelinePath)} data-testid="pipeline-use">Use this folder</button>{/if}
              </div>
            </div>
          {:else if s.save_to === 'pipeline' && pipeline?.path}
            <div class="cap">{pipeline.path} · <button class="link" onclick={() => (pipelineOpen = true)}>Change</button></div>
          {/if}
          {#if plan && !needsFolder}
            <div class="cap tn" data-testid="saved-caption">{savedCaption(plan)}</div>
            {#if unchangedCaption(plan)}<div class="cap tn note" data-testid="unchanged-caption">{unchangedCaption(plan)}</div>{/if}
            {#if plan.empty.length}<div class="cap tn">Left out, nothing in them: Sc. {plan.empty.join(', ')}.</div>{/if}
          {/if}
          {#if planError && !needsFolder}<div class="cap note">{planError}</div>{/if}
          {#each status?.warnings ?? [] as w (w)}<div class="cap note" data-testid="export-warning">{w}</div>{/each}
        </div>

        {#if history.length}
          <div class="lab">History</div>
          <details>
            <summary class="tn">{history.length} {history.length === 1 ? 'version' : 'versions'}</summary>
            <ul>
              {#each history as h (h.r.folder)}
                <li class="tn">
                  <span class="ellip">Sc. {h.scene.number} {version(h.r.version)} · {h.r.date.slice(0, 10)} · {h.r.destination === 'pipeline' ? 'Pipeline folder' : 'Episode exports'}</span>
                  <button class="link" onclick={() => app.reveal(h.r.folder)}>Reveal</button>
                </li>
              {/each}
            </ul>
          </details>
        {/if}
      </div>

      <div class="foot">
        {#if running}
          <div class="bar" role="progressbar" aria-valuemin="0" aria-valuemax="1" aria-valuenow={progress} aria-label="Export"><span style="width: {(progress * 100).toFixed(1)}%"></span></div>
          <span class="mut tn grow ellip" data-testid="export-stage">{stage}</span>
        {:else if failed}
          <span class="fail grow" role="alert" data-testid="export-failed">{failed}</span>
        {:else if done}
          <span class="mut tn grow ellip" data-testid="export-done">{done.text}{#if done.notes.length}. {done.notes.join(' ')}{/if}</span>
          {#if done.folder}<button class="link" onclick={() => done && app.reveal(done.folder)}>Reveal</button>{/if}
        {:else if show}
          <span class="mut tn grow">{showScenes} {showScenes === 1 ? 'scene' : 'scenes'} to the pipeline folder</span>
        {:else}
          <span class="mut tn grow" data-testid="export-size">{plan ? footer(plan) : ''}</span>
        {/if}
        {#if naming}
          <!-- svelte-ignore a11y_autofocus -->
          <input class="field name" aria-label="Preset name" placeholder="Preset name" autofocus bind:value={presetName} onkeydown={(e) => { e.stopPropagation(); if (e.key === 'Enter') void savePreset(); if (e.key === 'Escape') naming = false; }} data-testid="preset-name" />
          <button class="btnx" onclick={savePreset} disabled={!presetName.trim()}>Save</button>
        {:else if !show}
          <button class="btnx" onclick={() => (naming = true)} disabled={running} data-testid="save-preset">Save preset</button>
        {/if}
        <button class="btnx pri" onclick={run} disabled={!canExport} data-testid="export-go">Export{#if !show}<span class="key">⌘E</span>{/if}</button>
      </div>
    {/if}
  </section>
</div>

<style>
  .page {
    position: absolute;
    inset: 0;
    overflow-y: auto;
    padding: 92px 16px 40px;
    box-sizing: border-box;
  }
  .page.over {
    z-index: 30;
    background: var(--scrim-light);
  }
  .sheet-x {
    width: 640px;
    max-width: 100%;
    margin: 0 auto;
    box-sizing: border-box;
    padding: 30px 34px 26px;
    border-radius: var(--radius-sheet);
    background: var(--panel);
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 12px;
    margin-bottom: 26px;
  }
  .closeb,
  .head .pop {
    margin-left: auto;
    align-self: center;
  }
  h1 {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .muted,
  .mut {
    font-size: 12.5px;
    color: var(--text-3);
  }
  .dim {
    color: var(--text-5);
    font-size: 12px;
    font-weight: 500;
  }
  .form {
    display: grid;
    grid-template-columns: 132px minmax(0, 1fr);
    column-gap: 20px;
    row-gap: 18px;
    align-items: start;
  }
  .lab {
    font-size: 13px;
    color: var(--text-3);
    text-align: right;
    padding-top: 7px;
  }
  .line {
    height: 1px;
    background: var(--glass-outline);
    grid-column: 1 / -1;
  }
  .segx {
    display: inline-flex;
    padding: 2px;
    border-radius: 9px;
    background: var(--glass-fill-4);
  }
  .segx button {
    height: 28px;
    padding: 0 14px;
    border-radius: 7px;
    font-size: 13px;
    font-weight: 500;
    color: var(--text-3);
  }
  .segx button.on {
    background: var(--glass-active);
    color: var(--text-strong);
    font-weight: 600;
    box-shadow: 0 1px 2px rgb(var(--night-rgb) / 0.3);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 10px;
  }
  .pick {
    position: relative;
    height: 28px;
    padding: 0 8px 0 11px;
    border-radius: var(--radius-pill);
    display: inline-flex;
    align-items: center;
    gap: 7px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-strong);
    background: var(--glass-hover);
  }
  .pick button {
    display: flex;
    color: var(--text-4);
  }
  .pick button:hover {
    color: var(--text-1);
  }
  .pick.add {
    padding: 0 11px;
    background: none;
    box-shadow: inset 0 0 0 1px rgb(var(--veil-rgb) / 0.14);
    color: var(--text-3);
  }
  .pop {
    position: relative;
    height: 32px;
    padding: 0 10px 0 12px;
    border-radius: 8px;
    display: inline-flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    font-weight: 500;
    color: var(--text-strong);
    background: var(--glass-hover);
    box-shadow: inset 0 0 0 1px var(--glass-outline);
  }
  .pop.small {
    height: 24px;
    font-size: 12px;
    padding: 0 8px 0 10px;
    gap: 6px;
  }
  .pop :global(svg) {
    opacity: 0.6;
  }
  /* the popups are the system's: invisible over the label, opened by it */
  .pop select,
  .pick select {
    position: absolute;
    inset: 0;
    width: 100%;
    opacity: 0;
    cursor: pointer;
  }
  .pop:focus-within,
  .pick:focus-within {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .chk {
    display: flex;
    align-items: center;
    gap: 9px;
    font-size: 13px;
    color: var(--text-strong);
    height: 26px;
    min-width: 0;
  }
  .chk .off {
    color: var(--text-4);
  }
  .chk input {
    appearance: none;
    -webkit-appearance: none;
    flex: none;
    width: 16px;
    height: 16px;
    margin: 0;
    border-radius: 5px;
    box-shadow: inset 0 0 0 1.5px rgb(var(--veil-rgb) / 0.3);
    background: center / 11px no-repeat;
    cursor: pointer;
  }
  .chk input:checked {
    box-shadow: none;
    background-color: var(--accent);
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='%2316131F' stroke-width='3.6' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M5 12l5 5 9-10'/%3E%3C/svg%3E");
  }
  .chk input:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .chk input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex: none;
  }
  .cap {
    font-size: 12px;
    color: var(--text-4);
    line-height: 1.45;
    margin-top: 7px;
  }
  .cap.flat {
    margin-top: 0;
  }
  .cap.note {
    color: var(--warn);
  }
  .lockrow {
    margin-top: 6px;
  }
  .as {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-top: 10px;
  }
  .as svg {
    flex: none;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    column-gap: 20px;
  }
  .row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .col {
    display: flex;
    flex-direction: column;
  }
  .inline {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .ep + .ep {
    margin-top: 8px;
  }
  .ep .chips {
    margin: 4px 0 2px 25px;
  }
  .pathline {
    font-size: 13px;
    color: var(--text-2);
    word-break: break-all;
    display: block;
    padding-top: 7px;
  }
  .confirm {
    margin-top: 10px;
    padding: 12px 14px;
    border-radius: 12px;
    background: var(--glass-fill-3);
    font-size: 12.5px;
    color: var(--text-2);
  }
  .confirm p {
    margin: 0 0 8px;
    line-height: 1.45;
  }
  .confirm code {
    display: block;
    margin-bottom: 10px;
    font-size: 12px;
    word-break: break-all;
    color: var(--text-1);
  }
  .confirm .field {
    width: 100%;
    margin-bottom: 10px;
    box-sizing: border-box;
  }
  details summary {
    font-size: 13px;
    color: var(--text-2);
    padding-top: 7px;
    cursor: pointer;
  }
  ul {
    list-style: none;
    margin: 6px 0 0;
    padding: 0;
    max-height: 160px;
    overflow-y: auto;
  }
  li {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    font-size: 12px;
    color: var(--text-3);
    padding: 3px 0;
  }
  .link {
    flex: none;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-1);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 28px;
    padding-top: 20px;
    border-top: 1px solid var(--glass-outline);
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .fail {
    font-size: 12.5px;
    line-height: 1.4;
    color: var(--danger, var(--warn));
  }
  .bar {
    flex: none;
    width: 120px;
    height: 4px;
    border-radius: 2px;
    background: var(--glass-active);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 200ms linear;
  }
  .btnx {
    flex: none;
    height: 34px;
    padding: 0 16px;
    border-radius: 9px;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-strong);
    background: var(--glass-hover);
  }
  .btnx.pri {
    background: var(--accent);
    color: var(--on-accent);
  }
  .btnx:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .key {
    display: inline-flex;
    align-items: center;
    height: 18px;
    padding: 0 5px;
    border-radius: 5px;
    background: rgb(22 19 31 / 0.16);
    font-size: 10.5px;
    font-weight: 600;
  }
  .name {
    width: 150px;
    height: 34px;
  }
  @media (max-width: 620px) {
    .form {
      grid-template-columns: minmax(0, 1fr);
      row-gap: 8px;
    }
    .lab {
      text-align: left;
      padding-top: 10px;
    }
    .two {
      grid-template-columns: 1fr;
    }
  }
</style>
