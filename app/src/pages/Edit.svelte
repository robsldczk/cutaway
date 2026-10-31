<script lang="ts">
  // Edit (Round 2): Logic-style control bar, lane groups with mute/solo, the script strip,
  // anchors, room tone loop seams, the inspector (I), mixer (X), cycle (C) and the docked
  // scene bucket (B). Every change is a command; the core schedules and plays.
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import { clamp, fmt } from '../lib/format';
  import { charColor, charLabel, rgba } from '../lib/colors';
  import { GROUP_LABEL, laneLabel, laneRgb, mixOf, placementLane, setMix, type LaneGroup, type LaneRow } from '../lib/lanes';
  import { LANE_COLORS, colorOf, type Rgb } from '../theme';
  import Waveform from '../components/Waveform.svelte';
  import Icon from '../components/Icon.svelte';
  import EditBar from '../components/EditBar.svelte';
  import EditInspector from '../components/EditInspector.svelte';
  import SceneBucket from '../components/SceneBucket.svelte';
  import type { DragSound, ItemView, PlacementView } from '../lib/types';

  const HEAD = 168; // lane header (160) + gap
  const RULER = 20;
  const SCRIPT = 44;

  let zoom = $state(0);
  let scroller: HTMLDivElement | undefined = $state();
  let viewW = $state(1000);
  let collapsed = $state<Record<string, boolean>>({});
  let drag = $state<{ id: string; dx: number; dy: number } | null>(null);
  let fadeEdit = $state<{ id: string; fade_in_s: number; fade_out_s: number } | null>(null);
  let band = $state<{ x0: number; x1: number } | null>(null);
  let ghost = $state<{ x: number; y: number; w: number; name: string; progress: number; snap: string | null } | null>(null);

  const v = $derived(app.view);
  const ev = $derived(app.edit);
  const duration = $derived(Math.max(5, (v?.duration_s ?? 0) + 2, ...(ev?.placements.map((p) => p.start_s + p.len_s + 0.5) ?? [0])));
  const fitPx = $derived(Math.max(4, (viewW - HEAD - 24) / duration));
  const pxPerS = $derived(fitPx * Math.pow(400 / fitPx, zoom));
  const width = $derived(duration * pxPerS);
  const lines = $derived(app.lines.filter((l) => l.pieces.length > 0));
  const actions = $derived((v?.items ?? []).filter((i) => i.kind === 'spacer'));
  const roles = $derived((v?.characters ?? []).filter((c) => app.lines.some((l) => l.character === c.id)));
  const sfxLayers = $derived(Math.max(1, ...(ev?.placements.filter((p) => p.lane === 'sfx').map((p) => p.layer + 1) ?? [1])));
  const curLine = $derived(app.lineAt(app.pos));

  // rows: ruler and script on top, then the lane groups
  const rows = $derived.by(() => {
    const out: LaneRow[] = [];
    let y = RULER + SCRIPT;
    const group = (g: LaneGroup, lanes: Omit<LaneRow, 'y' | 'kind' | 'group'>[]) => {
      out.push({ kind: 'group', key: `g:${g}`, label: GROUP_LABEL[g], y, h: 24, group: g, rgb: [0, 0, 0] });
      y += 24;
      if (collapsed[g]) return;
      for (const l of lanes) {
        out.push({ ...l, kind: 'lane', group: g, y });
        y += l.h;
      }
    };
    group('dialogue', roles.map((c) => ({ key: c.id, label: c.label, h: 46, rgb: colorOf(c.color_index).rgb, char: c.id, lane: 'dialogue' as const })));
    group('sfx', Array.from({ length: sfxLayers }, (_, i) => ({ key: `sfx${i + 1}`, label: `SFX ${i + 1}`, h: 42, rgb: LANE_COLORS.sfx, layer: i, lane: 'sfx' as const })));
    group('ambience', [
      { key: 'roomtone', label: 'Room tone', h: 38, rgb: LANE_COLORS.room, lane: 'roomtone' as const },
      { key: 'ambience', label: 'Ambience', h: 38, rgb: LANE_COLORS.room, lane: 'ambience' as const },
    ]);
    group('music', [{ key: 'music', label: 'Music', h: 42, rgb: LANE_COLORS.music, lane: 'music' as const }]);
    return out;
  });
  const lanes = $derived(rows.filter((r) => r.kind === 'lane'));
  const totalH = $derived(rows.length ? rows[rows.length - 1].y + rows[rows.length - 1].h + 12 : 200);
  const rowOf = (key: string) => lanes.find((r) => r.key === key);

  $effect(() => {
    if (app.page === 'edit' && !app.edit) void app.loadEdit();
  });
  $effect(() => {
    void v?.rev;
    if (app.page === 'edit') void app.loadEdit();
  });
  $effect(() => {
    const el = scroller;
    if (!el) return;
    const ro = new ResizeObserver(() => (viewW = el.clientWidth));
    ro.observe(el);
    el.addEventListener('wheel', wheel, { passive: false });
    return () => {
      ro.disconnect();
      el.removeEventListener('wheel', wheel);
    };
  });

  function wheel(e: WheelEvent) {
    if (!(e.metaKey || e.ctrlKey)) return;
    e.preventDefault();
    zoom = clamp(zoom - e.deltaY * 0.002, 0, 1);
  }

  const x = (t: number) => t * pxPerS;
  const timeAt = (clientX: number) => {
    const r = scroller?.getBoundingClientRect();
    return clamp((clientX - (r?.left ?? 0) + (scroller?.scrollLeft ?? 0) - HEAD) / pxPerS, 0, duration);
  };

  // ---- snapping ----
  /** Word starts and action lines near a time (within 10 px), for drops and moves. */
  function snapAt(t: number): { t: number; line_id: string; word: number | null; label: string } | null {
    if (!app.snapWords || !v) return null;
    let best: { t: number; line_id: string; word: number | null; label: string; d: number } | null = null;
    const consider = (tt: number, line_id: string, word: number | null, label: string) => {
      const d = Math.abs(tt - t) * pxPerS;
      if (d <= 10 && (!best || d < best.d)) best = { t: tt, line_id, word, label, d };
    };
    for (const a of actions) consider(a.start_s, a.line_id, null, a.text);
    for (const l of lines) for (const w of l.words) if (!w.deleted && w.script_idx != null) consider(w.start_s, l.line_id, w.script_idx, `${charLabel(v, l.character)} · “${w.text}”`);
    return best;
  }

  // ---- selection and dragging ----
  function track(e: MouseEvent, id: string, done: (dt: number, dy: number, alt: boolean) => Promise<void>, onMove?: (dt: number, dy: number) => void) {
    e.preventDefault();
    e.stopPropagation();
    const x0 = e.clientX;
    const y0 = e.clientY;
    drag = { id, dx: 0, dy: 0 };
    const mv = (m: MouseEvent) => {
      if (!drag) return;
      drag = { ...drag, dx: m.clientX - x0, dy: m.clientY - y0 };
      onMove?.((m.clientX - x0) / pxPerS, m.clientY - y0);
    };
    const up = async (u: MouseEvent) => {
      window.removeEventListener('mousemove', mv);
      window.removeEventListener('mouseup', up);
      const dx = u.clientX - x0;
      const dy = u.clientY - y0;
      drag = null;
      if (Math.abs(dx) > 3 || Math.abs(dy) > 3) await done(dx / pxPerS, dy, u.altKey);
    };
    window.addEventListener('mousemove', mv);
    window.addEventListener('mouseup', up);
  }

  async function splitAt(kind: 'line' | 'sound', id: string, e: MouseEvent) {
    const t = timeAt(e.clientX);
    if (kind === 'sound') await app.cmd({ cmd: 'split_placement', placement_id: id, at_s: +t.toFixed(3) });
    else await app.cmd({ cmd: 'split_clip', line_id: id, at_s: +t.toFixed(3) });
    await app.loadEdit();
  }

  function lineDown(e: MouseEvent, l: ItemView) {
    e.stopPropagation();
    app.editSel = { kind: 'line', id: l.line_id };
    app.select(l.line_id);
    if (app.tool === 'split') return void splitAt('line', l.line_id, e);
    if (app.tool === 'gain') {
      const g0 = l.gain_db;
      return track(e, l.line_id, async (_dt, dy) => {
        await app.cmd({ cmd: 'set_clip_gain', line_id: l.line_id, gain_db: +clamp(g0 - dy * 0.1, -30, 12).toFixed(1) });
      });
    }
    if (app.tool !== 'pointer') return;
    track(e, l.line_id, async (dt, _dy, alt) => {
      await app.cmd({ cmd: 'move_clip', line_id: l.line_id, delta_s: +dt.toFixed(3), ripple: alt ? !app.ripple : app.ripple });
      await app.loadEdit();
    });
  }

  function soundDown(e: MouseEvent, p: PlacementView) {
    e.stopPropagation();
    app.editSel = { kind: 'sound', id: p.id };
    if (app.tool === 'split') return void splitAt('sound', p.id, e);
    if (app.tool === 'gain') {
      const g0 = p.gain_db;
      return track(e, p.id, async (_dt, dy) => patch(p, { gain_db: +clamp(g0 - dy * 0.1, -40, 12).toFixed(1) }));
    }
    if (app.tool === 'fade') {
      // drag in from an edge: the left half sets the fade in, the right half the fade out
      const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
      const left = e.clientX - r.left < r.width / 2;
      const set = (dt: number) => {
        const base = left ? (e.clientX - r.left) / pxPerS + dt : (r.right - e.clientX) / pxPerS - dt;
        const f = +clamp(base, 0, p.len_s / 2).toFixed(3);
        fadeEdit = { id: p.id, fade_in_s: left ? f : p.fade_in_s, fade_out_s: left ? p.fade_out_s : f };
      };
      set(0);
      return track(
        e,
        p.id,
        async () => {
          const f = fadeEdit;
          fadeEdit = null;
          if (f) await patch(p, { fade_in_s: f.fade_in_s, fade_out_s: f.fade_out_s });
        },
        (dt) => set(dt),
      );
    }
    if (app.tool !== 'pointer') return;
    track(e, p.id, async (dt) => moveSound(p, dt));
  }

  async function patch(p: PlacementView, patch: Record<string, unknown>) {
    await app.cmd({ cmd: 'update_placement', placement_id: p.id, patch });
    await app.loadEdit();
  }

  async function moveSound(p: PlacementView, dt: number) {
    let to = p.start_s + dt;
    const s = !p.anchored ? snapAt(to) : null;
    if (s) to = s.t;
    // anchored sounds keep their anchor and change the offset; pinned ones change their time
    const pt = p.anchored ? { offset_s: +(p.offset_s + dt).toFixed(3) } : { at_s: +to.toFixed(3), offset_s: 0 };
    await patch(p, pt);
  }

  function anchorTime(p: PlacementView): number | null {
    const a = p.anchor;
    if (!a) return null;
    const it = v?.items.find((i) => i.line_id === a.line_id);
    if (!it) return null;
    if (a.word != null) {
      const w = it.words.find((w) => w.script_idx === a.word);
      if (w) return a.edge === 'end' ? w.end_s : w.start_s;
    }
    return a.edge === 'end' ? it.end_s : it.start_s;
  }
  /** Where an anchor line starts: the dialogue lane of its character, or the script strip for actions. */
  function anchorY(p: PlacementView): number {
    const it = v?.items.find((i) => i.line_id === p.anchor?.line_id);
    const r = it?.character ? rowOf(it.character) : undefined;
    return r ? r.y + r.h / 2 : RULER + SCRIPT;
  }

  // ---- ruler: click seeks, drag sets the cycle range ----
  function rulerDown(e: MouseEvent) {
    const t0 = timeAt(e.clientX);
    let moved = false;
    const mv = (m: MouseEvent) => {
      const t1 = timeAt(m.clientX);
      if (Math.abs(t1 - t0) * pxPerS > 4) moved = true;
      if (moved) app.cycle = { on: true, from: Math.min(t0, t1), to: Math.max(t0, t1) };
    };
    const up = () => {
      window.removeEventListener('mousemove', mv);
      window.removeEventListener('mouseup', up);
      if (!moved) app.seek(t0);
    };
    window.addEventListener('mousemove', mv);
    window.addEventListener('mouseup', up);
  }

  // ---- empty lane: marquee sets a range, otherwise a click seeks and clears the selection ----
  function laneDown(e: MouseEvent) {
    if (e.button !== 0) return;
    const t0 = timeAt(e.clientX);
    if (app.tool !== 'marquee') {
      app.editSel = null;
      app.seek(t0);
      return;
    }
    band = { x0: t0, x1: t0 };
    const mv = (m: MouseEvent) => band && (band = { ...band, x1: timeAt(m.clientX) });
    const up = () => {
      window.removeEventListener('mousemove', mv);
      window.removeEventListener('mouseup', up);
      if (band && Math.abs(band.x1 - band.x0) * pxPerS > 4) app.cycle = { on: true, from: Math.min(band.x0, band.x1), to: Math.max(band.x0, band.x1) };
      band = null;
    };
    window.addEventListener('mousemove', mv);
    window.addEventListener('mouseup', up);
  }

  // ---- drops from the scene bucket ----
  function dragOver(e: DragEvent, r: LaneRow) {
    if (r.lane === 'dialogue' || r.lane === 'roomtone') return;
    if (!e.dataTransfer?.types.includes('application/x-cutaway-bucket')) return;
    e.preventDefault();
  }

  async function drop(e: DragEvent, r: LaneRow) {
    const raw = e.dataTransfer?.getData('application/x-cutaway-bucket');
    if (!raw || !v || r.lane === 'dialogue' || r.lane === 'roomtone' || !r.lane) return;
    e.preventDefault();
    const s = JSON.parse(raw) as DragSound;
    const t = timeAt(e.clientX);
    const snap = snapAt(t);
    const where = snap ? { anchor_line_id: snap.line_id, anchor_word: snap.word ?? undefined } : { at_s: +t.toFixed(3) };
    const lane = r.lane;
    if (s.asset_id) {
      await app.method('place_from_bucket', { scene: v.scene.id, asset_id: s.asset_id, lane, ...where });
      await app.loadEdit();
      return;
    }
    // a preview: downloads on drop, shown as a grey clip with a gold progress bar
    const gx = x(snap?.t ?? t);
    ghost = { x: gx, y: r.y + 4, w: Math.max(24, Math.min(x(s.duration_s || 1), width - gx)), name: s.name, progress: 0, snap: snap?.label ?? null };
    const job = await app.run(() => call<{ job_id: string }>('place_from_bucket', { scene: v!.scene.id, bucket_id: s.bucket_id, remote_id: s.remote_id, lane, ...where }));
    if (!job) {
      ghost = null;
      return;
    }
    const j = await app.waitJob(job.job_id, (jj) => ghost && (ghost = { ...ghost, progress: jj.progress }));
    ghost = null;
    if (j?.state === 'failed') app.say(`Download failed: ${j.message}`);
    await app.refresh();
    await app.loadEdit();
  }

  // ---- keys (the page's own: nudges, delete, I X C) ----
  const selSound = $derived(app.editSel?.kind === 'sound' ? ev?.placements.find((p) => p.id === app.editSel!.id) : undefined);
  const selLine = $derived(app.editSel?.kind === 'line' ? app.lines.find((l) => l.line_id === app.editSel!.id) : undefined);

  async function nudgeSel(d: number) {
    if (selSound) await moveSound(selSound, d);
    else if (selLine) {
      await app.cmd({ cmd: 'move_clip', line_id: selLine.line_id, delta_s: d, ripple: app.ripple });
      await app.loadEdit();
    }
  }

  function onKey(e: KeyboardEvent) {
    if (app.page !== 'edit' || app.overlay) return;
    const t = e.target as HTMLElement | null;
    if (t && (t.tagName === 'INPUT' || t.tagName === 'SELECT' || t.tagName === 'TEXTAREA')) return;
    const k = e.key;
    let handled = true;
    if ((k === 'ArrowLeft' || k === 'ArrowRight') && e.altKey) void nudgeSel((k === 'ArrowLeft' ? -1 : 1) * (e.shiftKey ? 0.1 : 0.01));
    else if ((k === 'ArrowLeft' || k === 'ArrowRight') && selLine) void app.stepTake(k === 'ArrowLeft' ? -1 : 1);
    else if ((k === 'Backspace' || k === 'Delete') && selSound) {
      void app.cmd({ cmd: 'delete_placement', placement_id: selSound.id }).then(() => app.loadEdit());
      app.editSel = null;
    } else if ((k === 'i' || k === 'I') && !e.metaKey && !e.ctrlKey) app.editInspector = !app.editInspector;
    else if (k === 'x' || k === 'X') app.mixer = !app.mixer;
    else if (k === 'c' || k === 'C') {
      if (app.cycle.to <= app.cycle.from) {
        const l = curLine;
        app.cycle = { on: true, from: l?.start_s ?? app.pos, to: l?.end_s ?? app.pos + 4 };
      } else app.cycle = { ...app.cycle, on: !app.cycle.on };
    } else if (k === 'Escape' && app.tool !== 'pointer') app.tool = 'pointer';
    else handled = false;
    if (handled) {
      e.preventDefault();
      e.stopPropagation();
    }
  }
  $effect(() => {
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  });

  // ---- display helpers ----
  const lcd = $derived.by(() => {
    const sc = `Sc. ${v?.scene.number ?? ''}`;
    if (selSound) return `${sc} · ${laneLabel(placementLane(selSound))} · ${selSound.name.replace(/\.[a-z0-9]+$/i, '')}`;
    if (selLine) return `${sc} · ${charLabel(v, selLine.character)} · T${(selLine.take_index ?? 0) + 1}`;
    if (app.editSel?.kind === 'roomtone') return `${sc} · Room tone`;
    return `${sc} · ${v?.scene.place ?? ''}`;
  });
  const clipBg = (c: Rgb, strong: boolean) => `background: linear-gradient(180deg, ${rgba(c, strong ? 0.5 : 0.32)}, ${rgba(c, 0.14)})`;
  const ticks = $derived.by(() => {
    const step = [1, 2, 5, 10, 15, 30, 60].find((s) => s * pxPerS >= 70) ?? 60;
    return Array.from({ length: Math.floor(duration / step) + 1 }, (_, i) => i * step);
  });
  const left = $derived(app.editInspector ? 284 : 20);
  const right = $derived(app.editBucket ? 404 : 60);
  const lineText = (l: ItemView) => l.words.filter((w) => !w.deleted).map((w) => w.text).join(' ') || l.text;
  const flat = Array.from({ length: 60 }, (_, i) => 0.18 + 0.1 * Math.abs(Math.sin(i * 1.7)));
