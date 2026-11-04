<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { fmt, clamp } from '../lib/format';
  import { charColor, charLabel, itemShade, rgba } from '../lib/colors';
  import { openEditor } from '../lib/keys';

  let mapEl: HTMLDivElement | undefined = $state();
  let zoom = $state(1);
  let start = $state(0);
  const total = $derived(Math.max(0.01, app.view?.duration_s ?? 0.01));
  /** The playhead on the map: it waits at the end of the scene while the room rings out. */
  const head = $derived(Math.min(app.pos, total));
  const cur = $derived(app.cur);
  const curShade = $derived(cur ? itemShade(app.view, cur) : ([240, 238, 232] as [number, number, number]));
  const fav = $derived(cur?.take_index != null ? cur.takes[cur.take_index]?.favorite : false);

  // segments: lines in their take shade, pauses neutral
  const segs = $derived.by(() => {
    const out: { w: number; style: string; lbl: string }[] = [];
    const lines = app.lines.filter((l) => l.end_s > l.start_s);
    let t = 0;
    const ci = app.curIndex;
    for (const l of lines) {
      if (l.start_s > t + 1e-4) out.push({ w: l.start_s - t, style: 'background: var(--glass-fill-4)', lbl: '' });
      const i = app.lines.indexOf(l);
      const a = i === ci ? 1 : i < ci ? 0.78 : 0.4;
      out.push({
        w: l.end_s - l.start_s,
        style: `background: ${rgba(itemShade(app.view, l), a)}`,
        lbl: zoom >= 2.5 ? charLabel(app.view, l.character).charAt(0) : '',
      });
      t = Math.max(t, l.end_s);
    }
    if (total > t) out.push({ w: total - t, style: 'background: var(--glass-fill-4)', lbl: '' });
    return out;
  });

  const viewStart = $derived.by(() => {
    let vs = clamp(start, 0, 1 - 1 / zoom);
    const ef = head / total;
    if (app.playing && (ef < vs || ef > vs + 1 / zoom)) vs = clamp(ef - 0.1 / zoom, 0, 1 - 1 / zoom);
    return vs;
  });

  function seekAt(clientX: number) {
    if (!mapEl) return;
    const r = mapEl.getBoundingClientRect();
    const fx = clamp((clientX - r.left) / r.width, 0, 0.9999);
    app.seek(clamp(viewStart + fx / zoom, 0, 1) * total);
  }

  function down(e: MouseEvent) {
    e.preventDefault();
    seekAt(e.clientX);
    const move = (ev: MouseEvent) => seekAt(ev.clientX);
    const up = () => {
      window.removeEventListener('mousemove', move);
      window.removeEventListener('mouseup', up);
    };
    window.addEventListener('mousemove', move);
    window.addEventListener('mouseup', up);
  }

  function wheel(e: WheelEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (!mapEl) return;
    const r = mapEl.getBoundingClientRect();
    const fx = clamp((e.clientX - r.left) / r.width, 0, 1);
    if (e.metaKey || e.ctrlKey) {
      const nz = clamp(zoom * Math.exp(-e.deltaY * 0.01), 1, 16);
      const tf = viewStart + fx / zoom;
      start = clamp(tf - fx / nz, 0, 1 - 1 / nz);
      zoom = nz;
    } else {
      const dl = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
      start = clamp(viewStart + dl / r.width / zoom, 0, 1 - 1 / zoom);
    }
  }

  $effect(() => {
    const el = mapEl;
    if (!el) return;
    el.addEventListener('wheel', wheel, { passive: false });
    return () => el.removeEventListener('wheel', wheel);
  });

  const mid = $derived.by(() => {
    const z = zoom > 1.05 ? `zoom ×${zoom.toFixed(1)} · ` : '⌘ scroll to zoom · ';
    if (app.page === 'arrange') return z + 'drag ⠿ to reorder, drag a pause to stretch or overlap';
    if (!cur) return z;
    const n = cur.takes.length;
    const k = (cur.take_index ?? 0) + 1;
    return z + (n ? `${charLabel(app.view, cur.character)} · take ${k} of ${n}${fav ? ' · ★' : ''}` : `${charLabel(app.view, cur.character)} · no takes`);
  });
  const nameColor = $derived(cur ? charColor(app.view, cur.character).name : 'var(--text-3)');
</script>

