<script lang="ts">
  // Free Epidemic Sound search (Import › Sounds): results to audition and drag onto a
  // bucket, or onto "New bucket". Nothing is downloaded until a sound is kept or placed.
  import { app } from '../lib/app.svelte';
  import { grab } from '../lib/drag.svelte';
  import { len } from '../lib/format';
  import Icon from './Icon.svelte';
  import PreviewPlayer from './PreviewPlayer.svelte';
  import Waveform from './Waveform.svelte';
  import type { SearchResult } from '../lib/types';

  let {
    query,
    results,
    more,
    busy,
    onmore,
    onclose,
  }: {
    query: string;
    results: SearchResult[];
    more: boolean;
    busy: boolean;
    onmore: () => void;
    onclose: () => void;
  } = $props();

  function drag(e: MouseEvent, r: SearchResult) {
    const preview = { provider: r.provider, remote_id: r.remote_id, title: r.title, duration_s: r.duration_s };
    grab(e, { kind: 'search', name: r.title, preview });
  }

  const face = (r: SearchResult) => ({ name: r.title, duration_s: r.duration_s, peaks: pseudo(r.remote_id) });

  function playAt(e: MouseEvent, x: SearchResult) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    void app.playSound({ remote_id: x.remote_id }, Math.max(0, ((e.clientX - r.left) / r.width) * x.duration_s), face(x));
  }

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

<aside class="glass side panel" aria-label="Epidemic Sound search">
  <div class="hd">
    <span class="t ellip">“{query}”</span>
    <span class="chip">Epidemic Sound</span>
    <button class="ib" aria-label="Close search" onclick={onclose}><Icon name="x" /></button>
  </div>
  <p class="muted note">Drag a sound onto a bucket, or onto <b>New bucket</b>. It goes in as a preview; nothing is downloaded until you keep or place it.</p>

  <div class="list">
    {#each results as r (r.remote_id)}
      {@const on = app.previewing(r.remote_id)}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions (dragging is the mouse's) -->
      <div class="srow" class:on role="listitem" onmousedown={(e) => drag(e, r)} data-testid="search-result" title="Drag onto a bucket">
        <span class="grip muted"><Icon name="grip" /></span>
        <button class="ib pl" aria-label="Play {r.title}" onclick={() => app.toggleSound({ remote_id: r.remote_id }, face(r))}><Icon name={on ? 'stop' : 'play'} size={11} /></button>
        <div class="mid">
          <span class="ellip nm3">{r.title}{#if r.buckets.length}<span class="muted in">&nbsp;· in {r.buckets.length === 1 ? 'a bucket' : `${r.buckets.length} buckets`}</span>{/if}</span>
          <span class="wv" role="button" tabindex="-1" aria-label="Play {r.title} from here" onclick={(e) => playAt(e, r)} onkeydown={() => {}}>
            <Waveform peaks={pseudo(r.remote_id)} fill="rgb(var(--ink-rgb) / 0.32)" />
            {#if on}<span class="ph" style="left: {progress(r.duration_s)}%"></span>{/if}
          </span>
        </div>
        <span class="muted tn d">{len(r.duration_s)}</span>
      </div>
    {:else}
      <p class="muted note">{busy ? 'Searching…' : 'Nothing found.'}</p>
    {/each}
    {#if more && results.length}<button class="btn more" onclick={onmore} disabled={busy}>{busy ? 'Loading…' : 'Load more'}</button>{/if}
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
    gap: 12px;
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
  .srow {
    display: grid;
    grid-template-columns: 12px 32px minmax(0, 1fr) 40px;
    gap: 10px;
    align-items: center;
    padding: 8px;
    border-radius: 14px;
    cursor: grab;
  }
  .srow:hover,
  .srow.on {
    background: var(--glass-fill-2);
  }
  .grip {
    display: flex;
    justify-content: center;
  }
  .pl {
    border-radius: 50%;
    box-shadow: inset 0 0 0 1.5px rgb(var(--ink-rgb) / 0.5);
    color: var(--text-strong);
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
  .in {
    font-weight: 400;
  }
  .wv {
    position: relative;
    display: block;
    height: 22px;
    cursor: pointer;
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
</style>
