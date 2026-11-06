<script lang="ts">
  // The Edit page's docked bucket: only this scene's buckets. Gold = downloaded, outlined =
  // preview that downloads when dropped on a lane. Rows drag onto the timeline.
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import { grab } from '../lib/drag.svelte';
  import { len } from '../lib/format';
  import Icon from './Icon.svelte';
  import PreviewPlayer from './PreviewPlayer.svelte';
  import Waveform from './Waveform.svelte';
  import type { Asset, Bucket, BucketsView } from '../lib/types';

  /** Up to here a dropped sound keeps its length, looped or not. */
  const LOOP_MIN_S = 6;
  /** Library results shown under a search. */
  const LIBRARY_ROWS = 30;

  /** On Arrange the layer owns the buckets: it hands them in, with an eye per bucket. */
  interface Eyes {
    hidden: string[];
    toggle: (id: string, solo: boolean) => void;
    /** The colour of a bucket. */
    color: (id: string) => string | null;
    /** Sounds already in the scene. */
    used: Set<string>;
    /** Placed sounds that come from no bucket, with their eye. */
    others: { id: string; color: string } | null;
  }

  let { given = null, reload = null, eyes = null, off = false }: { given?: BucketsView | null; reload?: (() => Promise<void>) | null; eyes?: Eyes | null; off?: boolean } = $props();

  let own = $state<BucketsView | null>(null);
  let closed = $state<Record<string, boolean>>({});
  let busy = $state<string | null>(null);
  const data = $derived(given ?? own);

  async function load() {
    if (reload) return reload();
    if (!app.sceneId) return;
    own = (await app.run(() => call<BucketsView>('buckets', { scene: app.sceneId }))) ?? own;
  }
  $effect(() => {
    void app.sceneId;
    void app.status?.rev;
    if (!reload) void load();
  });

  // ---- search: this scene's buckets narrow down, the rest of the library comes in below ----
  let q = $state('');
  let library = $state<Asset[]>([]);
  const words = $derived(q.toLowerCase().split(/\s+/).filter(Boolean));
  const hit = (text: string) => words.every((w) => text.toLowerCase().includes(w));
  const whole = (b: Bucket) => !words.length || hit(`${b.name} ${b.keywords.join(' ')}`);
  const keptOf = (b: Bucket) => (whole(b) ? b.kept : b.kept.filter((s) => hit(`${s.name} ${s.tags.join(' ')}`)));
  const previewsOf = (b: Bucket) => (whole(b) ? b.previews : b.previews.filter((p) => hit(p.title)));
  const shows = (b: Bucket) => !words.length || keptOf(b).length + previewsOf(b).length > 0;
  const listed = $derived(new Set((data?.buckets ?? []).flatMap((b) => b.kept.map((s) => s.id))));
  const further = $derived(library.filter((a) => !listed.has(a.id)).slice(0, LIBRARY_ROWS));
  const nameOf = (a: Asset) => (a.file.split('/').pop() ?? a.id).replace(/\.\w+$/, '');

  $effect(() => {
    const term = q.trim();
    if (!term) {
      library = [];
      return;
    }
    const wait = setTimeout(async () => {
      const r = await app.run(() => call<Asset[]>('list_library', { query: term }));
      if (q.trim() === term) library = r ?? [];
    }, 180);
    return () => clearTimeout(wait);
  });

  function searchKey(e: KeyboardEvent) {
    if (e.key !== 'Escape') return;
    e.stopPropagation();
    q = '';
    (e.currentTarget as HTMLElement).blur();
  }

  function isOpen(id: string, i: number) {
    if (words.length) return true;
    return closed[id] === undefined ? i < 2 && !data?.buckets[i].show_wide : !closed[id];
  }

  async function setLoop(bucket_id: string, key: { asset_id?: string; remote_id?: string }, on: boolean) {
    await app.method('bucket_set_loop', { bucket_id, ...key, on });
    await load();
  }

  async function more(id: string) {
    busy = id;
    const r = await app.run(() => call<{ added: number }>('bucket_search', { bucket_id: id, more: true }));
    if (r && !r.added) app.say('No more results for these keywords');
    busy = null;
    await load();
  }

  function pseudo(id: string): number[] {
    let a = 11;
    for (const ch of id) a = (a * 31 + ch.charCodeAt(0)) >>> 0;
    const out: number[] = [];
    let env = 0.3;
    for (let i = 0; i < 40; i++) {
      a = (a * 1103515245 + 12345) >>> 0;
      env = env * 0.75 + ((a >>> 8) / 16777216) * 0.4;
      out.push(Math.min(1, env * Math.min(1, i / 5) * Math.min(1, (40 - i) / 5) + 0.05));
    }
    return out;
  }
