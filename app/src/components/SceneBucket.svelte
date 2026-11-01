<script lang="ts">
  // The Edit page's docked bucket: only this scene's buckets. Gold = downloaded, outlined =
  // preview that downloads when dropped on a lane. Rows drag onto the timeline.
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import { len } from '../lib/format';
  import Icon from './Icon.svelte';
  import Waveform from './Waveform.svelte';
  import type { BucketsView, DragSound } from '../lib/types';
  import { colorOf, solid } from '../theme';

  let data = $state<BucketsView | null>(null);
  let closed = $state<Record<string, boolean>>({});
  let busy = $state<string | null>(null);

  async function load() {
    if (!app.sceneId) return;
    data = (await app.run(() => call<BucketsView>('buckets', { scene: app.sceneId }))) ?? data;
  }
  $effect(() => {
    void app.sceneId;
    void app.status?.rev;
    void load();
  });

  function isOpen(id: string, i: number) {
    return closed[id] === undefined ? i < 2 : !closed[id];
  }

  function drag(e: DragEvent, s: DragSound) {
    e.dataTransfer?.setData('application/x-cutaway-bucket', JSON.stringify(s));
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'copy';
  }

  function toggle(key: { asset_id?: string; remote_id?: string }) {
    const id = key.asset_id ?? key.remote_id;
    if (app.playing && app.sound === id) void app.stop();
    else void app.playSound(key);
  }

  async function more(id: string) {
    busy = id;
    await app.run(() => call('bucket_search', { bucket_id: id, more: true }));
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

<aside class="glass panel" aria-label="Bucket for scene {app.view?.scene.number ?? ''}">
  <div class="hd">
    <span class="t">Bucket · Sc. {app.view?.scene.number ?? ''}</span>
    <button class="btn" onclick={() => app.toggleFullBucket()}>Full bucket <span class="kb">⇧B</span></button>
    <button class="ib" aria-label="Hide bucket (B)" title="Hide (B)" onclick={() => (app.editBucket = false)}><Icon name="chevron-right" /></button>
  </div>
  <div class="legend muted">
    <span><span class="dot gold"></span>Downloaded</span>
    <span><span class="dot ring"></span>Preview, downloads on drop</span>
  </div>
  <div class="list">
    {#each data?.buckets ?? [] as b, i (b.id)}
      {@const open = isOpen(b.id, i)}
      {@const col = b.character ? solid(colorOf(b.color_index ?? 0).rgb) : null}
      <section class="bkt" aria-label={b.name}>
        <button class="bh" aria-expanded={open} onclick={() => (closed = { ...closed, [b.id]: open })}>
          <span class="chev" style="transform: rotate({open ? 0 : -90}deg)"><Icon name="chevron" size={11} stroke={2.4} /></span>
          {#if col}<span class="dot" style="background: {col}"></span>{/if}
          <span class="bn">{b.name}</span>
          <span class="muted tn cnt">{col ? b.kept.length : `${b.kept.length} + ${b.previews.length}`}</span>
        </button>
        {#if open}
          {#each b.kept as s (s.id)}
            <div class="srow kept" style={col ? `background: color-mix(in srgb, ${col} 12%, transparent)` : ''} draggable="true" role="listitem" ondragstart={(e) => drag(e, { bucket_id: b.id, asset_id: s.id, name: s.name, duration_s: s.duration_s })} data-testid={col ? 'vocal-sound' : 'bucket-sound'}>
              <span class="grip"><Icon name="grip" /></span>
              <button class="play gold" style={col ? `background: ${col}; color: #16110d` : ''} aria-label="Play {s.name}" onclick={() => toggle({ asset_id: s.id })}><Icon name={app.playing && app.sound === s.id ? 'stop' : 'play'} size={10} /></button>
              <div class="mid"><span class="ellip nm4">{s.name}</span><span class="wv"><Waveform peaks={s.peaks} fill={col ?? 'var(--sfx)'} /></span></div>
              <span class="muted tn d">{len(s.duration_s)}</span>
            </div>
          {/each}
          {#each b.previews.slice(0, 6) as p (p.remote_id)}
            <div class="srow" draggable="true" role="listitem" ondragstart={(e) => drag(e, { bucket_id: b.id, remote_id: p.remote_id, name: p.title, duration_s: p.duration_s })} data-testid="bucket-preview">
              <span class="grip"><Icon name="grip" /></span>
              <button class="play ring" aria-label="Play {p.title}" onclick={() => toggle({ remote_id: p.remote_id })}><Icon name={app.playing && app.sound === p.remote_id ? 'stop' : 'play'} size={10} /></button>
              <div class="mid"><span class="ellip nm4 muted">{p.title}</span><span class="wv"><Waveform peaks={pseudo(p.remote_id)} fill="rgb(var(--ink-rgb) / 0.32)" /></span></div>
              <span class="muted tn d">{len(p.duration_s)}</span>
            </div>
          {/each}
          {#if !col && data?.connected.epidemic && b.keywords.length}<button class="btn sm" onclick={() => more(b.id)} disabled={busy === b.id}>{busy === b.id ? 'Searching…' : 'More like these'}</button>{/if}
        {/if}
      </section>
    {:else}
      <p class="muted note">No buckets for this scene. Open the full bucket (⇧B) to spot the script or start one.</p>
    {/each}
  </div>
</aside>

<style>
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
    grid-template-columns: 12px 28px minmax(0, 1fr) 34px;
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
  @media (max-width: 1100px) {
    .panel {
      display: none;
    }
  }
</style>
