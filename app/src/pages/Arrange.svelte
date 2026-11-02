<script lang="ts">
  import ClipState from '../components/ClipState.svelte';
  import { tick } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import { clamp, fmt } from '../lib/format';
  import { charColor, charLabel, itemShade, rgba } from '../lib/colors';
  import { openEditor } from '../lib/keys';
  import Waveform from '../components/Waveform.svelte';
  import type { ItemView } from '../lib/types';

  const sc = $derived(app.scale);
  const S = $derived(Math.round(92 * sc));

  interface Row {
    it: ItemView;
    j: number;
    gapAfter: number | null; // pause before the next line
    acts: string[]; // action lines in that pause
    next: ItemView | null;
  }

  const rows = $derived.by(() => {
    const v = app.view;
    if (!v) return [] as Row[];
    const out: Row[] = [];
    const lines = v.items.filter((i) => i.kind === 'line');
    lines.forEach((it, j) => {
      const next = lines[j + 1] ?? null;
      const acts: string[] = [];
      if (next) {
        const a = v.items.indexOf(it);
        const b = v.items.indexOf(next);
        for (const x of v.items.slice(a + 1, b)) if (x.kind === 'spacer') acts.push(x.text);
      }
      out.push({ it, j, gapAfter: next ? next.gap_before_s : null, acts, next });
    });
    return out;
  });

  // local previews while dragging; committed on mouse up
  let gapPreview = $state<{ id: string; g: number } | null>(null);
  // in and out in source seconds
  let trimPreview = $state<{ id: string; a: number; z: number } | null>(null);
  let drag = $state<{ id: string; dy: number; y0: number } | null>(null);
  let rowEls: Record<string, HTMLDivElement> = $state({});
  let off = $state(0);

  $effect(() => {
    void app.curId;
    void app.view;
    void sc;
    void tick().then(() => {
      const el = app.curId ? rowEls[app.curId] : undefined;
      if (el) off = el.offsetTop + el.offsetHeight / 2;
    });
  });

  function takeOf(it: ItemView) {
    return it.takes[it.take_index ?? 0];
  }

  /** Source stretch a strip shows: the take plus handles to lengthen into (none on comps). */
  function win(it: ItemView) {
    const t = takeOf(it);
    if (!t || t.duration <= 0) return null;
    const base = it.comp.length ? 0 : clamp(t.duration * 0.4, 0.75, 2);
    const pre = base ? Math.min(t.src_in, Math.max(0, -it.head_s) + base) : 0;
    const post = base ? Math.min(Math.max(0, t.source_len - t.src_out), Math.max(0, -it.tail_s) + base) : 0;
    return { a: t.src_in - pre, z: t.src_out + post };
  }
  /** Position of a source time on the strip (0..1). */
  function fx(it: ItemView, s: number) {
    const w = win(it);
    return w ? clamp((s - w.a) / Math.max(0.001, w.z - w.a), 0, 1) : 0;
  }

  function trims(it: ItemView) {
    if (trimPreview?.id === it.line_id) return trimPreview;
    const t = takeOf(it);
    if (!t) return { id: it.line_id, a: 0, z: 0 };
    return { id: it.line_id, a: t.src_in + it.head_s, z: t.src_out - it.tail_s };
  }

  function inWord(it: ItemView, s: number) {
    const t = takeOf(it);
    if (!t) return false;
    const x = s - t.src_in;
    return t.words.some((w) => x > w.t0 + 0.003 && x < w.t1 - 0.003);
  }

  // waveforms of the strips' handles, fetched once per stretch
  let stripPeaks = $state<Record<string, number[]>>({});
  const winKey = (src: string, a: number, z: number) => `${src}|${a.toFixed(3)}|${z.toFixed(3)}`;
  function peaksOf(it: ItemView): number[] {
    const t = takeOf(it);
    const w = win(it);
    if (!t || !w) return [];
    const got = stripPeaks[winKey(t.source_id, w.a, w.z)];
    if (got?.length) return got;
    // until they arrive: the take's own envelope with flat handles
    const per = t.peaks.length / t.duration;
    const flat = (s: number) => new Array<number>(Math.round(s * per)).fill(0);
    return [...flat(t.src_in - w.a), ...t.peaks, ...flat(w.z - t.src_out)];
  }
  $effect(() => {
    const want: { key: string; source_id: string; t0: number; t1: number; buckets: number }[] = [];
    for (const r of rows) {
      const t = takeOf(r.it);
      const w = win(r.it);
      if (!t || !w || (w.a >= t.src_in && w.z <= t.src_out)) continue;
      const key = winKey(t.source_id, w.a, w.z);
      if (!(key in stripPeaks)) want.push({ key, source_id: t.source_id, t0: w.a, t1: w.z, buckets: Math.min(480, Math.round((t.peaks.length * (w.z - w.a)) / t.duration)) });
    }
    if (!want.length) return;
    void call<number[][]>('peaks_batch', { ranges: want.map((m) => ({ source_id: m.source_id, t0: m.t0, t1: m.t1, buckets: m.buckets, normalize: true })) })
      .then((r) => {
        const next = { ...stripPeaks };
        want.forEach((m, i) => (next[m.key] = r[i] ?? []));
        stripPeaks = next;
      })
      .catch(() => {
        const next = { ...stripPeaks };
        for (const m of want) next[m.key] = [];
        stripPeaks = next;
      });
  });

  function gapOf(r: Row) {
    if (!r.next) return 0;
    return gapPreview?.id === r.next.line_id ? gapPreview.g : (r.gapAfter ?? 0);
  }

  function track(e: MouseEvent, move: (ev: MouseEvent) => void, up: (ev: MouseEvent) => void) {
    e.preventDefault();
    e.stopPropagation();
    const mv = (ev: MouseEvent) => move(ev);
    const u = (ev: MouseEvent) => {
      window.removeEventListener('mousemove', mv);
      window.removeEventListener('mouseup', u);
      up(ev);
    };
    window.addEventListener('mousemove', mv);
    window.addEventListener('mouseup', u);
  }

  function grabGap(e: MouseEvent, r: Row) {
    if (!r.next) return;
    const id = r.next.line_id;
    const g0 = r.gapAfter ?? 0;
    const y0 = e.clientY;
    track(
      e,
      (ev) => (gapPreview = { id, g: clamp(Math.round((g0 + (ev.clientY - y0) / (26 * sc)) * 20) / 20, -1, 4) }),
      async () => {
        const g = gapPreview?.g;
        gapPreview = null;
        if (g != null && Math.abs(g - g0) > 0.001) await app.cmd({ cmd: 'set_gap', line_id: id, gap_s: g });
      },
    );
  }

  function grabTrim(e: MouseEvent, it: ItemView, edge: 'h' | 't') {
    const strip = (e.currentTarget as HTMLElement).parentElement;
    const w = win(it);
    if (!strip || !w) return;
    const rect = strip.getBoundingClientRect();
    const base = trims(it);
    track(
      e,
      (ev) => {
        const s = w.a + clamp((ev.clientX - rect.left) / rect.width, 0, 1) * (w.z - w.a);
        const cur = trimPreview ?? { ...base };
        if (edge === 'h') cur.a = Math.min(s, cur.z - 0.1);
        else cur.z = Math.max(s, cur.a + 0.1);
        trimPreview = { ...cur };
      },
      async () => {
        const p = trimPreview;
        trimPreview = null;
        const t = takeOf(it);
        if (!p || !t) return;
        await app.cmd({ cmd: 'set_trim', line_id: it.line_id, head_s: +(p.a - t.src_in).toFixed(3), tail_s: +(t.src_out - p.z).toFixed(3) });
      },
    );
  }

  function scrub(e: MouseEvent, it: ItemView) {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const w = win(it);
    const set = (x: number) => {
      if (!w) return;
      const tr = trims(it);
      const s = clamp(w.a + ((x - rect.left) / rect.width) * (w.z - w.a), tr.a, tr.z);
      app.curId = it.line_id;
      const at = it.start_s + (s - tr.a);
      app.locator = at;
      if (!app.playing) app.pos = at;
    };
    set(e.clientX);
    track(e, (ev) => set(ev.clientX), () => {});
  }

  function grabRow(e: MouseEvent, it: ItemView) {
    const y0 = e.clientY;
    drag = { id: it.line_id, dy: 0, y0 };
    track(
      e,
      (ev) => drag && (drag = { ...drag, dy: ev.clientY - y0 }),
      async () => {
        const d = drag;
        drag = null;
        const t = dropIndex(d);
        if (d == null || t == null || !app.view) return;
        const lines = app.view.items.filter((i) => i.kind === 'line');
        const others = lines.filter((l) => l.line_id !== d.id);
        // target position in the full item list: before the line at t, or at the end
        const before = others[t];
        const fromIdx = app.view.items.findIndex((i) => i.line_id === d.id);
        let to = before ? app.view.items.findIndex((i) => i.line_id === before.line_id) : app.view.items.length;
        if (fromIdx < to) to -= 1;
        if (to !== fromIdx) await app.cmd({ cmd: 'move_item', scene_id: app.view.scene.id, item_id: d.id, to_index: to });
      },
    );
  }

  function dropIndex(d: { id: string; dy: number } | null): number | null {
    if (!d) return null;
    const el = rowEls[d.id];
    if (!el) return null;
    const center = el.offsetTop + d.dy + el.offsetHeight / 2;
    let n = 0;
    for (const r of rows) {
      if (r.it.line_id === d.id) continue;
      const o = rowEls[r.it.line_id];
      if (o && o.offsetTop + o.offsetHeight / 2 < center) n++;
    }
    return n;
  }

  const dropY = $derived.by(() => {
    if (!drag) return null;
    const t = dropIndex(drag);
    const others = rows.filter((r) => r.it.line_id !== drag!.id);
    if (t == null) return null;
    const target = others[t];
    if (target) return (rowEls[target.it.line_id]?.offsetTop ?? 0) - 6;
    const last = others[others.length - 1];
    return last ? (rowEls[last.it.line_id]?.offsetTop ?? 0) + S + 6 : 0;
  });

  function rowStyle(r: Row, prevGap: number) {
    const shd = itemShade(app.view, r.it);
    const isCur = r.it.line_id === app.curId;
    const dragging = drag?.id === r.it.line_id;
    const ov = prevGap < 0 ? Math.round(-prevGap * 44 * sc) : 0;
    return `height: ${S}px; margin-top: ${-ov}px; z-index: ${dragging ? 30 : 2}; transform: translateY(${dragging ? drag!.dy.toFixed(1) : 0}px)${dragging ? ' rotate(-1deg)' : ''}; transition: ${dragging ? 'none' : 'transform .2s'}; mix-blend-mode: ${ov > 0 ? 'screen' : 'normal'}; background: linear-gradient(90deg, ${rgba(shd, isCur ? 0.44 : 0.28)}, ${rgba(shd, 0.1)} 60%, rgb(var(--veil-rgb) / 0.03)); box-shadow: inset 0 0 0 ${isCur ? 1.5 : 1}px ${rgba(shd, isCur ? 0.75 : 0.2)}${dragging ? ', 0 30px 60px rgb(var(--shadow-rgb) / 0.55)' : isCur ? `, 0 16px 60px ${rgba(shd, 0.22)}` : ''}`;
  }
