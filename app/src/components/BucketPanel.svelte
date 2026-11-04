<script lang="ts">
  // The side panel of one bucket (Import › Sounds): provider tabs, keyword query, the kept
  // sounds (gold) and previews (grey, click the waveform to audition from there).
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import { len } from '../lib/format';
  import Icon from './Icon.svelte';
  import KeywordField from './KeywordField.svelte';
  import PreviewPlayer from './PreviewPlayer.svelte';
  import Waveform from './Waveform.svelte';
  import type { Bucket, BucketSound, Preview, SoundFace, SoundKey } from '../lib/types';
  import { colorOf, solid } from '../theme';

  let { bucket, connected, onchange }: { bucket: Bucket; connected: { epidemic: boolean; elevenlabs: boolean }; onchange: () => Promise<void> } = $props();
  let provider = $state<'epidemic' | 'elevenlabs' | 'library'>('epidemic');
  let keywords = $state<string[]>([]);
  let prompt = $state('');
  let library = $state<BucketSound[]>([]);
  let busy = $state<string | null>(null);
  let cursor = $state(0);
  let tagFilter = $state<string | null>(null);
  /** The provider has no more results for these keywords ("Load more" hides). */
  let exhausted = $state(false);
  /** A character's vocal bucket: the actor's own recordings, nothing to search or generate. */
  const vocal = $derived(!!bucket.character);
  const col = $derived(bucket.character ? solid(colorOf(bucket.color_index ?? 0).rgb) : null);
  const who = $derived(bucket.name.replace(/ · vocals$/, ''));
  const kept = $derived(vocal && tagFilter ? bucket.kept.filter((k) => k.tags[0] === tagFilter) : bucket.kept);

  let shownId = '';
  $effect(() => {
    // a different bucket: start from its saved query (edits to this one survive reloads)
    if (bucket.id === shownId) return;
    shownId = bucket.id;
    keywords = [...bucket.keywords];
    prompt = bucket.keywords.join(', ');
    library = [];
    cursor = 0;
    tagFilter = null;
    exhausted = false;
  });

  async function search(more = false) {
    busy = more ? 'more' : 'search';
    if (provider === 'library') {
      const r = await app.run(() => call<{ library: BucketSound[] }>('bucket_search', { bucket_id: bucket.id, provider: 'library', keywords }));
      library = r?.library ?? [];
    } else {
      const r = await app.run(() => call<{ added: number; more: boolean }>('bucket_search', { bucket_id: bucket.id, more, keywords }));
      if (r) exhausted = !r.more;
      if (more && r && !r.added) app.say('No more results for these keywords');
    }
    busy = null;
    await onchange();
  }

  async function generate() {
    busy = 'generate';
    await app.run(() => call('bucket_generate', { bucket_id: bucket.id, prompt }));
    busy = null;
    await onchange();
  }

  async function download(p: Preview) {
    busy = p.remote_id;
    await app.run(() => call('bucket_download', { bucket_id: bucket.id, remote_id: p.remote_id }));
    busy = null;
    await onchange();
  }

  async function act(name: string, params: object) {
    await app.run(() => call(name, { bucket_id: bucket.id, ...params }));
    await onchange();
  }

  function playAt(e: MouseEvent, key: SoundKey, face: SoundFace) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    void app.playSound(key, Math.max(0, ((e.clientX - r.left) / r.width) * face.duration_s), face);
  }

  /** Up to here a dropped sound keeps its length, looped or not. */
  const LOOP_MIN_S = 6;

  const previewFace = (p: Preview): SoundFace => ({ name: p.title, duration_s: p.duration_s, peaks: pseudo(p.remote_id) });

  // Space plays, ← → step through results (SND-51)
  const rows = $derived<{ key: SoundKey; face: SoundFace }[]>([...kept.map((k) => ({ key: { asset_id: k.id }, face: k })), ...(vocal ? [] : bucket.previews.map((p) => ({ key: { remote_id: p.remote_id }, face: previewFace(p) })))]);
  function key(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    if (t.tagName === 'INPUT' || app.overlay || !rows.length) return;
    if (e.key === 'ArrowDown' || e.key === 'ArrowRight' || e.key === 'ArrowUp' || e.key === 'ArrowLeft') {
      const d = e.key === 'ArrowDown' || e.key === 'ArrowRight' ? 1 : -1;
      cursor = Math.max(0, Math.min(rows.length - 1, cursor + d));
      void app.playSound(rows[cursor].key, 0, rows[cursor].face);
    } else if (e.key === ' ') {
      const r = rows[Math.min(cursor, rows.length - 1)];
      void app.toggleSound(r.key, r.face);
    } else return;
    e.preventDefault();
    e.stopPropagation();
  }
  $effect(() => {
    window.addEventListener('keydown', key, true);
    return () => window.removeEventListener('keydown', key, true);
  });

  function pseudo(id: string): number[] {
    let a = 7;
    for (const ch of id) a = (a * 31 + ch.charCodeAt(0)) >>> 0;
    const out: number[] = [];
    let env = 0.3;
    for (let i = 0; i < 60; i++) {
      a = (a * 1103515245 + 12345) >>> 0;
      env = env * 0.8 + ((a >>> 8) / 16777216) * 0.3;
      out.push(Math.min(1, (0.3 + env) * Math.min(1, i / 10) * Math.min(1, (60 - i) / 18)));
    }
    return out;
  }
  const progress = (dur: number) => (dur > 0 ? Math.min(100, ((app.preview?.pos ?? 0) / dur) * 100) : 0);
