<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { clamp, fmt } from '../lib/format';
  import { charColor, charLabel, itemShade, rgba } from '../lib/colors';
  import Waveform from '../components/Waveform.svelte';
  import type { ItemView, PlacementView } from '../lib/types';

  let pxPerS = $state(60);
  let scroller: HTMLDivElement | undefined = $state();
  let sel = $state<{ kind: 'line' | 'sound'; id: string } | null>(null);
  let drag = $state<{ id: string; kind: 'line' | 'sound'; dx: number } | null>(null);

  const v = $derived(app.view);
  const ev = $derived(app.edit);
  const duration = $derived(Math.max(5, (v?.duration_s ?? 0) + 3, ...(ev?.placements.map((p) => p.start_s + p.len_s + 1) ?? [0])));
  const width = $derived(duration * pxPerS);
  const lines = $derived(app.lines.filter((l) => l.pieces.length > 0));
  const roles = $derived((v?.characters ?? []).filter((c) => app.lines.some((l) => l.character === c.id)));
  const sfxLayers = $derived(Math.max(1, ...(ev?.placements.filter((p) => p.lane === 'sfx').map((p) => p.layer + 1) ?? [1])));
  // one action line per cluster in the script strip, so texts don't pile up
  const spacers = $derived.by(() => {
    const out: ItemView[] = [];
    let last = -Infinity;
    for (const s of v?.items.filter((i) => i.kind === 'spacer') ?? []) {
      if (s.start_s * pxPerS - last < 160) continue;
      out.push(s);
      last = s.start_s * pxPerS;
    }
    return out;
  });

  $effect(() => {
    if (app.page === 'edit' && !app.edit) void app.loadEdit();
  });
  $effect(() => {
    void v?.rev;
    if (app.page === 'edit') void app.loadEdit();
  });

  function x(t: number) {
    return t * pxPerS;
  }

  function fit() {
    if (scroller) pxPerS = clamp((scroller.clientWidth - 40) / Math.max(1, duration), 8, 400);
  }
  $effect(() => {
    void duration;
    if (scroller && pxPerS === 60) fit();
  });

  function wheel(e: WheelEvent) {
    if (!(e.metaKey || e.ctrlKey)) return;
    e.preventDefault();
    pxPerS = clamp(pxPerS * Math.exp(-e.deltaY * 0.01), 8, 400);
  }
  $effect(() => {
    const el = scroller;
    if (!el) return;
    el.addEventListener('wheel', wheel, { passive: false });
    return () => el.removeEventListener('wheel', wheel);
  });

  function track(e: MouseEvent, id: string, kind: 'line' | 'sound', done: (dt: number, alt: boolean) => Promise<void>) {
    e.preventDefault();
    e.stopPropagation();
    sel = { kind, id };
    if (kind === 'line') app.select(id);
    const x0 = e.clientX;
    drag = { id, kind, dx: 0 };
    const mv = (ev: MouseEvent) => drag && (drag = { ...drag, dx: ev.clientX - x0 });
    const up = async (ev: MouseEvent) => {
      window.removeEventListener('mousemove', mv);
      window.removeEventListener('mouseup', up);
      const dx = ev.clientX - x0;
      drag = null;
      if (Math.abs(dx) > 3) await done(dx / pxPerS, ev.altKey);
    };
    window.addEventListener('mousemove', mv);
    window.addEventListener('mouseup', up);
  }

  const moveLine = (e: MouseEvent, l: ItemView) =>
    track(e, l.line_id, 'line', async (dt, alt) => {
      await app.cmd({ cmd: 'move_clip', line_id: l.line_id, delta_s: +dt.toFixed(3), ripple: !alt });
      await app.loadEdit();
    });

  async function moveSound(p: PlacementView, dt: number) {
    // anchored sounds keep their anchor and change the offset; pinned ones change their time
    const patch = p.anchored ? { offset_s: +(p.start_s + dt - anchorX(p)).toFixed(3) } : { at_s: +(p.start_s + dt).toFixed(3), offset_s: 0 };
    await app.cmd({ cmd: 'update_placement', placement_id: p.id, patch });
    await app.loadEdit();
  }

  function anchorX(p: PlacementView): number {
    if (!p.anchor) return p.start_s;
    const l = app.lines.find((i) => i.line_id === p.anchor!.line_id);
    if (!l) return p.start_s;
    if (p.anchor.word != null) {
      const w = l.words.find((w) => w.script_idx === p.anchor!.word) ?? l.words[p.anchor.word];
      if (w) return p.anchor.edge === 'end' ? w.end_s : w.start_s;
    }
    return p.anchor.edge === 'end' ? l.end_s : l.start_s;
  }

  function rulerDown(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    app.seek(clamp((e.clientX - r.left) / pxPerS, 0, duration));
  }

  const selSound = $derived(sel?.kind === 'sound' ? ev?.placements.find((p) => p.id === sel!.id) : undefined);
  const selLine = $derived(sel?.kind === 'line' ? app.lines.find((l) => l.line_id === sel!.id) : undefined);

  async function nudgeSel(d: number) {
    if (selSound) await moveSound(selSound, d);
    else if (selLine) {
      await app.cmd({ cmd: 'move_clip', line_id: selLine.line_id, delta_s: d, ripple: true });
      await app.loadEdit();
    }
  }

  function onKey(e: KeyboardEvent) {
    if (app.page !== 'edit' || app.overlay) return;
    if ((e.key === 'ArrowLeft' || e.key === 'ArrowRight') && e.altKey) {
      e.preventDefault();
      e.stopPropagation();
      void nudgeSel((e.key === 'ArrowLeft' ? -1 : 1) * (e.shiftKey ? 0.1 : 0.01));
    } else if ((e.key === 'ArrowLeft' || e.key === 'ArrowRight') && selLine) {
      e.preventDefault();
      e.stopPropagation();
      void app.stepTake(e.key === 'ArrowLeft' ? -1 : 1);
    } else if ((e.key === 'Backspace' || e.key === 'Delete') && selSound) {
      e.preventDefault();
      void app.cmd({ cmd: 'delete_placement', placement_id: selSound.id }).then(() => app.loadEdit());
      sel = null;
    }
  }
  $effect(() => {
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  });

  async function drop(e: DragEvent, lane: 'sfx' | 'ambience' | 'music') {
    e.preventDefault();
    const data = e.dataTransfer?.getData('application/x-cutaway-sound');
    if (!data || !v) return;
    const { asset_id, cue_id, anchored } = JSON.parse(data) as { asset_id: string; cue_id?: string; anchored?: boolean };
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const at = clamp((e.clientX - r.left) / pxPerS, 0, duration);
    await app.method('place_sound', { asset_id, scene: v.scene.id, cue_id, lane, at_s: anchored ? undefined : +at.toFixed(3) });
    await app.loadEdit();
  }

  async function soundPatch(patch: Record<string, unknown>) {
    if (!selSound) return;
    await app.cmd({ cmd: 'update_placement', placement_id: selSound.id, patch });
    await app.loadEdit();
  }

  const laneList = $derived([
    ...roles.map((c) => ({ key: `d:${c.id}`, label: c.label, group: 'dialogue' as const, char: c.id })),
    ...Array.from({ length: sfxLayers }, (_, i) => ({ key: `s:${i}`, label: `SFX ${i + 1}`, group: 'sfx' as const, layer: i })),
    { key: 'amb', label: 'Ambience', group: 'ambience' as const },
    { key: 'mus', label: 'Music', group: 'music' as const },
  ]);
