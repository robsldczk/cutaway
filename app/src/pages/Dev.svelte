<script lang="ts">
  // Dev panel (behind the developer switch): one actor session under the hood. Three
  // columns scroll together: the full transcript (session mic and director mic as
  // sentences), what Claude rescued, and where each sentence sits in the script. Select
  // session-mic sentences to link them to a script line and send them to Claude.
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import { fmt } from '../lib/format';
  import type { Job } from '../lib/types';
  import { MARK_KINDS, splitMatches, type DevLine, type DevRow, type DevSession, type DevSessionItem, type MarkKind } from '../lib/dev';
  import DevMarks from '../components/DevMarks.svelte';
  import DevLearn from '../components/DevLearn.svelte';

  type Filter = 'all' | 'takes' | 'rescued' | 'near' | 'unmatched' | 'director' | 'marked';
  const FILTERS: { id: Filter; label: string }[] = [
    { id: 'all', label: 'All' },
    { id: 'takes', label: 'Takes' },
    { id: 'rescued', label: 'Rescued' },
    { id: 'near', label: 'Near misses' },
    { id: 'unmatched', label: 'Unmatched' },
    { id: 'director', label: 'Director' },
    { id: 'marked', label: 'Marked' },
  ];

  let tab = $state<'hood' | 'learn'>('hood');
  let sessions = $state<DevSessionItem[]>([]);
  let sourceId = $state('');
  let s = $state<DevSession | null>(null);
  let q = $state('');
  let filter = $state<Filter>('all');
  let sceneFilter = $state('');
  let sel = $state<string[]>([]);
  let lastSel = $state<string | null>(null);
  let playing = $state<string | null>(null);
  let job = $state<Job | null>(null);
  // link bar
  let kind = $state<MarkKind>('missed_take');
  let lineId = $state('');
  let note = $state('');

  async function loadSessions() {
    const r = await app.run(() => call<{ sessions: DevSessionItem[] }>('dev_sessions'));
    sessions = r?.sessions ?? [];
    if (!sourceId && sessions.length) sourceId = sessions[0].source_id;
  }
  async function load() {
    if (!sourceId) return;
    s = (await app.run(() => call<DevSession>('dev_session', { source_id: sourceId }))) ?? null;
  }
  $effect(() => {
    void app.status?.rev;
    void loadSessions();
  });
  $effect(() => {
    void sourceId;
    void app.status?.rev;
    void load();
  });

  const lineById = $derived(
    new Map((s?.scenes ?? []).flatMap((sc) => sc.lines.map((l) => [l.id, { line: l, scene: sc.number }] as [string, { line: DevLine; scene: number }]))),
  );
  const markById = $derived(new Map((s?.marks ?? []).map((m) => [m.id, m])));
  /** Scene of each row: its line's, else carried over from the row before. */
  const sceneOf = $derived.by(() => {
    let cur = 0;
    const pairs: [string, number][] = (s?.rows ?? []).map((r) => {
      const hit = r.status.line_id ? lineById.get(r.status.line_id) : undefined;
      if (hit) cur = hit.scene;
      return [r.id, cur];
    });
    return new Map(pairs);
  });
  const scenesSeen = $derived([...new Set(sceneOf.values())].filter((n) => n > 0).sort((a, b) => a - b));
  const rows = $derived.by(() => {
    const needle = q.trim().toLowerCase();
    return (s?.rows ?? []).filter((r) => {
      if (sceneFilter && String(sceneOf.get(r.id)) !== sceneFilter) return false;
      if (needle && !r.text.toLowerCase().includes(needle) && !(r.status.read_as ?? '').toLowerCase().includes(needle)) return false;
      switch (filter) {
        case 'takes': return r.status.kind === 'take';
        case 'rescued': return r.status.kind === 'take' && r.status.origin !== 'matcher';
        case 'near': return r.status.kind === 'near_miss';
        case 'unmatched': return r.status.kind === 'unmatched';
        case 'director': return r.mic === 'director';
        case 'marked': return r.marks.length > 0;
        default: return true;
      }
    });
  });
  const counts = $derived.by(() => {
    const c = { take: 0, rescue: 0, manual: 0, near: 0, unmatched: 0 };
    for (const r of s?.rows ?? []) {
      if (r.status.kind === 'take') {
        if (r.status.origin === 'rescue') c.rescue++;
        else if (r.status.origin === 'manual' || r.status.origin === 'marked') c.manual++;
        else c.take++;
      } else if (r.status.kind === 'near_miss') c.near++;
      else if (r.status.kind === 'unmatched') c.unmatched++;
    }
    return c;
  });
  const selRows = $derived((s?.rows ?? []).filter((r) => sel.includes(r.id)));
  const ownLines = $derived((s?.scenes ?? []).map((sc) => ({ sc, lines: sc.lines.filter((l) => l.own) })).filter((g) => g.lines.length));

  function rowClass(r: DevRow): string {
    const st = r.status;
    if (r.mic === 'director') return 'k-dir';
    if (st.kind === 'take') return st.origin === 'matcher' ? 'k-take' : st.origin === 'rescue' ? 'k-resc' : 'k-man';
    if (st.kind === 'near_miss') return 'k-near';
    if (st.kind === 'bleed') return 'k-bleed';
    return 'k-un';
  }

  function toggle(r: DevRow, e: MouseEvent) {
    if (r.mic !== 'session') return;
    const all = rows.filter((x) => x.mic === 'session').map((x) => x.id);
    if (e.shiftKey && lastSel) {
      const a = all.indexOf(lastSel);
      const b = all.indexOf(r.id);
      if (a >= 0 && b >= 0) {
        const range = all.slice(Math.min(a, b), Math.max(a, b) + 1);
        sel = [...new Set([...sel, ...range])];
        lastSel = r.id;
        return;
      }
    }
    sel = sel.includes(r.id) ? sel.filter((x) => x !== r.id) : [...sel, r.id];
    lastSel = r.id;
    // suggest the line: the near miss or take of the first selected sentence
    const first = (s?.rows ?? []).find((x) => sel.includes(x.id));
    const guess = first?.status.line_id;
    if (guess && lineById.get(guess)?.line.own && !lineId) lineId = guess;
  }

  async function play(r: DevRow) {
    if (playing === r.id) {
      await app.run(() => call('stop'));
      playing = null;
      return;
    }
    await app.run(() => call('play_source', { source_id: r.source_id, t0: r.t0, t1: r.t1 }));
    playing = r.id;
    const len = (r.t1 - r.t0) * 1000 + 200;
    setTimeout(() => {
      if (playing === r.id) playing = null;
    }, len);
  }

  async function runJob(name: string, params: object, done: string) {
    const r = await app.run(() => call<{ job_id: string }>(name, params));
    if (!r) return;
    const j = await app.waitJob(r.job_id, (x) => (job = x));
    job = null;
    if (j?.state === 'failed') app.say(j.message || 'Failed');
    else app.say(done);
    await load();
    await loadSessions();
    await app.refresh();
  }

  async function saveMark(send: 'no' | 'claude' | 'manual') {
    if (!selRows.length || !s) return;
    const t0 = Math.min(...selRows.map((r) => r.t0));
    const t1 = Math.max(...selRows.map((r) => r.t1));
    const r = await app.run(() =>
      call<{ mark_id: string }>('put_mark', { source_id: s!.source_id, t0, t1, kind, line_id: lineId || null, note: note.trim() }),
    );
    if (!r) return;
    sel = [];
    note = '';
    if (send === 'no') {
      app.say('Marked');
      await load();
    } else {
      await runJob('send_marks', { mark_ids: [r.mark_id], manual: send === 'manual' }, send === 'manual' ? 'Proposal ready: linked by hand' : 'Claude answered: proposal ready');
    }
  }

  async function rerunCharacter() {
    if (!s) return;
    await runJob('start_analysis', { characters: [s.character] }, `${s.character} analysed again`);
  }
  async function rerunLine(id: string) {
    await runJob('start_analysis', { line_ids: [id] }, 'Line analysed again');
  }

  function onKey(e: KeyboardEvent) {
    const t = e.target as HTMLElement | null;
    if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT')) return;
    if (e.key === 'Escape') sel = [];
    if (e.key === ' ' && playing) {
      e.preventDefault();
      void app.run(() => call('stop'));
      playing = null;
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="dev" data-testid="dev-page">
  <div class="top">
    <div class="seg" role="tablist" aria-label="Dev view">
      <button role="tab" aria-selected={tab === 'hood'} class:on={tab === 'hood'} onclick={() => (tab = 'hood')}>Under the hood</button>
      <button role="tab" aria-selected={tab === 'learn'} class:on={tab === 'learn'} onclick={() => (tab = 'learn')}>Learn</button>
    </div>
    <select class="field" aria-label="Actor session" bind:value={sourceId}>
      {#each sessions as x (x.source_id)}
        <option value={x.source_id}>{x.character} · {x.name} ({x.takes.matcher} matched, {x.takes.rescue} rescued{x.takes.manual ? `, ${x.takes.manual} by hand` : ''})</option>
      {/each}
    </select>
    {#if tab === 'hood'}
      <input class="field search" type="search" placeholder="Search the transcript" bind:value={q} aria-label="Search" />
      <select class="field" aria-label="Scene" bind:value={sceneFilter}>
        <option value="">All scenes</option>
        {#each scenesSeen as n (n)}<option value={String(n)}>Scene {n}</option>{/each}
      </select>
      <button class="btn" disabled={!s || !!job} onclick={rerunCharacter} title="Analyse this character's session again; the rest of the episode stays as it is">Rerun {s?.character ?? 'character'}</button>
    {/if}
    {#if job}<span class="job muted">{job.message || job.stage}…</span>{/if}
  </div>

  {#if tab === 'learn'}
    <DevLearn session={s} {sessions} reload={load} open={(n) => { tab = 'hood'; filter = 'all'; sceneFilter = String(n); }} />
  {:else if s}
    <div class="chips" role="group" aria-label="Filter">
      {#each FILTERS as f (f.id)}<button class:on={filter === f.id} onclick={() => (filter = f.id)}>{f.label}</button>{/each}
      <span class="legend muted">
        sentences: <i class="sw k-take"></i>{counts.take} in matched takes <i class="sw k-resc"></i>{counts.rescue} rescued <i class="sw k-man"></i>{counts.manual} by hand <i class="sw k-near"></i>{counts.near} near misses <i class="sw k-un"></i>{counts.unmatched} unmatched · director mic: {s.director.join(', ') || 'none'}
      </span>
    </div>
    <div class="body">
      <div class="table" role="table" aria-label="Transcript, rescue and script">
        <div class="hd" role="row">
          <span role="columnheader">Full transcript</span>
          <span role="columnheader">Rescued by Claude</span>
          <span role="columnheader">Script position</span>
        </div>
        <div class="rows">
          {#each rows as r (r.id)}
            {@const hit = r.status.line_id ? lineById.get(r.status.line_id) : undefined}
            <div class="row {rowClass(r)}" class:sel={sel.includes(r.id)} class:marked={r.marks.length > 0} role="row">
              <div class="c tx" role="cell">
                {#if r.mic === 'session'}
                  <input type="checkbox" aria-label="Select {r.id}" checked={sel.includes(r.id)} onclick={(e) => toggle(r, e)} />
                {:else}<span class="nochk" title="The director mic is notes only, never a take"></span>{/if}
                <button class="play" class:on={playing === r.id} onclick={() => play(r)} aria-label="Play {r.id}">{playing === r.id ? '■' : '▶'}</button>
                <span class="t">{fmt(r.t0)}</span>
                <span class="who">{r.mic === 'director' ? 'DIR' : r.speaker}</span>
                <span class="txt">{#each splitMatches(r.text, q.trim()) as p, i (i)}{#if p.hit}<mark>{p.t}</mark>{:else}{p.t}{/if}{/each}</span>
                {#if r.mic === 'director' && r.status.class}<span class="tag">{r.status.class.replace('_', ' ')}</span>{/if}
                {#if r.status.kind === 'bleed'}<span class="tag">bleed</span>{/if}
              </div>
              <div class="c rs" role="cell">
                {#if r.status.kind === 'take' && r.status.origin === 'rescue'}
                  <span class="lbl">rescued</span>{#if r.status.read_as}<span class="read">“{r.status.read_as}”</span>{/if}
                {:else if r.status.kind === 'take' && (r.status.origin === 'manual' || r.status.origin === 'marked')}
                  <span class="lbl">linked by hand</span>
                {:else if r.status.kind === 'near_miss'}
                  <span class="lbl">near miss · {r.status.score}%</span>
                {/if}
                {#each r.marks as mid (mid)}{@const m = markById.get(mid)}{#if m}<span class="mk" title={m.note}>mark · {m.status}</span>{/if}{/each}
              </div>
              <div class="c sp" role="cell">
                {#if hit}
                  <span class="pos">Sc. {hit.scene} · {hit.line.label}{#if r.status.take_no} · take {r.status.take_no}{/if}{#if r.status.kind === 'near_miss'} ?{/if}{#if r.status.kind === 'take'}<button class="mini" disabled={!!job} onclick={() => rerunLine(hit.line.id)} title="Analyse this line's takes again" aria-label="Rerun this line">↻</button>{/if}</span>
                  <span class="ln">{hit.line.text}</span>
                {/if}
              </div>
            </div>
          {:else}
            <p class="muted pad">Nothing matches.</p>
          {/each}
        </div>
      </div>
      <aside class="side">
        {#if selRows.length}
          <div class="link glass" data-testid="link-bar">
            <div class="lt">{selRows.length} sentence{selRows.length > 1 ? 's' : ''} · {fmt(Math.min(...selRows.map((r) => r.t0)))}</div>
            <div class="quote">“{selRows.map((r) => r.text).join(' ')}”</div>
            <label class="lab" for="mk-kind">What is it</label>
            <select id="mk-kind" class="field" bind:value={kind}>
              {#each MARK_KINDS as k (k.id)}<option value={k.id}>{k.label}</option>{/each}
            </select>
            <label class="lab" for="mk-line">Script line</label>
            <select id="mk-line" class="field" bind:value={lineId}>
              <option value="">(let Claude find it)</option>
              {#each ownLines as g (g.sc.id)}
                <optgroup label="Scene {g.sc.number} · {g.sc.heading}">
                  {#each g.lines as l (l.id)}<option value={l.id}>{l.label} {l.text.slice(0, 60)}</option>{/each}
                </optgroup>
              {/each}
            </select>
            <label class="lab" for="mk-note">Note for Claude</label>
            <textarea id="mk-note" class="field" rows="2" placeholder="e.g. misheard, she changed 'bit' to 'tad'" bind:value={note}></textarea>
            <div class="acts">
              <button class="btn primary" disabled={!!job} onclick={() => saveMark('claude')}>Send to Claude</button>
              <button class="btn" disabled={!!job || !lineId} onclick={() => saveMark('manual')} title="Make a take of the linked line as marked, without asking Claude">Make take</button>
              <button class="btn" disabled={!!job} onclick={() => saveMark('no')}>Just mark</button>
              <button class="btn" onclick={() => (sel = [])}>Clear</button>
            </div>
          </div>
        {:else}
          <p class="muted hint">Tick session-mic sentences (⇧-click for a range) to link them to a script line and send them to Claude. Director-mic sentences are notes only.</p>
        {/if}
        <DevMarks marks={s.marks} {lineById} busy={!!job} onsend={(id, manual) => runJob('send_marks', { mark_ids: [id], manual }, 'Proposal ready')} reload={load} />
      </aside>
    </div>
  {:else}
    <p class="muted pad">{sessions.length ? 'Loading…' : 'No character sessions yet: import recordings and run the analysis.'}</p>
  {/if}
</div>

<style>
  .dev {
    position: absolute;
    inset: 0;
    padding: 76px 20px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    box-sizing: border-box;
    font-size: var(--fs-ui);
  }
  .top {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    align-items: center;
  }
  .search {
    min-width: 220px;
    flex: 1;
  }
  .job {
    font-size: 12px;
  }
  .seg {
    display: inline-flex;
    padding: 3px;
    border-radius: var(--radius-pill);
    background: var(--glass-fill-2);
  }
  .seg button {
    height: 28px;
    padding: 0 14px;
    border-radius: var(--radius-pill);
    font-size: 12px;
    color: var(--text-3);
  }
  .seg button.on {
    background: var(--button-light-active);
    color: var(--text-on-light);
    font-weight: 600;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .chips button {
    height: 26px;
    padding: 0 12px;
    border-radius: var(--radius-pill);
    background: var(--glass-fill-1);
    color: var(--text-3);
    font-size: 12px;
  }
  .chips button.on {
    background: var(--glass-fill-4);
    color: var(--text-strong);
  }
  .legend {
    margin-left: auto;
    font-size: 11.5px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .sw {
    display: inline-block;
    width: 9px;
    height: 9px;
    border-radius: 3px;
    margin-left: 6px;
  }
  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 340px;
    gap: 14px;
  }
  .table {
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-panel);
    background: var(--glass-fill-1);
    box-shadow: inset 0 0 0 1px var(--glass-outline);
    overflow: hidden;
  }
  .hd,
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1.25fr) minmax(0, 0.8fr) minmax(0, 1fr);
  }
  .hd {
    padding: 8px 12px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-4);
    border-bottom: 1px solid var(--glass-outline);
  }
  .rows {
    overflow-y: auto;
    flex: 1;
  }
  .row {
    padding: 0 12px;
    border-left: 3px solid transparent;
    border-bottom: 1px solid rgb(var(--ink-rgb) / 0.05);
  }
  .row.sel {
    background: rgb(var(--locator-rgb) / 0.12);
  }
  .row.marked .tx {
    text-decoration: underline dotted rgb(var(--locator-rgb) / 0.7);
    text-underline-offset: 3px;
  }
  .row.k-take,
  .sw.k-take {
    border-left-color: var(--ok);
    background-color: var(--ok);
  }
  .row.k-resc,
  .sw.k-resc {
    border-left-color: var(--sfx);
    background-color: var(--sfx);
  }
  .row.k-man,
  .sw.k-man {
    border-left-color: var(--locator);
    background-color: var(--locator);
  }
  .row.k-near,
  .sw.k-near {
    border-left-color: var(--warn);
    background-color: var(--warn);
  }
  .sw.k-un {
    background-color: var(--text-4);
  }
  .row.k-take,
  .row.k-resc,
  .row.k-man,
  .row.k-near {
    background-color: transparent;
  }
  .row.sel {
    background-color: rgb(var(--locator-rgb) / 0.12);
  }
  .row.k-dir .txt,
  .row.k-dir .who {
    color: var(--text-4);
    font-style: italic;
  }
  .row.k-bleed .txt {
    color: var(--text-4);
  }
  .c {
    padding: 6px 8px 6px 0;
    display: flex;
    gap: 7px;
    align-items: baseline;
    min-width: 0;
  }
  .tx input {
    flex: none;
    margin: 0;
    align-self: center;
  }
  .nochk {
    width: 13px;
    flex: none;
  }
  .play {
    flex: none;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    font-size: 9px;
    color: var(--text-3);
    background: var(--glass-fill-2);
    align-self: center;
  }
  .play.on {
    color: var(--text-on-light);
    background: var(--button-light-active);
  }
  .t {
    flex: none;
    color: var(--text-4);
    font-variant-numeric: tabular-nums;
    font-size: 11.5px;
  }
  .who {
    flex: none;
    font-size: 10.5px;
    letter-spacing: 0.05em;
    color: var(--text-3);
    width: 64px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .txt {
    color: var(--text-2);
    line-height: 1.4;
  }
  mark {
    background: rgb(var(--highlight-rgb) / 0.45);
    color: inherit;
    border-radius: 2px;
  }
  .tag,
  .mk {
    flex: none;
    font-size: 10.5px;
    padding: 1px 6px;
    border-radius: var(--radius-chip);
    box-shadow: inset 0 0 0 1px var(--text-5);
    color: var(--text-3);
  }
  .rs,
  .sp {
    flex-wrap: wrap;
    font-size: 12px;
  }
  .lbl {
    color: var(--text-3);
  }
  .read {
    color: var(--text-1);
  }
  .pos {
    flex: none;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .ln {
    color: var(--text-2);
    min-width: 0;
  }
  .mini {
    margin-left: 6px;
    font-size: 12px;
    color: var(--text-4);
  }
  .mini:hover {
    color: var(--text-strong);
  }
  .side {
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .link {
    padding: 14px;
    border-radius: var(--radius-panel);
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  .lt {
    font-weight: 600;
  }
  .quote {
    color: var(--text-2);
    font-size: 12.5px;
    line-height: 1.4;
    max-height: 90px;
    overflow-y: auto;
  }
  .lab {
    font-size: 11px;
    color: var(--text-4);
    margin-top: 4px;
  }
  textarea.field {
    resize: vertical;
    font: inherit;
  }
  .acts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 6px;
  }
  .hint {
    font-size: 12px;
    line-height: 1.45;
  }
  .pad {
    padding: 16px;
  }
</style>