</script>

<aside class="glass side panel" aria-label="Bucket {bucket.name}">
  <div class="hd">
    <span class="t ellip">{bucket.name}</span>
    {#each bucket.scenes as s (s)}<span class="chip">{s}</span>{/each}
    <button class="ib" aria-label="Close panel" onclick={() => (app.openBucket = null)}><Icon name="x" /></button>
  </div>
  {#if vocal}
    <p class="muted note">From {who}’s own recordings, found by the analysis. Removing one keeps it out of the next analysis.</p>
    <div class="tags" role="group" aria-label="Kinds">
      <button class="chip" class:on={!tagFilter} onclick={() => (tagFilter = null)}>All {bucket.kept.length}</button>
      {#each bucket.keywords as k (k)}<button class="chip" class:on={tagFilter === k} onclick={() => (tagFilter = tagFilter === k ? null : k)}>{k} {bucket.kept.filter((x) => x.tags[0] === k).length}</button>{/each}
    </div>
  {:else}
  <div class="seg" role="tablist" aria-label="Provider">
      {#each [['epidemic', 'Epidemic Sound'], ['elevenlabs', 'ElevenLabs'], ['library', 'Library']] as [k, l] (k)}
        <button role="tab" aria-selected={provider === k} class="tab" class:on={provider === k} onclick={() => (provider = k as typeof provider)}>{l}</button>
      {/each}
    </div>
  
    {#if provider === 'elevenlabs'}
      <div class="q">
        <input class="field grow" bind:value={prompt} aria-label="Prompt" placeholder="Describe the sound" />
        <button class="btn" onclick={generate} disabled={busy === 'generate' || !connected.elevenlabs}>{busy === 'generate' ? 'Generating…' : 'Generate'}</button>
      </div>
      {#if !connected.elevenlabs}<p class="muted note">Connect ElevenLabs in Settings → Connections to generate sounds.</p>{/if}
    {:else}
      <div class="q">
        <div class="grow"><KeywordField bind:keywords /></div>
        <button class="btn" onclick={() => search()} disabled={!!busy || (provider === 'epidemic' && !connected.epidemic)}>{busy === 'search' ? 'Searching…' : 'Search'}</button>
      </div>
      {#if provider === 'epidemic' && !connected.epidemic}<p class="muted note">Connect Epidemic Sound in Settings → Connections to search and preview.</p>{/if}
    {/if}
  {/if}

  <div class="list">
    <div class="label sec">{vocal ? 'Vocals' : 'In this bucket'}</div>
    {#each kept as r, i (r.id)}
      <div class="srow kept" class:cur={cursor === i} style={col ? `background: color-mix(in srgb, ${col} 12%, transparent)` : ''} data-testid="kept-row">
        <button class="ib pl gold" style={col ? `background: ${col}; color: #16110d` : ''} aria-label="Play {r.name}" onclick={() => app.toggleSound({ asset_id: r.id }, r)}><Icon name={app.previewing(r.id) ? 'stop' : 'play'} size={11} /></button>
        <div class="mid">
          <span class="ellip nm3">{r.name}{#if r.heard}<span class="muted heard"> · “{r.heard}”</span>{/if}{#if r.matched}<span class="muted heard" title="Matched by its file name. It stays in Sound/{r.file.replace('show:', '')}; remove it to hide it here"> · matched</span>{/if}</span>
          <span class="wv" role="button" tabindex="-1" aria-label="Play {r.name} from here" onclick={(e) => playAt(e, { asset_id: r.id }, r)} onkeydown={() => {}}>
            <Waveform peaks={r.peaks} fill={col ?? 'var(--sfx)'} />
            {#if app.previewing(r.id)}<span class="ph" style="left: {progress(r.duration_s)}%"></span>{/if}
          </span>
        </div>
        <span class="muted tn d">{len(r.duration_s)}</span>
        {#if vocal}<span></span>{:else if r.duration_s > LOOP_MIN_S}<button class="lp" class:on={r.loop} aria-pressed={r.loop} aria-label="Loop {r.name}" title="Loop: goes on the ambience lane and repeats to the end of the scene, and can be a scene's room tone" onclick={() => act('bucket_set_loop', { asset_id: r.id, on: !r.loop })} data-testid="panel-loop">Loop</button>{:else}<span class="ib ok" role="img" aria-label="Downloaded"><Icon name="check" stroke={2.4} /></span>{/if}
        <button class="ib" aria-label="Remove {r.name}" onclick={() => act('bucket_remove', { asset_id: r.id })}><Icon name="x" size={13} /></button>
      </div>
    {:else}
      <p class="muted note">{vocal ? 'Nothing found yet. The analysis collects these.' : 'Nothing kept yet. Download a preview to keep it.'}</p>
    {/each}

    {#if vocal}
      <!-- the actor's own recordings only -->
    {:else if provider === 'library'}
      <div class="label sec">Library</div>
      {#each library as r (r.id)}
        <div class="srow">
          <button class="ib pl" aria-label="Play {r.name}" onclick={() => app.toggleSound({ asset_id: r.id }, r)}><Icon name={app.previewing(r.id) ? 'stop' : 'play'} size={11} /></button>
          <div class="mid"><span class="ellip nm3 muted">{r.name}</span><span class="wv"><Waveform peaks={r.peaks} fill="rgb(var(--ink-rgb) / 0.32)" /></span></div>
          <span class="muted tn d">{len(r.duration_s)}</span>
          <button class="ib" aria-label="Keep {r.name}" onclick={() => act('bucket_keep', { asset_id: r.id }).then(() => (library = library.filter((x) => x.id !== r.id)))}><Icon name="plus" /></button>
          <span></span>
        </div>
      {:else}
        <p class="muted note">Search the library for the keywords.</p>
      {/each}
    {:else}
      <div class="label sec">Previews · not downloaded</div>
      {#each bucket.previews as r, i (r.remote_id)}
        {@const on = app.previewing(r.remote_id)}
        <div class="srow" class:cur={cursor === bucket.kept.length + i} class:on data-testid="preview-row">
          <button class="ib pl" aria-label="Play preview {r.title}" onclick={() => app.toggleSound({ remote_id: r.remote_id }, previewFace(r))}><Icon name={on ? 'stop' : 'play'} size={11} /></button>
          <div class="mid">
            <span class="ellip nm3 muted">{r.title}</span>
            <span class="wv tall" role="button" tabindex="-1" aria-label="Play {r.title} from here" onclick={(e) => playAt(e, { remote_id: r.remote_id }, previewFace(r))} onkeydown={() => {}}>
              <Waveform peaks={pseudo(r.remote_id)} fill="rgb(var(--ink-rgb) / 0.32)" />
              {#if on}<span class="ph" style="left: {progress(r.duration_s)}%"></span>{/if}
            </span>
          </div>
          <span class="muted tn d">{len(r.duration_s)}</span>
          <button class="ib" aria-label="Download {r.title}" disabled={busy === r.remote_id} onclick={() => download(r)}>{#if busy === r.remote_id}<span class="spin"></span>{:else}<Icon name="download" />{/if}</button>
          <button class="ib" aria-label="Dismiss {r.title}" onclick={() => act('bucket_dismiss', { remote_id: r.remote_id })}><Icon name="x" size={13} /></button>
        </div>
      {:else}
        {#if connected.epidemic}<p class="muted note">Search to fill the bucket with previews.</p>{/if}
      {/each}
      {#if bucket.previews.length && !exhausted}<button class="btn more" onclick={() => search(true)} disabled={!!busy}>{busy === 'more' ? 'Loading…' : 'Load more'}</button>{/if}
    {/if}
  </div>
  <PreviewPlayer />
</aside>

<style>
  .panel {
    position: absolute;
    top: 76px;
    bottom: 16px;
    right: 16px;
    width: 460px;
    box-sizing: border-box;
    border-radius: 28px;
    padding: 18px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    background: var(--bar-strong);
    z-index: 12;
  }
  .hd {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .t {
    font-size: 18px;
    font-weight: 600;
    flex: 1;
    min-width: 0;
  }
  .seg .tab {
    flex: 1;
    height: 30px;
    font-size: 12px;
  }
  .q {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .q .btn {
    height: 38px;
  }
  .note {
    font-size: 12px;
    margin: 0;
    padding: 4px 8px;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .sec {
    padding: 12px 8px 4px;
  }
  .sec:first-child {
    padding-top: 4px;
  }
  .srow {
    display: grid;
    grid-template-columns: 32px minmax(0, 1fr) 40px 32px 32px;
    gap: 10px;
    align-items: center;
    padding: 8px;
    border-radius: 14px;
  }
  .lp {
    height: 22px;
    padding: 0 6px;
    border-radius: var(--radius-pill);
    font-size: 10.5px;
    color: var(--text-3);
    box-shadow: inset 0 0 0 1px rgb(var(--ink-rgb) / 0.2);
  }
  .lp.on {
    color: var(--on-sfx);
    background: var(--sfx);
    box-shadow: none;
  }
  .srow.kept {
    background: rgb(var(--sfx-rgb) / 0.1);
  }
  .srow.on,
  .srow.cur:not(.kept) {
    background: var(--glass-fill-2);
  }
  .pl {
    border-radius: 50%;
    box-shadow: inset 0 0 0 1.5px rgb(var(--ink-rgb) / 0.5);
    color: var(--text-strong);
  }
  .pl.gold {
    background: var(--sfx);
    color: var(--on-sfx);
    box-shadow: none;
  }
  .tags {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .tags .chip.on {
    background: var(--button-light-active);
    color: var(--text-on-light);
  }
  .heard {
    font-weight: 400;
  }
  .ok {
    color: var(--sfx);
  }
  .mid {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .nm3 {
    font-size: 12.5px;
  }
  .kept .nm3 {
    font-weight: 600;
  }
  .wv {
    position: relative;
    display: block;
    height: 16px;
    cursor: pointer;
  }
  .wv.tall {
    height: 22px;
  }
  .ph {
    position: absolute;
    top: -2px;
    bottom: -2px;
    width: 2px;
    background: var(--playhead);
  }
  .d {
    font-size: 11px;
  }
  .more {
    align-self: center;
    margin-top: 8px;
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
</style>