</script>

{#snippet eye(id: string, name: string, faded: boolean)}
  <button class="eye" onmousedown={(e) => e.preventDefault()} aria-pressed={!faded} aria-label="{faded ? 'Show' : 'Fade'} {name}" title="{faded ? 'Show' : 'Fade'} {name} · ⌥ click to show only this bucket" onclick={(e) => eyes?.toggle(id, e.altKey)} data-testid="bucket-eye">
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7-10-7-10-7Z" />
      {#if faded}<path d="M4 4l16 16" />{:else}<circle cx="12" cy="12" r="3" />{/if}
    </svg>
  </button>
{/snippet}

<aside class="glass panel" class:beside={!!eyes} class:off class:over={app.narrow} aria-hidden={off} inert={off} data-own-keys aria-label="Bucket for scene {app.view?.scene.number ?? ''}">
  <div class="hd">
    <span class="t">Bucket · Sc. {app.view?.scene.number ?? ''}</span>
    <button class="btn" onclick={() => app.toggleFullBucket()}>Full bucket <span class="kb">⇧B</span></button>
    <button class="ib" aria-label="Hide bucket (B)" title="Hide (B)" onclick={() => app.setBucket(false)}><Icon name="chevron-right" /></button>
  </div>
  <div class="legend muted">
    <span><span class="dot gold"></span>Downloaded</span>
    <span><span class="dot ring"></span>Preview, downloads on drop</span>
  </div>
  <input class="field find" type="search" placeholder="Search sounds" aria-label="Search sounds" bind:value={q} onkeydown={searchKey} data-testid="bucket-search" />
  <div class="list">
    {#if eyes?.others}
      {@const faded = eyes.hidden.includes(eyes.others.id)}
      <section class="bkt" class:faded aria-label="Other sounds">
        <div class="bhrow">
          <span class="bh"><span class="dot" style="background: {faded ? 'transparent' : eyes.others.color}"></span><span class="bn">Other sounds</span></span>
          {@render eye(eyes.others.id, 'Other sounds', faded)}
        </div>
      </section>
    {/if}
    {#each data?.buckets ?? [] as b, i (b.id)}
      {@const open = isOpen(b.id, i)}
      {@const col = eyes ? eyes.color(b.id) : null}
      {@const faded = !!eyes?.hidden.includes(b.id)}
      {#if (!eyes || col) && shows(b)}
      {#if b.show_wide && !data?.buckets[i - 1]?.show_wide}<div class="label wide">Whole show</div>{/if}
      <section class="bkt" class:faded aria-label={b.name}>
        <div class="bhrow">
        <button class="bh" onmousedown={(e) => e.preventDefault()} aria-expanded={open} onclick={() => (closed = { ...closed, [b.id]: open })}>
          <span class="chev" style="transform: rotate({open ? 0 : -90}deg)"><Icon name="chevron" size={11} stroke={2.4} /></span>
          {#if col}<span class="dot" style="background: {faded ? 'transparent' : col}; box-shadow: {faded ? `inset 0 0 0 1.5px ${col}` : 'none'}"></span>{/if}
          <span class="bn">{b.name}</span>
          <span class="muted tn cnt">{b.kept.length} + {b.previews.length}</span>
        </button>
        {#if eyes}{@render eye(b.id, b.name, faded)}{/if}
        </div>
        {#if open}
          {#each keptOf(b) as s (s.id)}
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions (dragging is the mouse's; D82) -->
            <div class="srow kept" class:used={eyes?.used.has(s.id)} style={col ? `background: color-mix(in srgb, ${col} 12%, transparent)` : ''} role="listitem" onmousedown={(e) => grab(e, { kind: 'bucket', bucket_id: b.id, asset_id: s.id, name: s.name, duration_s: s.duration_s })} data-testid="bucket-sound">
              <span class="grip"><Icon name="grip" /></span>
              <button class="play gold" style={col ? `background: ${col}; color: #16110d` : ''} aria-label="Play {s.name}" onmousedown={(e) => e.preventDefault()} onclick={() => app.toggleSound({ asset_id: s.id }, s)}><Icon name={app.previewing(s.id) ? 'stop' : 'play'} size={10} /></button>
              <div class="mid"><span class="ellip nm4">{s.name}</span><span class="wv"><Waveform peaks={s.peaks} fill={col ?? 'var(--sfx)'} /></span></div>
              {#if s.duration_s > LOOP_MIN_S}<button class="lp" class:on={s.loop} aria-pressed={s.loop} aria-label="Loop {s.name}" title="Loop: goes on the ambience lane and repeats to the end of the scene" onmousedown={(e) => e.preventDefault()} onclick={() => setLoop(b.id, { asset_id: s.id }, !s.loop)} data-testid="bucket-loop">Loop</button>{:else}<span></span>{/if}
              <span class="muted tn d">{len(s.duration_s)}</span>
            </div>
          {/each}
          {#each previewsOf(b).slice(0, 6) as p (p.remote_id)}
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions (dragging is the mouse's; D82) -->
            <div class="srow" role="listitem" onmousedown={(e) => grab(e, { kind: 'bucket', bucket_id: b.id, remote_id: p.remote_id, name: p.title, duration_s: p.duration_s })} data-testid="bucket-preview">
              <span class="grip"><Icon name="grip" /></span>
              <button class="play ring" aria-label="Play {p.title}" onmousedown={(e) => e.preventDefault()} onclick={() => app.toggleSound({ remote_id: p.remote_id }, { name: p.title, duration_s: p.duration_s, peaks: pseudo(p.remote_id) })}><Icon name={app.previewing(p.remote_id) ? 'stop' : 'play'} size={10} /></button>
              <div class="mid"><span class="ellip nm4 muted">{p.title}</span><span class="wv"><Waveform peaks={pseudo(p.remote_id)} fill="rgb(var(--ink-rgb) / 0.32)" /></span></div>
              {#if p.duration_s > LOOP_MIN_S}<button class="lp" class:on={p.loop} aria-pressed={!!p.loop} aria-label="Loop {p.title}" title="Loop: goes on the ambience lane and repeats to the end of the scene" onmousedown={(e) => e.preventDefault()} onclick={() => setLoop(b.id, { remote_id: p.remote_id }, !p.loop)}>Loop</button>{:else}<span></span>{/if}
              <span class="muted tn d">{len(p.duration_s)}</span>
            </div>
          {/each}
          {#if data?.connected.epidemic && b.keywords.length}<button class="btn sm" onclick={() => more(b.id)} disabled={busy === b.id}>{busy === b.id ? 'Searching…' : 'More like these'}</button>{/if}
        {/if}
      </section>
      {/if}
    {:else}
      <p class="muted note">No buckets for this scene. Open the full bucket (⇧B) to spot the script or start one.</p>
    {/each}
    {#if further.length}
      <section class="bkt" aria-label="In the library">
        <span class="bn">In the library</span>
        {#each further as a (a.id)}
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions (dragging is the mouse's; D82) -->
          <div class="srow" role="listitem" onmousedown={(e) => grab(e, { kind: 'bucket', asset_id: a.id, name: nameOf(a), duration_s: a.duration_s })} data-testid="library-sound">
            <span class="grip"><Icon name="grip" /></span>
            <button class="play ring" aria-label="Play {nameOf(a)}" onmousedown={(e) => e.preventDefault()} onclick={() => app.toggleSound({ asset_id: a.id }, { name: nameOf(a), duration_s: a.duration_s, peaks: [] })}><Icon name={app.previewing(a.id) ? 'stop' : 'play'} size={10} /></button>
            <div class="mid"><span class="ellip nm4">{nameOf(a)}</span><span class="muted ellip cat">{a.category}</span></div>
            <span></span>
            <span class="muted tn d">{len(a.duration_s)}</span>
          </div>
        {/each}
      </section>
    {:else if words.length && !data?.buckets.some(shows)}
      <p class="muted note">No sound by that name.</p>
    {/if}
  </div>
  {#if !off}<PreviewPlayer />{/if}
</aside>

<style>
  .wide {
    margin-top: 6px;
  }
  .bhrow {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .bhrow .bh {
    flex: 1;
    min-width: 0;
  }
  .eye {
    flex: none;
    width: 28px;
    height: 24px;
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgb(var(--ink-rgb) / 0.75);
    transition:
      background var(--dur-fast),
      color var(--dur-fast);
  }
  .eye:hover {
    background: rgb(var(--veil-rgb) / 0.08);
  }
  .faded .eye {
    color: rgb(var(--ink-rgb) / 0.35);
  }
  .faded .bn {
    color: var(--text-4);
  }
  .faded .srow {
    opacity: 0.35;
  }
  .srow.used {
    opacity: 0.45;
  }
  .panel.beside {
    top: 76px;
    bottom: 96px;
    transition:
      transform var(--dur-move) var(--ease-move),
      opacity 0.5s var(--ease-move);
  }
  .panel.beside.off {
    transform: translateX(420px);
    opacity: 0;
    pointer-events: none;
    transition-delay: 0s;
  }
  @media (prefers-reduced-motion: reduce) {
    .panel.beside {
      transition: none;
    }
  }
  .panel {
    position: absolute;
    top: 138px;
    bottom: 16px;
    right: 16px;
    width: 372px;
    box-sizing: border-box;
    border-radius: 26px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    background: var(--bar-strong);
    z-index: 8;
  }
  .hd {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .t {
    font-size: 15px;
    font-weight: 600;
    flex: 1;
  }
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding: 0 12px;
    font-size: 12px;
    white-space: nowrap;
  }
  .btn.sm {
    height: 28px;
    font-size: 11.5px;
    align-self: flex-start;
  }
  .legend {
    display: flex;
    align-items: center;
    gap: 14px;
    font-size: 11.5px;
  }
  .legend > span {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .dot.gold {
    background: var(--sfx);
  }
  .dot.ring {
    box-shadow: inset 0 0 0 1.5px rgb(var(--ink-rgb) / 0.5);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .bkt {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border-radius: 16px;
    background: var(--glass-fill-1);
  }
  .bh {
    display: flex;
    align-items: center;
    gap: 8px;
    text-align: left;
  }
  .chev {
    display: inline-flex;
    opacity: 0.7;
    transition: transform var(--dur-fast);
  }
  .bn {
    font-size: 13px;
    font-weight: 600;
    flex: 1;
  }
  .cnt {
    font-size: 11px;
  }
  .srow {
    display: grid;
    grid-template-columns: 12px 28px minmax(0, 1fr) auto 34px;
    gap: 8px;
    align-items: center;
    height: 40px;
    padding: 0 6px;
    border-radius: 12px;
    cursor: grab;
  }
  .srow.kept {
    background: rgb(var(--sfx-rgb) / 0.1);
  }
  .find {
    flex: none;
    height: 32px;
    font-size: 12.5px;
  }
  .cat {
    font-size: 10.5px;
  }
  .lp {
    height: 20px;
    padding: 0 7px;
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
  .grip {
    color: rgb(var(--ink-rgb) / 0.45);
    display: flex;
  }
  .play {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .play.gold {
    background: var(--sfx);
    color: var(--on-sfx);
  }
  .play.ring {
    box-shadow: inset 0 0 0 1.5px rgb(var(--ink-rgb) / 0.45);
    color: var(--text-strong);
  }
  .mid {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .nm4 {
    font-size: 12px;
  }
  .kept .nm4 {
    font-weight: 600;
  }
  .wv {
    position: relative;
    height: 14px;
  }
  .d {
    font-size: 10.5px;
    text-align: right;
  }
  .note {
    font-size: 12px;
    padding: 4px;
  }
  /* no room beside the timeline: it lies over it, and steps aside for a drag */
  .panel.over {
    z-index: 30;
    width: min(372px, calc(100% - 32px));
    transition: opacity 0.16s var(--ease-move);
  }
  :global(body.sound-drag) .panel.over {
    opacity: 0;
    pointer-events: none;
  }
</style>