</script>

<div class="edit">
  <div class="tools">
    <span class="muted">Ripple by default · ⌥-drag moves one clip alone · ⌘ scroll zooms · drag sounds from the bucket (B)</span>
    <span class="grow"></span>
    <button class="btn" onclick={() => (app.drawer = 'bucket')}>Bucket <span class="kb">B</span></button>
    <button class="btn" onclick={fit}>Fit</button>
  </div>
  {#if selSound}
    <div class="clipbar glass">
      <b class="ellip">{selSound.name}</b>
      <span>{selSound.anchored ? 'anchored' : 'pinned'} · {fmt(selSound.start_s)}</span>
      <button class="btn" onclick={() => soundPatch({ gain_db: selSound!.gain_db - 1 })}>−1 dB</button>
      <span>{selSound.gain_db.toFixed(1)} dB</span>
      <button class="btn" onclick={() => soundPatch({ gain_db: selSound!.gain_db + 1 })}>+1 dB</button>
      <button class="btn" onclick={() => soundPatch({ fade_in_s: Math.min(2, selSound!.fade_in_s + 0.1) })}>Fade in +</button>
      <button class="btn" onclick={() => soundPatch({ fade_out_s: Math.min(2, selSound!.fade_out_s + 0.1) })}>Fade out +</button>
      {#if selSound.anchored}<button class="btn" onclick={() => app.cmd({ cmd: 'unanchor_placement', placement_id: selSound!.id }).then(() => app.loadEdit())}>Pin to time</button>{/if}
      <button class="btn" onclick={() => app.cmd({ cmd: 'delete_placement', placement_id: selSound!.id }).then(() => { sel = null; return app.loadEdit(); })}>Remove</button>
    </div>
  {:else if selLine}
    <div class="clipbar glass">
      <b>{charLabel(v, selLine.character)} · S{v?.scene.number}.{selLine.number}</b>
      <span>T{(selLine.take_index ?? 0) + 1} of {selLine.takes.length} (← →) · gap {selLine.gap_before_s.toFixed(2)} s{selLine.pinned ? ' · pinned' : ''}</span>
      <button class="btn" onclick={() => app.cmd({ cmd: 'set_clip_gain', line_id: selLine!.line_id, gain_db: selLine!.gain_db - 1 })}>−1 dB</button>
      <span>{selLine.gain_db.toFixed(1)} dB</span>
      <button class="btn" onclick={() => app.cmd({ cmd: 'set_clip_gain', line_id: selLine!.line_id, gain_db: selLine!.gain_db + 1 })}>+1 dB</button>
      {#if selLine.pinned}<button class="btn" onclick={() => app.cmd({ cmd: 'pin_clip', line_id: selLine!.line_id, at_s: null })}>Unpin</button>{:else}<button class="btn" onclick={() => app.cmd({ cmd: 'pin_clip', line_id: selLine!.line_id, at_s: selLine!.start_s })}>Pin to time</button>{/if}
    </div>
  {/if}

  <div class="scroll" bind:this={scroller}>
    <div class="content" style="width: {width + 162}px">
      <!-- script strip -->
      <div class="lane strip">
        <div class="lbl plain"><span class="t">Script</span></div>
        <div class="area" style="width: {width}px">
          {#each lines as l (l.line_id)}<span class="sl" style="left: {x(l.start_s)}px; color: {charColor(v, l.character).name}">{charLabel(v, l.character)}</span>{/each}
          {#each spacers as s (s.line_id)}<span class="sa script ellip" style="left: {x(s.start_s)}px" title={s.text}>{s.text}</span>{/each}
        </div>
      </div>
      <div class="lane">
        <div class="lbl plain"><span class="t">{fmt(app.pos)}</span></div>
        <div class="area ruler" style="width: {width}px" onmousedown={rulerDown} role="slider" tabindex="-1" aria-valuenow={app.pos}>
          {#each Array.from({ length: Math.ceil(duration) + 1 }, (_, i) => i) as s (s)}
            {#if pxPerS > 30 || s % 5 === 0}<span class="tick" style="left: {x(s)}px"></span><span class="tl" style="left: {x(s)}px">{s}s</span>{/if}
          {/each}
          <span class="flag" style="left: {x(app.locator)}px"></span>
        </div>
      </div>
      {#each laneList as ln (ln.key)}
        <div class="lane" class:groupstart={ln.key === 's:0' || ln.key === 'amb'}>
          <div class="lbl" style={ln.group === 'dialogue' ? `box-shadow: inset 3px 0 0 ${charColor(v, ln.char ?? null).name}` : ''}><span class="t2">{ln.label}</span><span class="sub">{ln.group}</span></div>
          <div class="area track" style="width: {width}px" role="list" ondragover={(e) => ln.group !== 'dialogue' && e.preventDefault()} ondrop={(e) => ln.group !== 'dialogue' && drop(e, ln.group as 'sfx' | 'ambience' | 'music')} data-testid="edit-lane">
            {#if ln.group === 'dialogue'}
              {#each lines.filter((l) => l.character === ln.char) as l (l.line_id)}
                {@const shd = itemShade(v, l)}
                {@const t = l.takes[l.take_index ?? 0]}
                {@const dx = drag?.id === l.line_id ? drag.dx : 0}
                <div class="clip" class:sel={sel?.id === l.line_id} role="button" tabindex="-1" onmousedown={(e) => moveLine(e, l)} title="{l.text} (T{(l.take_index ?? 0) + 1})" style="left: {x(l.start_s) + dx}px; width: {Math.max(4, x(l.end_s - l.start_s))}px; background: {rgba(shd, 0.18)}; box-shadow: inset 0 -3px 0 {rgba(shd, 0.9)}{sel?.id === l.line_id ? `, inset 0 0 0 2px ${rgba(shd, 1)}` : ''}">
                  {#if t}<Waveform peaks={t.peaks} fill={rgba(shd, 0.8)} />{/if}
                  <span class="cl">T{(l.take_index ?? 0) + 1} {l.words.filter((w) => !w.deleted).map((w) => w.text).join(' ')}</span>
                  {#if l.pinned}<span class="pin">pinned</span>{/if}
                </div>
              {/each}
            {:else}
              {#each (ev?.placements ?? []).filter((p) => p.lane === ln.group && (ln.group !== 'sfx' || p.layer === ln.layer)) as p (p.id)}
                {@const dx = drag?.id === p.id ? drag.dx : 0}
                <div class="clip snd" class:sel={sel?.id === p.id} role="button" tabindex="-1" onmousedown={(e) => track(e, p.id, 'sound', (dt) => moveSound(p, dt))} title={p.name} style="left: {x(p.start_s) + dx}px; width: {Math.max(4, x(p.len_s))}px">
                  <Waveform peaks={p.peaks} fill="rgba(240,238,232,0.45)" />
                  <span class="cl">{p.name}</span>
                  {#if p.fade_in_s > 0.02}<span class="fade in" style="width: {x(p.fade_in_s)}px"></span>{/if}
                  {#if p.fade_out_s > 0.02}<span class="fade out" style="width: {x(p.fade_out_s)}px"></span>{/if}
                </div>
                {#if p.anchored}<span class="anchor" style="left: {x(anchorX(p))}px; width: {Math.max(1, Math.abs(x(p.start_s + dx / pxPerS) - x(anchorX(p))))}px; transform: translateX({x(p.start_s) + dx < x(anchorX(p)) ? '-100%' : '0'})"></span>{/if}
              {/each}
            {/if}
          </div>
        </div>
      {/each}
      <div class="ph" style="left: {162 + x(app.pos)}px"></div>
    </div>
  </div>
</div>

<style>
  .edit {
    position: absolute;
    left: 84px;
    right: 24px;
    top: 80px;
    bottom: 130px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .tools {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 11.5px;
  }
  .grow {
    flex: 1;
  }
  .clipbar {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 6px 10px;
    border-radius: 16px;
    background: var(--panel);
    font-size: 12px;
  }
  .clipbar .btn {
    height: 28px;
    padding: 0 10px;
  }
  .scroll {
    flex: 1;
    overflow: auto;
    border-radius: 18px;
  }
  .content {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-bottom: 8px;
  }
  .lane {
    display: flex;
    gap: 12px;
    align-items: stretch;
  }
  .lane.groupstart {
    margin-top: 10px;
  }
  .lbl {
    position: sticky;
    left: 0;
    z-index: 4;
    flex: none;
    width: 150px;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 2px;
    padding: 6px 10px 6px 14px;
    border-radius: var(--radius-lane);
    background: var(--panel-solid);
  }
  .lbl.plain {
    background: var(--stage-bg);
  }
  .t {
    font-size: 12px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .t2 {
    font-size: 13px;
    font-weight: 600;
  }
  .sub {
    font-size: 10.5px;
    color: var(--text-4);
  }
  .area {
    position: relative;
    flex: none;
    border-radius: var(--radius-lane);
  }
  .strip .area {
    height: 30px;
  }
  .sl {
    position: absolute;
    top: 2px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: var(--name-tracking);
  }
  .sa {
    position: absolute;
    top: 15px;
    max-width: 220px;
    font-size: 11px;
    color: var(--text-4);
  }
  .ruler {
    height: 26px;
    background: rgba(255, 255, 255, 0.045);
    cursor: ew-resize;
  }
  .tick {
    position: absolute;
    bottom: 0;
    width: 1px;
    height: 8px;
    background: rgba(240, 238, 232, 0.35);
  }
  .tl {
    position: absolute;
    top: 4px;
    margin-left: 4px;
    font-size: 9.5px;
    color: var(--text-4);
  }
  .track {
    height: 58px;
    background: var(--glass-fill-1);
  }
  .clip {
    position: absolute;
    top: 4px;
    bottom: 4px;
    border-radius: 8px;
    overflow: hidden;
    cursor: grab;
  }
  .clip.snd {
    background: rgba(255, 255, 255, 0.08);
    box-shadow: inset 0 -3px 0 rgba(240, 238, 232, 0.6);
  }
  .clip.snd.sel {
    box-shadow: inset 0 0 0 2px var(--text-strong);
  }
  .cl {
    position: absolute;
    left: 6px;
    top: 4px;
    right: 4px;
    font-size: 10.5px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    text-shadow: 0 1px 6px rgba(0, 0, 0, 0.9);
  }
  .pin {
    position: absolute;
    right: 4px;
    bottom: 6px;
    font-size: 9px;
    color: var(--locator);
  }
  .fade {
    position: absolute;
    top: 0;
    bottom: 0;
    pointer-events: none;
  }
  .fade.in {
    left: 0;
    background: linear-gradient(90deg, rgba(8, 9, 12, 0.7), transparent);
  }
  .fade.out {
    right: 0;
    background: linear-gradient(270deg, rgba(8, 9, 12, 0.7), transparent);
  }
  .anchor {
    position: absolute;
    top: -2px;
    height: 0;
    border-top: 1px dashed var(--locator-soft);
    pointer-events: none;
  }
  .ph {
    position: absolute;
    top: 30px;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: var(--playhead);
    box-shadow: 0 0 10px var(--playhead-glow);
    pointer-events: none;
    z-index: 5;
  }
</style>