</script>

<EditBar bind:zoom lcdDetail={lcd} />

{#if app.editInspector}<EditInspector />{/if}

<section class="tl" aria-label="Timeline" style="left: {left}px; right: {right}px">
  <div class="scroll" bind:this={scroller}>
    <div class="content" style="width: {HEAD + width + 24}px; height: {totalH}px">
      <!-- ruler + script strip (sticky on top) -->
      <div class="top" style="width: {HEAD + width + 24}px">
        <div class="hcol" style="height: {RULER + SCRIPT}px"><span class="label stl">Script</span></div>
        <div class="area" style="left: {HEAD}px; width: {width}px">
          <div class="ruler" role="slider" tabindex="-1" aria-label="Ruler" aria-valuenow={app.pos} onmousedown={rulerDown}>
            {#if app.cycle.to > app.cycle.from}<span class="cyc" class:off={!app.cycle.on} style="left: {x(app.cycle.from)}px; width: {x(app.cycle.to - app.cycle.from)}px"></span>{/if}
            {#each ticks as t (t)}<span class="tick muted tn" style="left: {x(t)}px">{fmt(t).replace(/\.\d$/, '').replace(/^0/, '')}</span>{/each}
          </div>
          {#each lines as l (l.line_id)}
            {@const c = charColor(v, l.character).rgb}
            {@const cur = curLine?.line_id === l.line_id}
            <span class="si script" class:cur style="left: {x(l.start_s)}px; width: {Math.max(8, x(l.end_s - l.start_s))}px; top: 23px; {cur ? `background: ${rgba(c, 0.6)}` : `background: ${rgba(c, 0.22)}`}" title={l.text}>{l.text}</span>
          {/each}
          {#each actions as a (a.line_id)}
            <span class="si script act" style="left: {x(a.start_s)}px; width: {Math.max(40, x(Math.max(0.6, a.end_s - a.start_s)))}px; top: 44px" title={a.text}>{a.text}</span>
          {/each}
          <div class="ph" style="left: {x(app.pos)}px"><span></span></div>
        </div>
      </div>

      <!-- lane groups -->
      {#each rows as r (r.key)}
        <div class="row" style="top: {r.y}px; height: {r.h}px; width: {HEAD + width + 24}px">
          {#if r.kind === 'group'}
            <button class="gh" onclick={() => (collapsed = { ...collapsed, [r.group]: !collapsed[r.group] })} aria-expanded={!collapsed[r.group]}>
              <span style="display: inline-flex; transform: rotate({collapsed[r.group] ? -90 : 0}deg)"><Icon name="chevron" size={11} stroke={2.4} /></span>{r.label}
            </button>
          {:else}
            {@const m = mixOf(r.key)}
            {@const isSel = (selSound && placementLane(selSound) === r.key) || (selLine && selLine.character === r.key) || (app.editSel?.kind === 'roomtone' && r.key === 'roomtone')}
            <div class="lh" class:hl={isSel}>
              <span class="dot" style="background: {rgba(r.rgb, 1)}"></span>
              <span class="ellip ln">{r.label}</span>
              <button class="ms" class:mon={m.mute} aria-label="Mute {r.label}" aria-pressed={m.mute} onclick={() => setMix(r.key, { mute: !m.mute })}>M</button>
              <button class="ms" class:son={m.solo} aria-label="Solo {r.label}" aria-pressed={m.solo} onclick={() => setMix(r.key, { solo: !m.solo })}>S</button>
            </div>
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions (pointer editing; the keyboard path is the selection keys) -->
            <div class="lane" class:muted-lane={m.mute} style="left: {HEAD}px; width: {width}px" role="application" aria-label="{r.label} lane" data-testid="edit-lane" data-lane={r.key} onmousedown={laneDown} ondragover={(e) => dragOver(e, r)} ondrop={(e) => drop(e, r)}>
              {#if r.lane === 'dialogue'}
                {#each lines.filter((l) => l.character === r.char) as l (l.line_id)}
                  {@const c = r.rgb}
                  {@const sel = app.editSel?.id === l.line_id}
                  {@const t = l.takes[l.take_index ?? 0]}
                  {@const dx = drag?.id === l.line_id && app.tool === 'pointer' ? drag.dx : 0}
                  <div class="clip" class:sel role="button" tabindex="-1" onmousedown={(e) => lineDown(e, l)} title="{l.text} (T{(l.take_index ?? 0) + 1})" style="left: {x(l.start_s) + dx}px; width: {Math.max(4, x(l.end_s - l.start_s))}px; {clipBg(c, sel)}; box-shadow: inset 0 0 0 {sel ? '1.5px var(--playhead)' : `1px ${rgba(c, 0.45)}`}">
                    {#if t}<Waveform peaks={t.peaks} fill={rgba(c, 0.75)} />{/if}
                    <span class="cl ellip">{lineText(l)}</span>
                    {#if l.pinned}<span class="pin">pinned</span>{/if}
                  </div>
                {/each}
              {:else if r.lane === 'roomtone'}
                {#if ev?.room_tone}
                  {@const sel = app.editSel?.kind === 'roomtone'}
                  <div class="clip" class:sel role="button" tabindex="-1" onmousedown={(e) => { e.stopPropagation(); app.editSel = { kind: 'roomtone', id: 'roomtone' }; }} style="left: 0; width: {x(v?.duration_s ?? duration)}px; {clipBg(r.rgb, sel)}; box-shadow: inset 0 0 0 {sel ? '1.5px var(--playhead)' : `1px ${rgba(r.rgb, 0.45)}`}">
                    <Waveform peaks={flat} fill={rgba(r.rgb, 0.75)} />
                    <span class="cl ellip">{v?.scene.place} room tone · loop</span>
                  </div>
                  {#each ev.room_tone.seams.filter((s) => s < (v?.duration_s ?? 0)) as s (s)}<span class="seam" style="left: {x(s)}px"></span>{/each}
                {/if}
              {:else}
                {#each (ev?.placements ?? []).filter((p) => placementLane(p) === r.key) as p (p.id)}
                  {@const sel = app.editSel?.id === p.id}
                  {@const dx = drag?.id === p.id && app.tool === 'pointer' ? drag.dx : 0}
                  {@const fe = fadeEdit?.id === p.id ? fadeEdit : p}
                  <div class="clip" class:sel role="button" tabindex="-1" onmousedown={(e) => soundDown(e, p)} title={p.name} style="left: {x(p.start_s) + dx}px; width: {Math.max(4, x(p.len_s))}px; {clipBg(r.rgb, sel)}; box-shadow: inset 0 0 0 {sel ? '1.5px var(--playhead)' : `1px ${rgba(r.rgb, 0.45)}`}" data-testid="sound-clip">
                    <Waveform peaks={p.peaks} fill={rgba(r.rgb, 0.75)} />
                    <span class="cl ellip">{p.name.replace(/\.[a-z0-9]+$/i, '')}</span>
                    {#if sel}
                      {@const fi = Math.min(50, (fe.fade_in_s / p.len_s) * 100)}
                      {@const fo = Math.max(50, 100 - (fe.fade_out_s / p.len_s) * 100)}
                      <svg class="fx" viewBox="0 0 100 40" preserveAspectRatio="none" aria-hidden="true"><polyline points="0,40 {fi},4" /><polyline points="{fo},4 100,40" /></svg>
                      <span class="fh" style="left: {fi}%"></span><span class="fh" style="left: {fo}%"></span>
                      <span class="gl" style="top: {clamp(50 - p.gain_db * 3, 6, 94)}%"></span>
                    {/if}
                  </div>
                {/each}
              {/if}
            </div>
          {/if}
        </div>
      {/each}

      <!-- overlay: anchors, ghost of a downloading drop, marquee, playhead -->
      <div class="ov" style="left: {HEAD}px; width: {width}px; height: {totalH}px">
        {#each ev?.placements ?? [] as p (p.id)}
          {@const at = anchorTime(p)}
          {@const r = rowOf(placementLane(p))}
          {#if at != null && r}
            {@const y0 = anchorY(p)}
            {@const y1 = r.y + r.h / 2}
            <span class="anc" style="left: {x(at)}px; top: {Math.min(y0, y1)}px; height: {Math.abs(y1 - y0)}px"></span>
            <span class="ad" style="left: {x(at)}px; top: {y0}px"></span>
          {/if}
        {/each}
        {#if ghost}
          <div class="ghost" style="left: {ghost.x}px; top: {ghost.y}px; width: {ghost.w}px"><span class="cl ellip">{ghost.name}</span><span class="gp" style="width: {Math.round(ghost.progress * 100)}%"></span></div>
          <div class="gtip tn" style="left: {ghost.x}px; top: {Math.max(0, ghost.y - 32)}px">Downloading {Math.round(ghost.progress * 100)}%{ghost.snap ? ` · snaps to “${ghost.snap}”` : ''}</div>
        {/if}
        {#if band}<span class="band" style="left: {x(Math.min(band.x0, band.x1))}px; width: {x(Math.abs(band.x1 - band.x0))}px"></span>{/if}
        <div class="ph" style="left: {x(app.pos)}px"><span></span></div>
      </div>
    </div>
  </div>
</section>

{#if app.mixer}
  <section class="glass mixer" aria-label="Mixer" style="left: {left}px; right: {right}px">
    {#each lanes as r (r.key)}
      {@const m = mixOf(r.key)}
      <div class="strip">
        <span class="ellip sn"><span class="dot" style="background: {rgba(laneRgb(r.key), 1)}"></span>{r.label}</span>
        <input class="fader" type="range" min="-60" max="12" step="0.5" value={m.volume_db} aria-label="{r.label} volume" onchange={(e) => setMix(r.key, { volume_db: +(e.currentTarget as HTMLInputElement).value })} />
        <span class="tn small">{m.volume_db.toFixed(1)} dB</span>
        <input class="panr" type="range" min="-1" max="1" step="0.05" value={m.pan} aria-label="{r.label} pan" onchange={(e) => setMix(r.key, { pan: +(e.currentTarget as HTMLInputElement).value })} />
        <div class="msr">
          <button class="ms" class:mon={m.mute} aria-label="Mute {r.label}" onclick={() => setMix(r.key, { mute: !m.mute })}>M</button>
          <button class="ms" class:son={m.solo} aria-label="Solo {r.label}" onclick={() => setMix(r.key, { solo: !m.solo })}>S</button>
        </div>
      </div>
    {/each}
  </section>
{/if}

{#if app.editBucket}
  <SceneBucket />
{:else}
  <button class="glass btab" aria-label="Show bucket (B)" onclick={() => (app.editBucket = true)}><Icon name="chevron-left" size={14} /><span>Bucket</span></button>
{/if}

<style>
  .tl {
    position: absolute;
    top: 138px;
    bottom: 16px;
  }
  .scroll {
    position: absolute;
    inset: 0;
    overflow: auto;
  }
  .content {
    position: relative;
  }
  .top {
    position: sticky;
    top: 0;
    height: 64px;
    z-index: 4;
    background: linear-gradient(180deg, var(--stage-bg) 70%, transparent);
  }
  .hcol {
    position: sticky;
    left: 0;
    width: 160px;
    display: flex;
    align-items: flex-end;
    padding: 0 0 14px 10px;
    box-sizing: border-box;
    z-index: 2;
  }
  .stl {
    font-size: 10px;
  }
  .area {
    position: absolute;
    top: 0;
    height: 64px;
  }
  .ruler {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    height: 20px;
    cursor: pointer;
  }
  .cyc {
    position: absolute;
    top: 2px;
    height: 14px;
    border-radius: 4px;
    background: rgb(var(--ink-rgb) / 0.22);
  }
  .cyc.off {
    background: rgb(var(--ink-rgb) / 0.08);
  }
  .tick {
    position: absolute;
    top: 3px;
    font-size: 11px;
    padding-left: 4px;
    border-left: 1px solid rgb(var(--veil-rgb) / 0.15);
    height: 14px;
    line-height: 14px;
    pointer-events: none;
  }
  .si {
    position: absolute;
    height: 18px;
    line-height: 12px;
    padding: 3px 7px;
    box-sizing: border-box;
    border-radius: 7px;
    font-size: 11px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    color: rgb(var(--ink-rgb) / 0.85);
  }
  .si.cur {
    color: var(--text-strong);
    font-weight: 700;
  }
  .si.act {
    color: rgb(var(--ink-rgb) / 0.55);
    font-style: italic;
    background: var(--glass-fill-2);
  }
  .row {
    position: absolute;
    left: 0;
  }
  .gh {
    position: sticky;
    left: 0;
    height: 100%;
    display: flex;
    align-items: flex-end;
    gap: 8px;
    padding: 0 0 4px 6px;
    box-sizing: border-box;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--text-4);
  }
  .lh {
    position: sticky;
    left: 0;
    width: 160px;
    height: calc(100% - 4px);
    margin-top: 2px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 6px 0 10px;
    box-sizing: border-box;
    border-radius: 10px;
    background: var(--glass-fill-1);
    z-index: 3;
    backdrop-filter: var(--glass-blur-soft);
  }
  .lh.hl {
    background: var(--glass-hover);
  }
  .ln {
    flex: 1;
    min-width: 0;
    font-size: 12.5px;
    font-weight: 600;
  }
  .ms {
    width: 20px;
    height: 20px;
    flex: none;
    border-radius: 6px;
    font-size: 10px;
    font-weight: 700;
    color: var(--text-4);
    background: var(--glass-fill-2);
  }
  .ms.mon {
    background: var(--warn);
    color: var(--text-on-pill);
  }
  .ms.son {
    background: var(--locator);
    color: var(--text-on-pill);
  }
  .lane {
    position: absolute;
    top: 2px;
    bottom: 2px;
    border-radius: 10px;
    background: rgb(var(--veil-rgb) / 0.018);
  }
  .muted-lane {
    opacity: 0.45;
  }
  .clip {
    position: absolute;
    top: 2px;
    bottom: 2px;
    border-radius: 8px;
    overflow: hidden;
    box-sizing: border-box;
    cursor: default;
  }
  .cl {
    position: absolute;
    left: 6px;
    right: 6px;
    top: 3px;
    font-size: 10.5px;
    font-weight: 600;
    color: rgb(var(--ink-rgb) / 0.92);
    pointer-events: none;
  }
  .pin {
    position: absolute;
    right: 4px;
    bottom: 2px;
    font-size: 9px;
    color: var(--text-4);
  }
  .fx {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
  }
  .fx polyline {
    fill: none;
    stroke: var(--playhead);
    stroke-width: 1.4;
    vector-effect: non-scaling-stroke;
  }
  .fh {
    position: absolute;
    top: 2px;
    width: 8px;
    height: 8px;
    margin-left: -4px;
    border-radius: 2px;
    background: var(--playhead);
    pointer-events: none;
  }
  .gl {
    position: absolute;
    left: 0;
    right: 0;
    height: 1px;
    background: var(--playhead);
    pointer-events: none;
  }
  .seam {
    position: absolute;
    top: 6px;
    bottom: 6px;
    width: 1px;
    background: rgb(var(--ink-rgb) / 0.45);
    pointer-events: none;
  }
  .ov {
    position: absolute;
    top: 0;
    pointer-events: none;
    z-index: 2;
  }
  .anc {
    position: absolute;
    width: 0;
    border-left: 1px dashed rgb(var(--sfx-rgb) / 0.7);
  }
  .ad {
    position: absolute;
    width: 7px;
    height: 7px;
    margin: -3px 0 0 -3px;
    border-radius: 50%;
    background: var(--sfx);
  }
  .ghost {
    position: absolute;
    height: 30px;
    border-radius: 8px;
    box-sizing: border-box;
    background: rgb(var(--ink-rgb) / 0.08);
    box-shadow: inset 0 0 0 1.5px rgb(var(--ink-rgb) / 0.45);
    overflow: hidden;
  }
  .gp {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 3px;
    background: var(--sfx);
    transition: width 0.2s;
  }
  .gtip {
    position: absolute;
    padding: 5px 9px;
    border-radius: 9px;
    background: var(--button-light);
    color: var(--text-on-light);
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
  }
  .band {
    position: absolute;
    top: 64px;
    bottom: 0;
    background: rgb(var(--ink-rgb) / 0.08);
    box-shadow: inset 0 0 0 1px rgb(var(--ink-rgb) / 0.3);
  }
  .ph {
    position: absolute;
    top: 0;
    bottom: 0;
    pointer-events: none;
    width: 2px;
    margin-left: -1px;
    background: var(--playhead);
    box-shadow: 0 0 10px var(--playhead-glow);
  }
  .ph span {
    position: absolute;
    top: 0;
    left: -5px;
    width: 12px;
    height: 12px;
    border-radius: 3px;
    background: var(--playhead);
  }
  .btab {
    position: absolute;
    right: 16px;
    top: 50%;
    transform: translateY(-50%);
    width: 32px;
    height: 132px;
    border-radius: 16px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    background: var(--bar);
    z-index: 8;
  }
  .btab span {
    writing-mode: vertical-rl;
    transform: rotate(180deg);
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.06em;
  }
  .mixer {
    position: absolute;
    bottom: 16px;
    height: 230px;
    border-radius: 22px;
    background: var(--bar-strong);
    z-index: 7;
    display: flex;
    gap: 6px;
    padding: 12px;
    box-sizing: border-box;
    overflow-x: auto;
  }
  .strip {
    flex: none;
    width: 84px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 8px 4px;
    border-radius: 14px;
    background: var(--glass-fill-1);
  }
  .sn {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    font-weight: 600;
    max-width: 100%;
  }
  .fader {
    writing-mode: vertical-lr;
    direction: rtl;
    height: 100px;
    width: 18px;
    accent-color: var(--button-light);
  }
  .panr {
    width: 66px;
    accent-color: var(--button-light);
  }
  .small {
    font-size: 10.5px;
    color: var(--text-4);
  }
  .msr {
    display: flex;
    gap: 4px;
  }
  @media (max-width: 1100px) {
    .tl,
    .mixer {
      left: 20px !important;
      right: 20px !important;
    }
    .btab {
      display: none;
    }
  }
</style>