</script>

<div class="arr" style="width: min({Math.round(600 * sc)}px, 86%); transform: translate(-50%, -{off.toFixed(1)}px); transition: {drag ? 'none' : 'transform .3s var(--ease-out)'}">
  {#each rows as r, i (r.it.line_id)}
    {@const t = takeOf(r.it)}
    {@const tr = trims(r.it)}
    {@const hc = inWord(r.it, tr.a)}
    {@const tc = inWord(r.it, tr.z)}
    {@const h = fx(r.it, tr.a)}
    {@const tl = fx(r.it, tr.z)}
    {@const shd = itemShade(app.view, r.it)}
    {@const prevGap = i > 0 ? gapOf(rows[i - 1]) : 0}
    <div class="snip" class:clip-muted={r.it.muted} class:clip-dropped={r.it.dropped} bind:this={rowEls[r.it.line_id]} style={rowStyle(r, prevGap)} onclick={() => app.select(r.it.line_id)} ondblclick={() => { app.select(r.it.line_id); openEditor(); }} role="button" tabindex="-1" onkeydown={() => {}} data-testid="arrange-row">
      <div class="top">
        <span class="grip" onmousedown={(e) => grabRow(e, r.it)} title="Drag to reorder" aria-label="Drag to reorder" role="button" tabindex="-1">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><circle cx="9" cy="6" r="1.6" /><circle cx="15" cy="6" r="1.6" /><circle cx="9" cy="12" r="1.6" /><circle cx="15" cy="12" r="1.6" /><circle cx="9" cy="18" r="1.6" /><circle cx="15" cy="18" r="1.6" /></svg>
        </span>
        <span class="nm who" style="color: {charColor(app.view, r.it.character).name}">{charLabel(app.view, r.it.character)}</span>
        <span class="script ellip txt" style="font-size: {Math.round(15 * sc)}px">{#if r.it.words.length && !r.it.dropped}{#each r.it.words.filter((w) => !w.deleted) as w, wi (w.idx)}{wi ? ' ' : ''}<span class:wmuted={w.muted}>{w.text}</span>{/each}{:else}{r.it.text}{/if}</span>
        <ClipState it={r.it} />
        {#if r.it.moved}<span class="badge moved">Moved</span>{/if}
        {#if hc || tc}<span class="badge cuts">Cuts a word</span>{/if}
        <span class="dur" data-testid="row-dur">{(r.it.end_s - r.it.start_s).toFixed(1)} s</span>
      </div>
      <div class="strip" onmousedown={(e) => scrub(e, r.it)} onclick={(e) => e.stopPropagation()} onkeydown={() => {}} title="Click to set the locator, I / O to set in and out" style="height: {Math.round(30 * sc)}px" role="slider" tabindex="-1" aria-valuenow={0}>
        {#if t}
          <Waveform peaks={peaksOf(r.it)} fill={rgba(shd, 0.85)} />
          <div class="shade" style="left: 0; width: {h * 100}%"></div>
          <div class="shade" style="right: 0; width: {(1 - tl) * 100}%"></div>
          {#each [fx(r.it, t.src_in), fx(r.it, t.src_out)] as e, k (k)}{#if e > 0.001 && e < 0.999}<span class="tedge" style="left: {e * 100}%" title="Where the take was cut"></span>{/if}{/each}
          {#if r.it.line_id === app.curId}
            {@const lf = fx(r.it, tr.a + app.locator - r.it.start_s)}
            {@const pf = fx(r.it, tr.a + app.pos - r.it.start_s)}
            <span class="vline" style="background: var(--locator); left: {lf * 100}%"></span><span class="flag" style="left: {lf * 100}%"></span>
            {#if app.playing || app.pos > r.it.start_s}<span class="vline phead" style="left: {pf * 100}%"></span>{/if}
          {/if}
          <span class="hdl" role="slider" tabindex="-1" aria-label="Clip start" aria-valuenow={h} onmousedown={(e) => grabTrim(e, r.it, 'h')} title="Drag to trim the head, or out to lengthen it" style="left: calc({h * 100}% - 5px); background: {hc ? 'var(--warn)' : 'rgb(var(--ink-rgb) / 0.9)'}"></span>
          <span class="hdl" role="slider" tabindex="-1" aria-label="Clip end" aria-valuenow={tl} onmousedown={(e) => grabTrim(e, r.it, 't')} title="Drag to trim the tail, or out to lengthen it" style="left: calc({tl * 100}% - 5px); background: {tc ? 'var(--warn)' : 'rgb(var(--ink-rgb) / 0.9)'}"></span>
        {:else}
          <span class="none">no takes</span>
        {/if}
      </div>
    </div>
    {#if r.next}
      {@const g = gapOf(r)}
      <div class="gaprow" onmousedown={(e) => grabGap(e, r)} title="Drag up or down to change the pause" style="height: {Math.round((18 + Math.max(g, 0) * 14) * sc)}px" role="slider" tabindex="-1" aria-valuenow={g} data-testid="gap-row">
        <div class="gapline"></div>
        <div class="gappill" style="color: {g < 0 ? 'var(--overlap)' : 'rgb(var(--ink-rgb) / 0.78)'}">
          <span class="num">{g < 0 ? `overlap ${(-g).toFixed(2)} s` : `‖ ${g.toFixed(2)} s`}</span>
          {#if r.acts.length}<span class="script ellip actx">{r.acts.join(' ')}</span>{/if}
        </div>
      </div>
    {/if}
  {/each}
  {#if dropY != null}<div class="drop" style="top: {dropY}px"></div>{/if}
</div>
<div class="side tchip" style="left: calc(50% - {Math.round(300 * sc)}px - 24px)">{fmt(app.pos)}</div>

<style>
  .arr {
    position: absolute;
    left: 50%;
    top: 50%;
  }
  .wmuted {
    color: var(--text-dim);
  }
  .snip {
    position: relative;
    box-sizing: border-box;
    border-radius: var(--radius-snip);
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: 6px;
    cursor: pointer;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .grip {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 28px;
    border-radius: 7px;
    cursor: grab;
    color: rgb(var(--ink-rgb) / 0.5);
  }
  .grip:hover {
    background: rgb(var(--veil-rgb) / 0.1);
    color: var(--text-strong);
  }
  .who {
    flex: none;
    width: 76px;
  }
  .txt {
    flex: 1;
    min-width: 0;
    color: rgb(var(--ink-rgb) / 0.86);
  }
  .moved {
    color: var(--text-strong);
    box-shadow: inset 0 0 0 1px rgb(var(--ink-rgb) / 0.4);
  }
  .cuts {
    color: var(--text-on-pill);
    background: var(--warn);
  }
  .dur {
    flex: none;
    font-size: var(--fs-small);
    color: rgb(var(--ink-rgb) / 0.58);
    font-variant-numeric: tabular-nums;
  }
  .strip {
    position: relative;
    border-radius: 8px;
    overflow: hidden;
    background: rgb(var(--shadow-rgb) / 0.25);
    cursor: text;
  }
  .shade {
    position: absolute;
    top: 0;
    bottom: 0;
    pointer-events: none;
    background: var(--trim-shade);
  }
  .tedge {
    position: absolute;
    top: 3px;
    bottom: 3px;
    width: 0;
    border-left: 1px dashed rgb(var(--ink-rgb) / 0.35);
    pointer-events: none;
  }
  .phead {
    background: var(--playhead);
    box-shadow: 0 0 8px var(--playhead-glow);
  }
  .hdl {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 10px;
    border-radius: 3px;
    cursor: ew-resize;
    box-shadow: 0 0 0 1px rgb(var(--shadow-rgb) / 0.45);
    z-index: 3;
  }
  .none {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    color: var(--text-5);
  }
  .gaprow {
    position: relative;
    z-index: 6;
    cursor: ns-resize;
  }
  .gapline {
    position: absolute;
    left: 24px;
    right: 24px;
    top: 50%;
    height: 1px;
    background: rgb(var(--veil-rgb) / 0.09);
  }
  .gappill {
    position: absolute;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: 92%;
    padding: 3px 10px;
    border-radius: var(--radius-pill);
    background: var(--pill);
    font-size: var(--fs-small);
    white-space: nowrap;
    box-shadow: inset 0 0 0 1px rgb(var(--veil-rgb) / 0.09);
  }
  .gaprow:hover .gappill {
    color: var(--text-strong);
    box-shadow: inset 0 0 0 1px rgb(var(--veil-rgb) / 0.35);
  }
  .num {
    font-variant-numeric: tabular-nums;
    font-weight: 600;
  }
  .actx {
    max-width: 320px;
    color: var(--text-4);
  }
  .drop {
    position: absolute;
    left: -10px;
    right: -10px;
    height: 2px;
    border-radius: 2px;
    background: var(--playhead);
    box-shadow: 0 0 14px var(--playhead-glow);
    z-index: 40;
  }
  .tchip {
    position: absolute;
    top: 50%;
    transform: translate(-100%, -50%);
    font-size: var(--fs-small);
    font-variant-numeric: tabular-nums;
    color: var(--text-3);
    padding: 4px 8px;
    border-radius: var(--radius-chip);
    background: var(--chip);
  }
</style>