<section class="glass transport" aria-label="Transport">
  <button class="play" onclick={() => app.togglePlay()} aria-label="Play or stop (Space)" data-playing={app.playing} style="box-shadow: 0 0 0 6px {app.playing ? rgba(curShade, 0.3) : 'var(--glass-fill-3)'}">
    {#if app.playing}
      <svg width="18" height="18" viewBox="0 0 24 24" aria-hidden="true"><rect x="5" y="5" width="14" height="14" rx="3" fill="var(--text-on-light)" /></svg>
    {:else}
      <svg width="20" height="20" viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5v14l11-7z" fill="var(--text-on-light)" /></svg>
    {/if}
  </button>
  <div class="mid">
    <div class="times"><span data-testid="time-now">{fmt(head)}</span><span class="ellip">{mid}</span><span>{fmt(total)}</span></div>
    <div class="map" bind:this={mapEl} role="slider" tabindex="-1" aria-label="Scene position. Command scroll zooms, Shift scroll moves" aria-valuenow={Math.round((head / total) * 100)} onmousedown={down} title="Click or drag to move the playhead. ⌘ scroll zooms, ⇧ scroll moves">
      <div class="inner" style="left: {-viewStart * zoom * 100}%; width: {zoom * 100}%">
        {#each segs as s, i (i)}<span class="mseg" style="flex: {s.w} 1 0; {s.style}">{s.lbl}</span>{/each}
        <span class="vline" style="background: var(--locator); top: -6px; bottom: -6px; left: {(app.locator / total) * 100}%"></span>
        <span class="flag" style="top: -6px; left: {(app.locator / total) * 100}%"></span>
        <span class="vline head" style="left: {(head / total) * 100}%"></span>
      </div>
    </div>
  </div>
  <div class="btns">
    <button class="icb" onclick={() => app.toggleFav()} aria-label="Favorite take (F)" title="Favorite (F)" style="color: {fav ? nameColor : 'var(--text-3)'}; background: {fav ? rgba(curShade, 0.16) : 'transparent'}">
      <svg width="20" height="20" viewBox="0 0 24 24" fill={fav ? nameColor : 'none'} stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" aria-hidden="true"><polygon points="12 3 14.8 8.7 21 9.6 16.5 14 17.6 20.2 12 17.3 6.4 20.2 7.5 14 3 9.6 9.2 8.7" /></svg>
    </button>
    <button class="icb" onclick={openEditor} aria-label="Open box editor (E)" title="Box editor (E)">
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" aria-hidden="true"><path d="M4 20h4L19 9l-4-4L4 16v4z" /></svg>
    </button>
    <button class="icb" onclick={() => app.playFromLocator()} aria-label="Play from locator (Shift Space)" title="Play from locator (Shift Space)">
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="4 6 4 18" /><path d="M9 6v12l9-6z" /></svg>
    </button>
  </div>
</section>
<div class="hint">
  {#if app.page === 'options'}Space play · ⇧Space from locator · ← → takes · ↑ ↓ or scroll for lines · I O in/out · F favorite · E edit · ? all keys
  {:else if app.page === 'arrange'}Space play · ⇧Space from locator · click waveform for locator · I O in/out · ⌥↑ ⌥↓ move · ? all keys
  {:else}Space play · drag clips (⌥ moves one clip alone) · ⌥← ⌥→ nudge · B bucket · ? all keys{/if}
</div>

<style>
  .transport {
    position: absolute;
    left: 50%;
    bottom: 30px;
    transform: translateX(-50%);
    width: min(820px, 94%);
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 10px;
    border-radius: var(--radius-panel);
    background: var(--panel);
    z-index: 10;
  }
  .play {
    flex: none;
    width: 52px;
    height: 52px;
    border-radius: 50%;
    background: var(--button-light);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .mid {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .times {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    font-size: var(--fs-small);
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .map {
    position: relative;
    height: 24px;
    cursor: pointer;
    overflow: hidden;
    border-radius: 6px;
  }
  .inner {
    position: absolute;
    top: 6px;
    bottom: 6px;
    display: flex;
  }
  .mseg {
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    white-space: nowrap;
    font-size: 10px;
    font-weight: 700;
    line-height: 1;
    color: rgb(var(--night-rgb) / 0.8);
    box-shadow: inset -1px 0 0 rgb(var(--night-rgb) / 0.75);
  }
  .head {
    top: -6px;
    bottom: -6px;
    width: 3px;
    border-radius: 2px;
    background: var(--playhead);
    box-shadow: 0 0 8px rgb(var(--shadow-rgb) / 0.6);
  }
  .btns {
    display: flex;
    gap: 2px;
    flex: none;
  }
  .hint {
    position: absolute;
    left: 50%;
    bottom: 8px;
    transform: translateX(-50%);
    font-size: var(--fs-small);
    color: var(--text-4);
    white-space: nowrap;
    z-index: 10;
  }
</style>
