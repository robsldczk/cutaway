<script lang="ts">
  // Import › Sounds (Round 2): the full bucket. Buckets are named groups whose keywords are
  // the search; gold = downloaded, outlined grey = preview. Click a bucket for its panel.
  // Spot script (AI, script only) runs when asked; the search bar searches Epidemic Sound
  // freely and its results drag onto a bucket or onto "New bucket".
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import Icon from '../components/Icon.svelte';
  import Waveform from '../components/Waveform.svelte';
  import BucketPanel from '../components/BucketPanel.svelte';
  import KeywordField from '../components/KeywordField.svelte';
  import ImportTabs from '../components/ImportTabs.svelte';
  import EpidemicSearch from '../components/EpidemicSearch.svelte';
  import { EPIDEMIC_DRAG, type Bucket, type BucketsView, type Preview, type SearchPage, type SearchResult } from '../lib/types';
  import { colorOf, solid } from '../theme';

  let sort = $state<'scene' | 'tag' | 'show'>('scene');
  let data = $state<BucketsView | null>(null);
  let creating = $state(false);
  let spotting = $state(false);
  let spotMsg = $state('');
  let spotMenu = $state(false);
  let q = $state('');
  let found = $state<{ query: string; results: SearchResult[]; next: number; more: boolean } | null>(null);
  let searching = $state(false);
  let dragging = $state(false);
  /** The drop target under a dragged search result: a bucket id, or 'new'. */
  let over = $state<string | null>(null);
  let nb = $state({ name: '', keywords: [] as string[], min: '', max: '', exclude: 'music, voices', scenes: '' });

  async function load() {
    data = (await app.run(() => call<BucketsView>('buckets', { sort }))) ?? data;
  }
  $effect(() => {
    void sort;
    void app.status?.rev;
    void load();
  });

  const open = $derived(data?.buckets.find((b) => b.id === app.openBucket) ?? null);
  /** A side panel is showing (a bucket, or the search results). */
  const side = $derived(!!open || !!found);
  const spot = $derived(data?.spot ?? { scenes: 0, pending: 0 });
  const spotted = $derived(spot.scenes - spot.pending);
  const sceneNo = $derived(app.view?.scene.number);

  function tiles(b: Bucket) {
    const kept = b.kept.map((k) => ({ kept: true, peaks: k.peaks, key: k.id }));
    const prev = b.previews.map((p) => ({ kept: false, peaks: pseudoPeaks(p.remote_id), key: p.remote_id }));
    return [...kept, ...prev].slice(0, 4);
  }

  /** Previews have no peaks until downloaded: a quiet stand-in shape from the id. */
  function pseudoPeaks(id: string): number[] {
    let a = 0;
    for (const ch of id) a = (a * 31 + ch.charCodeAt(0)) >>> 0;
    const out: number[] = [];
    let env = 0.3;
    for (let i = 0; i < 40; i++) {
      a = (a * 1103515245 + 12345) >>> 0;
      env = env * 0.75 + ((a >>> 8) / 16777216) * 0.4;
      out.push(Math.min(1, env * Math.min(1, i / 6) * Math.min(1, (40 - i) / 6) + 0.05));
    }
    return out;
  }

  /** Spot script in the background: new and changed scenes, or every scene with `all`.
   * One AI call per scene, so it runs only when asked. */
  async function runSpot(all = false) {
    spotMenu = false;
    spotting = true;
    spotMsg = '';
    const r = await app.run(() => call<{ job_id: string }>('start_spotting', { all }));
    const j = r ? await app.waitJob(r.job_id, (x) => (spotMsg = x.message)) : undefined;
    spotting = false;
    spotMsg = '';
    if (j?.state === 'failed') app.say(j.message);
    else if (j?.result) {
      const res = j.result as { buckets: number; scenes: number; failed: { scene: number; error: string }[] };
      const failed = res.failed.length ? ` · scene ${res.failed.map((f) => f.scene).join(', ')} failed: ${res.failed[0].error}` : '';
      app.say(res.scenes ? `${res.scenes} scene${res.scenes === 1 ? '' : 's'} spotted, ${res.buckets} bucket${res.buckets === 1 ? '' : 's'}${failed}` : `Nothing new to spot${failed}`);
    }
    await load();
  }

  function spotLabel(): string {
    if (spotting) return spotMsg ? `Spotting ${spotMsg}…` : 'Spotting…';
    if (!spotted) return 'Spot script';
    if (spot.pending) return `Spot ${spot.pending} changed scene${spot.pending === 1 ? '' : 's'}`;
    return 'Re-spot script';
  }

  async function runSearch(more = false) {
    const query = more && found ? found.query : q.trim();
    if (!query) return;
    searching = true;
    const r = await app.run(() => call<SearchPage>('search_epidemic', { query, offset: more && found ? found.next : 0 }));
    searching = false;
    if (!r) return;
    app.openBucket = null;
    found = { query, results: more && found ? [...found.results, ...r.results] : r.results, next: r.next_offset, more: r.more };
  }

  // ---- drops from the search results ----
  function dragOver(e: DragEvent, target: string) {
    if (!e.dataTransfer?.types.includes(EPIDEMIC_DRAG)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
    over = target;
  }

  async function drop(e: DragEvent, b: Bucket | null) {
    const raw = e.dataTransfer?.getData(EPIDEMIC_DRAG);
    over = null;
    dragging = false;
    if (!raw) return;
    e.preventDefault();
    const p = JSON.parse(raw) as Preview;
    let into: string | null = null;
    if (b) {
      const r = await app.run(() => call<{ added: boolean; reason?: string }>('bucket_add_preview', { bucket_id: b.id, preview: p }));
      if (r) app.say(r.added ? `Added to ${b.name}` : `${p.title}: ${r.reason}`);
      if (r?.added) into = b.id;
    } else {
      const name = found?.query ?? p.title;
      const r = await app.run(() =>
        call<{ bucket_id: string }>('create_bucket', {
          name,
          keywords: [name.toLowerCase()],
          scenes: sceneNo ? [String(sceneNo)] : [],
          exclude: ['music', 'voices'],
          previews: [p],
        }),
      );
      if (r) app.say(`New bucket “${name}”`);
      into = r?.bucket_id ?? null;
    }
    if (into && found) {
      const id = into;
      found.results = found.results.map((x) => (x.remote_id === p.remote_id && !x.buckets.includes(id) ? { ...x, buckets: [...x.buckets, id] } : x));
    }
    await load();
  }

  function sceneList(s: string): string[] {
    return s
      .split(/[,\s]+/)
      .map((x) => x.replace(/^sc\.?/i, '').trim())
      .filter(Boolean);
  }

  async function create() {
    const r = await app.run(() =>
      call<{ bucket_id: string; search: unknown }>('create_bucket', {
        name: nb.name,
        keywords: nb.keywords,
        scenes: sceneList(nb.scenes),
        length_min_s: nb.min ? +nb.min : null,
        length_max_s: nb.max ? +nb.max : null,
        exclude: nb.exclude.split(',').map((x) => x.trim()).filter(Boolean),
        search: !!data?.connected.epidemic,
      }),
    );
    if (!r) return;
    creating = false;
    nb = { name: '', keywords: [], min: '', max: '', exclude: 'music, voices', scenes: '' };
    app.openBucket = r.bucket_id;
    await load();
  }

  function startNew() {
    creating = true;
    nb.scenes = sceneNo ? String(sceneNo) : '';
  }
</script>

<div class="bar" class:wide={!side}>
  <ImportTabs />
  <span class="vr" aria-hidden="true"></span>
  <div class="seg" role="group" aria-label="Sort buckets">
    {#each [['scene', 'Scene'], ['tag', 'Tag'], ['show', 'Show']] as [k, l] (k)}
      <button class="tab" class:on={sort === k} onclick={() => (sort = k as typeof sort)}>{l}</button>
    {/each}
  </div>
  <form class="sbox" role="search" onsubmit={(e) => { e.preventDefault(); void runSearch(); }}>
    <Icon name="search" size={14} />
    <input class="sin" bind:value={q} disabled={!data?.connected.epidemic} placeholder={data?.connected.epidemic ? 'Search Epidemic Sound' : 'Connect Epidemic Sound in Settings to search'} aria-label="Search Epidemic Sound" />
    {#if searching}<span class="spin"></span>{/if}
  </form>
  <div class="split">
    <button class="btn ic" class:joined={spotted > 0} onclick={() => (spot.pending ? runSpot() : (spotMenu = !spotMenu))} disabled={spotting || !spot.scenes} title="Claude reads the script (not the audio) and proposes buckets: one call per scene">
      <Icon name="sparkle" size={14} />{spotLabel()}
    </button>
    {#if spotted > 0}<button class="btn caret" aria-label="More spotting options" aria-expanded={spotMenu} disabled={spotting} onclick={() => (spotMenu = !spotMenu)}><Icon name="chevron" size={13} /></button>{/if}
    {#if spotMenu}
      <button class="backdrop" aria-label="Close menu" onclick={() => (spotMenu = false)}></button>
      <div class="menu glass" role="menu">
        {#if spot.pending}<button role="menuitem" onclick={() => runSpot()}>Spot {spot.pending} new or changed scene{spot.pending === 1 ? '' : 's'}<span class="muted">{spot.pending} AI call{spot.pending === 1 ? '' : 's'}</span></button>{/if}
        <button role="menuitem" onclick={() => runSpot(true)}>Re-spot all {spot.scenes} scenes<span class="muted">{spot.scenes} AI calls · cues you kept or accepted stay</span></button>
      </div>
    {/if}
  </div>
  <button class="btn primary ic" onclick={startNew}><Icon name="plus" size={14} />New bucket</button>
</div>

<main class="grid" class:wide={!side} aria-label="Buckets">
  {#if found}
    <article class="bk drop" class:over={over === 'new'} class:hint={dragging} aria-label="Drop here for a new bucket" ondragover={(e) => dragOver(e, 'new')} ondragleave={() => over === 'new' && (over = null)} ondrop={(e) => drop(e, null)} data-testid="new-bucket-drop">
      <Icon name="plus" size={18} />
      <span class="nm2">New bucket</span>
      <span class="muted dn">Drop a sound here to start “{found.query}”</span>
    </article>
  {/if}
  {#each data?.buckets ?? [] as b (b.id)}
    {@const here = !!app.sceneId && b.scene_ids.includes(app.sceneId)}
    {@const col = b.character ? solid(colorOf(b.color_index ?? 0).rgb) : null}
    {@const takes = !b.character}
    <article class="bk glass" class:sel={b.id === app.openBucket} class:here class:over={over === b.id} class:hint={dragging && takes} aria-label="Bucket {b.name}" data-testid="bucket-card" ondragover={(e) => takes && dragOver(e, b.id)} ondragleave={() => over === b.id && (over = null)} ondrop={(e) => takes && drop(e, b)}>
      <button class="cover" aria-label="Open {b.name}" onclick={() => (app.openBucket = app.openBucket === b.id ? null : b.id)}></button>
      <div class="hd">
        {#if col}<span class="ctag" style="background: {col}">{b.name.replace(/ · vocals$/, '')}</span><span class="ellip nm2 muted">vocals</span><span class="chip outline tn">{b.kept.length}</span>
        {:else}<span class="ellip nm2">{b.name}</span>{#each b.scenes as s (s)}<span class="chip outline tn">{s}</span>{/each}{/if}
      </div>
      <div class="kws">{#each b.keywords as k (k)}<span class="chip kw">{k}</span>{/each}</div>
      <div class="tiles">
        {#each tiles(b) as t (t.key)}
          <span class="tl" class:kept={t.kept} style={col && t.kept ? `background: color-mix(in srgb, ${col} 18%, transparent)` : ''}><span class="w"><Waveform peaks={t.peaks} fill={col ?? (t.kept ? 'var(--sfx)' : 'rgb(var(--ink-rgb) / 0.3)')} /></span></span>
        {/each}
        {#each Array.from({ length: Math.max(0, 4 - tiles(b).length) }, (_, i) => i) as i (i)}<span class="tl empty"></span>{/each}
      </div>
    </article>
  {/each}

  {#if creating}
    <article class="bk new" aria-label="New bucket">
      <!-- svelte-ignore a11y_autofocus -->
      <input class="nmin" placeholder="Bucket name" aria-label="Bucket name" bind:value={nb.name} autofocus />
      <KeywordField bind:keywords={nb.keywords} />
      <details open>
        <summary class="muted">Advanced</summary>
        <div class="adv">
          <label><span class="muted">Length</span><span class="rng"><input class="field xs" inputmode="decimal" placeholder="0" bind:value={nb.min} aria-label="Shortest (s)" /> to <input class="field xs" inputmode="decimal" placeholder="any" bind:value={nb.max} aria-label="Longest (s)" /> s</span></label>
          <label><span class="muted">Leave out</span><input class="field sm" bind:value={nb.exclude} aria-label="Leave out" /></label>
          <label><span class="muted">Scenes</span><input class="field sm" placeholder="2, 7, 9" bind:value={nb.scenes} aria-label="Scenes" /></label>
        </div>
      </details>
      <div class="acts"><button class="btn sm" onclick={() => (creating = false)}>Cancel</button><button class="btn primary sm" disabled={!nb.name.trim()} onclick={create}>{data?.connected.epidemic ? 'Create and search' : 'Create'}</button></div>
    </article>
  {/if}

  {#if data && !data.buckets.length && !creating}
    <p class="muted empty-note">No buckets yet. <b>Spot script</b> has Claude read the script and propose buckets, the search bar finds sounds on Epidemic Sound to drag in, or start one with <b>New bucket</b>.</p>
  {/if}
</main>

{#if open && data}
  <BucketPanel bucket={open} connected={data.connected} onchange={load} />
{:else if found}
  <EpidemicSearch query={found.query} results={found.results} more={found.more} busy={searching} onmore={() => runSearch(true)} onclose={() => (found = null)} ondragging={(on) => { dragging = on; if (!on) over = null; }} />
{/if}

<style>
  .bar {
    position: absolute;
    top: 84px;
    left: 28px;
    right: 492px;
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .grid {
    position: absolute;
    top: 140px;
    bottom: 24px;
    left: 28px;
    right: 492px;
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    grid-auto-rows: min-content;
    gap: 12px;
    overflow: auto;
  }
  .bar.wide,
  .grid.wide {
    right: 28px;
  }
  .sbox {
    flex: 1;
    min-width: 180px;
    max-width: 420px;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 12px;
    border-radius: var(--radius-pill);
    background: var(--glass-fill-1);
    box-shadow: inset 0 0 0 1px var(--glass-outline);
    color: var(--text-3);
  }
  .sin {
    flex: 1;
    min-width: 0;
    background: none;
    border: 0;
    outline: none;
    color: var(--text-strong);
    font: inherit;
    font-size: 13px;
  }
  .sbox:focus-within {
    box-shadow: inset 0 0 0 1.5px var(--text-3);
  }
  .split {
    position: relative;
    display: flex;
    margin-left: auto;
  }
  .split .joined {
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
  }
  .caret {
    border-top-left-radius: 0;
    border-bottom-left-radius: 0;
    padding: 0 8px;
    margin-left: 1px;
  }
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 20;
    cursor: default;
  }
  .menu {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 21;
    display: flex;
    flex-direction: column;
    min-width: 260px;
    padding: 6px;
    border-radius: 16px;
    background: var(--bar-strong);
  }
  .menu button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: 8px 10px;
    border-radius: 10px;
    text-align: left;
    font-size: 13px;
  }
  .menu button:hover {
    background: var(--glass-fill-2);
  }
  .menu .muted {
    font-size: 11px;
  }
  .spin {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 2px solid var(--text-4);
    border-top-color: transparent;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .vr {
    width: 1px;
    height: 22px;
    background: var(--glass-outline);
  }
  .btn.ic {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .btn.sm {
    height: 32px;
  }
  .bk {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
    border-radius: 22px;
    background: rgb(var(--night-rgb) / 0.35);
    min-width: 0;
  }
  .bk.sel {
    box-shadow:
      inset 0 0 0 1.5px var(--text-strong),
      0 18px 50px rgb(var(--shadow-rgb) / 0.35);
  }
  .bk.here:not(.sel) {
    box-shadow:
      inset 0 0 0 1px rgb(var(--sfx-rgb) / 0.5),
      var(--glass-shadow);
  }
  .bk.hint {
    outline: 1.5px dashed rgb(var(--sfx-rgb) / 0.7);
    outline-offset: -1.5px;
  }
  .bk.over {
    box-shadow:
      inset 0 0 0 2px var(--sfx),
      0 18px 50px rgb(var(--shadow-rgb) / 0.35);
  }
  .drop {
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-height: 120px;
    text-align: center;
    color: var(--text-3);
    background: transparent;
    box-shadow: inset 0 0 0 1.5px rgb(var(--ink-rgb) / 0.3);
  }
  .drop.hint {
    box-shadow: none;
  }
  .drop .nm2 {
    flex: none;
  }
  .dn {
    font-size: 12px;
  }
  .cover {
    position: absolute;
    inset: 0;
    border-radius: inherit;
  }
  .hd {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .nm2 {
    flex: 1;
    min-width: 0;
    font-size: 15px;
    font-weight: 600;
  }
  .ctag {
    display: inline-flex;
    align-items: center;
    height: 24px;
    padding: 0 10px;
    border-radius: var(--radius-pill);
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: #16110d;
    flex: none;
  }
  .kws {
    display: flex;
    gap: 5px;
    flex-wrap: wrap;
  }
  .kw {
    font-weight: 500;
    color: var(--text-3);
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 6px;
  }
  .tl {
    height: 42px;
    border-radius: 10px;
    display: flex;
    align-items: center;
    padding: 0 5px;
    box-sizing: border-box;
    background: var(--glass-fill-1);
    box-shadow: inset 0 0 0 1px rgb(var(--ink-rgb) / 0.18);
  }
  .tl.kept {
    background: rgb(var(--sfx-rgb) / 0.16);
    box-shadow: none;
  }
  .tl.empty {
    box-shadow: inset 0 0 0 1px rgb(var(--ink-rgb) / 0.06);
  }
  .tl .w {
    position: relative;
    width: 100%;
    height: 24px;
  }
  .new {
    box-shadow: inset 0 0 0 1.5px rgb(var(--ink-rgb) / 0.4);
    background: transparent;
  }
  .nmin {
    background: none;
    border: 0;
    outline: none;
    color: inherit;
    font: inherit;
    font-size: 15px;
    font-weight: 600;
    padding: 0;
  }
  details {
    font-size: 12.5px;
  }
  summary {
    cursor: pointer;
    padding: 2px 0;
  }
  .adv {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 8px;
  }
  .adv label {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
  }
  .field.sm {
    height: 28px;
    width: 150px;
    font-size: 12px;
  }
  .field.xs {
    height: 28px;
    width: 46px;
    padding: 0 6px;
    font-size: 12px;
    text-align: right;
  }
  .rng {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .acts {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }
  .empty-note {
    grid-column: 1 / -1;
    font-size: 13px;
  }
  @media (max-width: 1100px) {
    .grid,
    .bar {
      right: 28px !important;
    }
    .grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
