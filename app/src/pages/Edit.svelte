<script lang="ts">
  // Edit: a multitrack timeline at a fixed time scale, with Logic's control bar and key
  // commands. Lane groups with mute/solo and previews when collapsed, the script strip with
  // its words, anchors, room tone loop seams, the inspector (I), mixer (X), cycle (C) and the
  // docked scene bucket (B). Every change is a command; the core schedules and plays.
  import { tick, untrack } from 'svelte';
  import { app, type EditSel } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import { dropzone, type DropZone } from '../lib/drag.svelte';
  import { clamp, fmt } from '../lib/format';
  import { charColor, charLabel, rgba } from '../lib/colors';
  import { GROUP_LABEL, laneLabel, laneRgb, mixOf, placementLane, setMix, type LaneGroup, type LaneRow } from '../lib/lanes';
  import { TOOLS } from '../lib/tools';
  import { LANE_COLORS, colorOf, type Rgb } from '../theme';
  import Waveform from '../components/Waveform.svelte';
  import Icon from '../components/Icon.svelte';
  import EditBar from '../components/EditBar.svelte';
  import EditInspector from '../components/EditInspector.svelte';
  import SceneBucket from '../components/SceneBucket.svelte';
  import type { DragSound, ItemView, LayoutPart, PieceView, PlacementView } from '../lib/types';
  import { lineText, lineWords, type ShownWord } from '../lib/words';

  const HEAD = 168; // lane header (160) + gap
  const CYC = 10; // cycle strip at the top of the ruler
  const TOP = 92; // ruler, sentences, action lines, words
  const MIN_PX = 2;
  const MAX_PX = 1000;
  const HANDLE_S = 3; // hidden audio shown around an edge while trimming

  let scroller: HTMLDivElement | undefined = $state();
  let viewW = $state(1000);
  let measured = $state(false);
  let scrollX = $state(0);
  let collapsed = $state<Record<string, boolean>>({});
  /** Clips being moved together and how far (px). */
  let drag = $state<{ keys: string[]; dx: number } | null>(null);
  /** An edge being trimmed: the clip, which edge, how far (s), and the hidden audio around it. */
  let trim = $state<{ key: string; edge: 'start' | 'end'; dt: number; ghost: { t: number; len: number; peaks: number[] } | null } | null>(null);
  let fadeEdit = $state<{ key: string; fade_in_s: number; fade_out_s: number } | null>(null);
  let band = $state<{ x0: number; x1: number } | null>(null);
  let rubber = $state<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
  let ghost = $state<{ x: number; y: number; w: number; name: string; progress: number; snap: string | null } | null>(null);
  let toolMenu = $state(false);
  let zoomBack: { px: number; left: number } | null = null;
  let scrubbing = false;

  const v = $derived(app.view);
  const ev = $derived(app.edit);
  // The time scale is absolute: moving clips or opening panels never changes it.
  const sceneEnd = $derived(Math.max(v?.duration_s ?? 0, ...(ev?.placements.map((p) => p.start_s + p.len_s) ?? [0])));
  const fitPx = $derived(clamp((viewW - HEAD - 40) / Math.max(5, sceneEnd + 1), MIN_PX, MAX_PX));
  const px = $derived(app.editPx > 0 ? app.editPx : fitPx);
  const width = $derived(Math.max((sceneEnd + 4) * px, viewW - HEAD - 24));
  const duration = $derived(width / px);
  const vis0 = $derived(scrollX / px - 1);
  const vis1 = $derived((scrollX + viewW) / px + 1);

  const lines = $derived(app.lines.filter((l) => l.pieces.length > 0 || l.parts.some((p) => p.dropped)));
  // what the word lanes show: the script's words, timed by what was heard
  const shown = $derived(new Map(lines.map((l) => [l.line_id, lineWords(l).filter((w) => !w.deleted)])));
  const shownOf = (l: ItemView): ShownWord[] => shown.get(l.line_id) ?? [];
  const actions = $derived((v?.items ?? []).filter((i) => i.kind === 'spacer'));
  const roles = $derived((v?.characters ?? []).filter((c) => app.lines.some((l) => l.character === c.id)));
  const sfxLayers = $derived(Math.max(1, ...(ev?.placements.filter((p) => p.lane === 'sfx').map((p) => p.layer + 1) ?? [1])));
  const curLine = $derived(app.lineAt(app.pos));

  // rows: ruler, sentences, actions and words on top, then the lane groups
  const rows = $derived.by(() => {
    const out: LaneRow[] = [];
    let y = TOP;
    const ls = app.laneScale;
    const group = (g: LaneGroup, lanes: Omit<LaneRow, 'y' | 'kind' | 'group'>[]) => {
      const c = !!collapsed[g];
      out.push({ kind: 'group', key: `g:${g}`, label: GROUP_LABEL[g], y, h: c ? 30 : 24, group: g, rgb: [0, 0, 0] });
      y += c ? 30 : 24;
      if (c) return;
      for (const l of lanes) {
        const h = Math.round(l.h * ls);
        out.push({ ...l, h, kind: 'lane', group: g, y });
        y += h;
      }
    };
    group('dialogue', roles.map((c) => ({ key: c.id, label: c.label, h: 52, rgb: colorOf(c.color_index).rgb, char: c.id, lane: 'dialogue' as const })));
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

  // ---- clips: one list for selection, ←/→, rubber band and ⇧F ----
  interface ClipRef {
    sel: EditSel;
    key: string;
    lane: string;
    start: number;
    end: number;
  }
  const keyOf = (s: EditSel) => (s.kind === 'line' ? `line:${s.id}:${s.part ?? 0}` : `${s.kind}:${s.id}`);
  const kept = (l: ItemView) => l.parts.filter((p) => !p.dropped && p.end_s > p.start_s);
  const clips = $derived.by(() => {
    const out: ClipRef[] = [];
    for (const l of lines) for (const p of kept(l)) out.push({ sel: { kind: 'line', id: l.line_id, part: p.index }, key: `line:${l.line_id}:${p.index}`, lane: l.character ?? '', start: p.start_s, end: p.end_s });
    for (const p of ev?.placements ?? []) out.push({ sel: { kind: 'sound', id: p.id }, key: `sound:${p.id}`, lane: placementLane(p), start: p.start_s, end: p.start_s + p.len_s });
    return out.sort((a, b) => a.start - b.start || a.lane.localeCompare(b.lane));
  });
  const selKeys = $derived(new Set(app.editMany.map(keyOf)));
  const selection = $derived(app.editMany.length ? app.editMany : app.editSel ? [app.editSel] : []);
  const lineOf = (id: string) => app.lines.find((l) => l.line_id === id);
  const soundOf = (id: string) => ev?.placements.find((p) => p.id === id);
  const partOf = (l: ItemView, i: number) => l.parts.find((p) => p.index === i);
  const r3 = (t: number) => +t.toFixed(3);

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
    const ro = new ResizeObserver(() => {
      viewW = el.clientWidth;
      measured = true;
    });
    ro.observe(el);
    el.addEventListener('wheel', wheel, { passive: false });
    return () => {
      ro.disconnect();
      el.removeEventListener('wheel', wheel);
    };
  });
  // the first visit fits the scene; from then on the scale only changes when asked
  $effect(() => {
    if (measured && v && !untrack(() => app.editPx)) app.editPx = fitPx;
  });
  // page along with the playhead while playing
  $effect(() => {
    const t = app.pos;
    const el = scroller;
    if (!app.playing || !app.follow || !el || scrubbing) return;
    const xx = t * px;
    const vw = viewW - HEAD;
    if (xx < el.scrollLeft || xx > el.scrollLeft + vw - 40) el.scrollLeft = Math.max(0, xx - vw * 0.1);
  });

  // ---- zoom ----
  /** New scale, keeping time `at` under the same screen x (`sx`, relative to the timeline). */
  async function zoomTo(np: number, at: number, sx?: number) {
    const el = scroller;
    np = clamp(np, MIN_PX, MAX_PX);
    if (!el) return;
    const keep = sx ?? at * px + HEAD - el.scrollLeft;
    app.editPx = np;
    await tick();
    el.scrollLeft = Math.max(0, at * np + HEAD - keep);
  }

  function wheel(e: WheelEvent) {
    if (!(e.metaKey || e.ctrlKey || e.altKey)) return;
    e.preventDefault();
    const r = scroller!.getBoundingClientRect();
    zoomBack = null;
    void zoomTo(px * Math.exp(-e.deltaY * 0.0025), timeAt(e.clientX), e.clientX - r.left);
  }

  /** Keys zoom around the playhead when it is on screen, else around the middle. */
  function zoomBy(f: number) {
    const el = scroller;
    if (!el) return;
    const on = app.pos * px >= el.scrollLeft && app.pos * px <= el.scrollLeft + viewW - HEAD;
    zoomBack = null;
    void zoomTo(px * f, on ? app.pos : (el.scrollLeft + (viewW - HEAD) / 2) / px);
  }

  function zoomSlider(np: number) {
    zoomBack = null;
    zoomBy(np / px);
  }

  /** Z: zoom to the selection (or the whole scene) and back. */
  async function zoomToggle() {
    const el = scroller;
    if (!el) return;
    if (zoomBack) {
      const zb = zoomBack;
      zoomBack = null;
      app.editPx = zb.px;
      await tick();
      el.scrollLeft = zb.left;
      return;
    }
    zoomBack = { px, left: el.scrollLeft };
    const [a, b] = selRange() ?? [0, sceneEnd + 0.5];
    const np = clamp((viewW - HEAD - 60) / Math.max(0.2, b - a), MIN_PX, MAX_PX);
    app.editPx = np;
    await tick();
    el.scrollLeft = Math.max(0, a * np - 20);
  }

  /** Scrolls a time range into view (←/→, , and .). */
  function reveal(a: number, b: number) {
    const el = scroller;
    if (!el) return;
    const vw = viewW - HEAD - 24;
    if (a * px < el.scrollLeft) el.scrollLeft = Math.max(0, a * px - vw * 0.2);
    else if (b * px > el.scrollLeft + vw) el.scrollLeft = Math.max(0, Math.min(a * px - 40, b * px - vw * 0.8));
  }

  const x = (t: number) => t * px;
  const timeAt = (clientX: number) => {
    const r = scroller?.getBoundingClientRect();
    return clamp((clientX - (r?.left ?? 0) + (scroller?.scrollLeft ?? 0) - HEAD) / px, 0, duration);
  };
  /** Content coordinates of a pointer (for the rubber band). */
  const contentAt = (m: MouseEvent) => {
    const r = scroller!.getBoundingClientRect();
    return { x: m.clientX - r.left + scroller!.scrollLeft - HEAD, y: m.clientY - r.top + scroller!.scrollTop };
  };

  // ---- waveforms: each piece draws its own source range, finer as you zoom in ----
  let peakCache = $state<Record<string, number[]>>({});
  let peakLast = $state<Record<string, number[]>>({});
  let fetchTimer: ReturnType<typeof setTimeout> | null = null;
  const rangeKey = (src: string, a: number, z: number) => `${src}|${a.toFixed(3)}|${z.toFixed(3)}`;
  /** Points for a stretch at this zoom (powers of two, so small zoom steps reuse them). */
  const bucketsFor = (len: number) => {
    const want = Math.min(Math.ceil(len * 100), Math.ceil((len * px) / 2));
    return Math.max(8, 2 ** Math.ceil(Math.log2(Math.max(1, want))));
  };
  function peaksFor(src: string, a: number, z: number, fallback: () => number[]): number[] {
    const rk = rangeKey(src, a, z);
    return peakCache[`${rk}|${bucketsFor(z - a)}`] ?? peakLast[rk] ?? fallback();
  }
  /** Until the fine peaks arrive: the take's own envelope, cut to the piece. */
  function takeSlice(l: ItemView, pc: PieceView): number[] {
    const t = l.takes.find((t) => t.id === pc.take_id);
    if (!t || !t.peaks.length) return [];
    const n = t.peaks.length;
    const d = Math.max(0.001, t.src_out - t.src_in);
    const a = clamp(Math.floor(((pc.src_in - t.src_in) / d) * n), 0, n);
    const z = clamp(Math.ceil(((pc.src_out - t.src_in) / d) * n), a, n);
    return t.peaks.slice(a, z);
  }
  const piecePeaks = (l: ItemView, pc: PieceView) => peaksFor(pc.source_id, pc.src_in, pc.src_out, () => takeSlice(l, pc));
  const soundPeaks = (p: PlacementView) => peaksFor(`asset:${p.asset_id}`, p.in_s, p.in_s + p.len_s, () => p.peaks);

  $effect(() => {
    const want: { key: string; rk: string; source_id: string; t0: number; t1: number; buckets: number }[] = [];
    const need = (src: string, a: number, z: number) => {
      const rk = rangeKey(src, a, z);
      const n = bucketsFor(z - a);
      want.push({ key: `${rk}|${n}`, rk, source_id: src, t0: a, t1: z, buckets: n });
    };
    for (const l of lines) for (const pc of l.pieces) if (pc.start_s + pc.len_s >= vis0 && pc.start_s <= vis1) need(pc.source_id, pc.src_in, pc.src_out);
    for (const p of ev?.placements ?? []) if (p.start_s + p.len_s >= vis0 && p.start_s <= vis1) need(`asset:${p.asset_id}`, p.in_s, p.in_s + p.len_s);
    const have = peakCache;
    const missing = want.filter((w) => !(w.key in have));
    if (!missing.length) return;
    if (fetchTimer) clearTimeout(fetchTimer);
    fetchTimer = setTimeout(async () => {
      const r = await call<number[][]>('peaks_batch', { ranges: missing.map((m) => ({ source_id: m.source_id, t0: m.t0, t1: m.t1, buckets: m.buckets, normalize: true })) }).catch(() => null);
      if (!r) return;
      const big = Object.keys(peakCache).length > 3000;
      const next = big ? {} : { ...peakCache };
      const last = big ? {} : { ...peakLast };
      missing.forEach((m, i) => {
        next[m.key] = r[i] ?? [];
        if (r[i]?.length) last[m.rk] = r[i];
      });
      peakCache = next;
      peakLast = last;
    }, 60);
  });

  // ---- snapping ----
  /** Word starts and action lines near a time (within 10 px), for drops and moves. */
  function snapAt(t: number): { t: number; line_id: string; word: number | null; label: string } | null {
    if (!app.snapWords || !v) return null;
    let best: { t: number; line_id: string; word: number | null; label: string; d: number } | null = null;
    const consider = (tt: number, line_id: string, word: number | null, label: string) => {
      const d = Math.abs(tt - t) * px;
      if (d <= 10 && (!best || d < best.d)) best = { t: tt, line_id, word, label, d };
    };
    for (const a of actions) consider(a.start_s, a.line_id, null, a.text);
    for (const l of lines) for (const w of l.words) if (!w.deleted && w.script_idx != null) consider(w.start_s, l.line_id, w.script_idx, `${charLabel(v, l.character)} · “${l.tokens[w.script_idx] ?? w.text}”`);
    return best;
  }

  /** The nearest candidate within 8 px, else `t`. */
  function snapTo(t: number, cands: number[]): number {
    if (!app.snapWords) return t;
    let best = t;
    let bd = 8;
    for (const c of cands) {
      const d = Math.abs(c - t) * px;
      if (d < bd) {
        bd = d;
        best = c;
      }
    }
    return best;
  }

  // ---- selection ----
  function selectOnly(list: EditSel[]) {
    app.editMany = list;
    app.editSel = list[list.length - 1] ?? null;
    const s = app.editSel;
    if (s?.kind === 'line') app.curId = s.id;
  }

  /** Click: select; ⇧-click adds or removes; a click inside the selection keeps it for a drag. */
  function pick(s: EditSel, e: MouseEvent) {
    const k = keyOf(s);
    if (e.shiftKey) {
      app.editMany = selKeys.has(k) ? app.editMany.filter((x) => keyOf(x) !== k) : [...app.editMany, s];
      app.editSel = s;
    } else if (!selKeys.has(k)) {
      app.editMany = [s];
      app.editSel = s;
    } else app.editSel = s;
    if (s.kind === 'line') app.curId = s.id;
  }

  function selRange(): [number, number] | null {
    const cs = clips.filter((c) => selKeys.has(c.key));
    if (!cs.length) return null;
    return [Math.min(...cs.map((c) => c.start)), Math.max(...cs.map((c) => c.end))];
  }

  /** ←/→: the previous or next clip on the timeline. */
  function stepClip(d: number) {
    if (!clips.length) return;
    const cur = app.editSel ? clips.findIndex((c) => c.key === keyOf(app.editSel!)) : -1;
    let i: number;
    if (cur >= 0) i = cur + d;
    else if (d > 0) i = clips.findIndex((c) => c.start >= app.pos - 1e-6);
    else i = clips.filter((c) => c.start < app.pos - 1e-6).length - 1;
    const c = clips[i];
    if (!c) return;
    selectOnly([c.sel]);
    reveal(c.start, c.end);
  }

  /** , and .: the playhead to the previous or next word. */
  const wordStarts = $derived(
    lines
      .flatMap((l) => l.words.filter((w) => !w.deleted).map((w) => w.start_s))
      .sort((a, b) => a - b),
  );
  function stepWord(d: number) {
    const t = app.pos;
    const to = d > 0 ? wordStarts.find((w) => w > t + 0.005) : wordStarts.filter((w) => w < t - 0.005).at(-1);
    if (to == null) return;
    app.seek(to);
    reveal(to, to);
  }

  function selectFollowing() {
    const from = selRange()?.[0] ?? app.pos;
    selectOnly(clips.filter((c) => c.start >= from - 1e-6).map((c) => c.sel));
  }

  function cycleToSel() {
    const r = selRange();
    if (r) app.cycle = { on: true, from: r[0], to: r[1] };
  }

  // ---- commands ----
  async function batch(cmds: Record<string, unknown>[]) {
    if (!cmds.length) return;
    await app.cmd(cmds.length === 1 ? cmds[0] : { cmd: 'batch', commands: cmds });
    await app.loadEdit();
  }

  /** Where a moved sound goes: anchored ones change their offset, pinned ones their time. */
  function soundMove(p: PlacementView, dt: number, snap: boolean): Record<string, unknown> {
    let to = p.start_s + dt;
    const s = snap && !p.anchored ? snapAt(to) : null;
    if (s) to = s.t;
    const patch = p.anchored ? { offset_s: r3(p.offset_s + dt) } : { at_s: r3(to), offset_s: 0 };
    return { cmd: 'update_placement', placement_id: p.id, patch };
  }

  /** Moves the selection. Locked dialogue changes its pause (ripple from the first one);
   *  unlocked dialogue moves freely in time; parts after a split change their offset. */
  async function moveSel(dt: number, alt = false, snap = true) {
    const ripple = alt ? !app.ripple : app.ripple;
    const cmds: Record<string, unknown>[] = [];
    const lineSel = selection.filter((s) => s.kind === 'line');
    const heads = lineSel
      .filter((s) => (s.part ?? 0) === 0)
      .map((s) => lineOf(s.id))
      .filter((l): l is ItemView => !!l)
      .sort((a, b) => a.start_s - b.start_s);
    let rippled = false;
    for (const l of heads) {
      if (l.pinned) cmds.push({ cmd: 'move_clip', line_id: l.line_id, delta_s: r3(dt), ripple: false });
      else if (!app.lockScript) cmds.push({ cmd: 'pin_clip', line_id: l.line_id, at_s: r3(Math.max(0, l.start_s + dt)) });
      else if (!ripple) cmds.push({ cmd: 'move_clip', line_id: l.line_id, delta_s: r3(dt), ripple: false });
      else if (!rippled) {
        cmds.push({ cmd: 'move_clip', line_id: l.line_id, delta_s: r3(dt), ripple: true });
        rippled = true;
      }
    }
    const moving = new Set(heads.map((l) => l.line_id));
    for (const s of lineSel) if ((s.part ?? 0) > 0 && !moving.has(s.id)) cmds.push({ cmd: 'move_segment', line_id: s.id, part: s.part, delta_s: r3(dt), ripple });
    for (const s of selection) {
      const p = s.kind === 'sound' ? soundOf(s.id) : undefined;
      if (p) cmds.push(soundMove(p, dt, snap && selection.length === 1));
    }
    await batch(cmds);
  }

  async function splitAt(sel: EditSel[], t: number) {
    const cmds: Record<string, unknown>[] = [];
    for (const c of clips) {
      if (!sel.some((s) => keyOf(s) === c.key) || t <= c.start + 0.01 || t >= c.end - 0.01) continue;
      if (c.sel.kind === 'line') cmds.push({ cmd: 'split_clip', line_id: c.sel.id, at_s: r3(t) });
      else if (c.sel.kind === 'sound') cmds.push({ cmd: 'split_placement', placement_id: c.sel.id, at_s: r3(t) });
    }
    if (!cmds.length) return app.say('Nothing to split at the playhead');
    await batch(cmds);
  }

  /** ⌘T: the selected clips under the playhead, else the dialogue under it. */
  async function splitAtPlayhead() {
    const t = app.pos;
    const under = (c: ClipRef) => t > c.start + 0.01 && t < c.end - 0.01;
    let sel = selection.filter((s) => clips.some((c) => c.key === keyOf(s) && under(c)));
    if (!sel.length) sel = clips.filter((c) => c.sel.kind === 'line' && under(c)).map((c) => c.sel);
    await splitAt(sel, t);
  }

  /** ⌥[ / ⌥]: the start or end of the selected clips under the playhead to the playhead. */
  async function trimToPlayhead(edge: 'start' | 'end') {
    const t = app.pos;
    const cmds: Record<string, unknown>[] = [];
    for (const c of clips) {
      if (!selKeys.has(c.key) || t <= c.start + 0.05 || t >= c.end - 0.05) continue;
      const d = r3(edge === 'start' ? t - c.start : t - c.end);
      if (c.sel.kind === 'line') cmds.push({ cmd: 'trim_clip', line_id: c.sel.id, part: c.sel.part ?? 0, edge, delta_s: d, ripple: app.ripple });
      else if (c.sel.kind === 'sound') cmds.push({ cmd: 'trim_placement', placement_id: c.sel.id, edge, delta_s: d });
    }
    if (!cmds.length) return app.say('Select a clip under the playhead to trim it there');
    await batch(cmds);
  }

  const mutedOf = (s: EditSel) => (s.kind === 'line' ? !!partOf(lineOf(s.id)!, s.part ?? 0)?.muted : s.kind === 'sound' ? !!soundOf(s.id)?.muted : false);

  /** ⌃M: mutes the selection (or unmutes it when everything in it is muted). */
  async function toggleMute() {
    let sel = selection.filter((s) => s.kind !== 'roomtone');
    if (!sel.length && curLine) sel = [{ kind: 'line', id: curLine.line_id }];
    const muted = sel.some((s) => !mutedOf(s));
    await batch(
      sel.map((s) =>
        s.kind === 'line'
          ? { cmd: 'set_clip_muted', line_id: s.id, part: s.part ?? null, muted }
          : { cmd: 'update_placement', placement_id: s.id, patch: { muted } },
      ),
    );
  }

  /** ⌫: sounds go; dialogue is ripple-deleted (Ripple on) or muted in place (Ripple off). */
  async function deleteSel() {
    const sel = selection.filter((s) => s.kind !== 'roomtone');
    if (!sel.length) return;
    const lineCmd = (s: EditSel) =>
      app.ripple ? { cmd: 'set_clip_dropped', line_id: s.id, part: s.part ?? 0, dropped: true } : { cmd: 'set_clip_muted', line_id: s.id, part: s.part ?? 0, muted: true };
    await batch(sel.map((s) => (s.kind === 'line' ? lineCmd(s) : { cmd: 'delete_placement', placement_id: s.id })));
    if (sel.some((s) => s.kind === 'line')) app.say(app.ripple ? 'Ripple-deleted: the time closed up (⌘Z or the ✂ mark restores it)' : 'Muted in place (Ripple is off)');
    selectOnly([]);
  }

  async function restorePart(l: ItemView, p: LayoutPart) {
    await batch([{ cmd: 'set_clip_dropped', line_id: l.line_id, part: p.index, dropped: false }]);
  }

  /** The clip's lock: locked follows the script; unlocked is free in time. */
  async function toggleLock(l: ItemView) {
    await batch([{ cmd: 'pin_clip', line_id: l.line_id, at_s: l.pinned ? null : r3(l.start_s) }]);
  }

  function laneMix(which: 'mute' | 'solo') {
    const s = app.editSel;
    const key = !s ? null : s.kind === 'line' ? (lineOf(s.id)?.character ?? null) : s.kind === 'sound' ? (soundOf(s.id) ? placementLane(soundOf(s.id)!) : null) : 'roomtone';
    if (!key) return;
    const m = mixOf(key);
    void setMix(key, which === 'mute' ? { mute: !m.mute } : { solo: !m.solo });
  }

  // ---- pointer gestures ----
  function track(e: MouseEvent, move: (dx: number, dy: number, m: MouseEvent) => void, done: (dx: number, dy: number, u: MouseEvent) => void | Promise<void>, click?: (u: MouseEvent) => void) {
    e.preventDefault();
    const x0 = e.clientX;
    const y0 = e.clientY;
    let moved = false;
    const mv = (m: MouseEvent) => {
      if (Math.abs(m.clientX - x0) > 3 || Math.abs(m.clientY - y0) > 3) moved = true;
      if (moved) move(m.clientX - x0, m.clientY - y0, m);
    };
    const up = (u: MouseEvent) => {
      window.removeEventListener('mousemove', mv);
      window.removeEventListener('mouseup', up);
      if (moved) void done(u.clientX - x0, u.clientY - y0, u);
      else click?.(u);
    };
    window.addEventListener('mousemove', mv);
    window.addEventListener('mouseup', up);
  }

  function clipDown(e: MouseEvent, s: EditSel) {
    if (e.button !== 0) return;
    e.stopPropagation();
    pick(s, e);
    if (app.tool === 'split') return void splitAt([s], timeAt(e.clientX));
    if (app.tool === 'gain') {
      const l = s.kind === 'line' ? lineOf(s.id) : undefined;
      const p = s.kind === 'sound' ? soundOf(s.id) : undefined;
      const g0 = l?.gain_db ?? p?.gain_db ?? 0;
      return track(
        e,
        () => {},
        async (_dx, dy) => {
          const g = +clamp(g0 - dy * 0.1, -40, 12).toFixed(1);
          if (l) await batch([{ cmd: 'set_clip_gain', line_id: l.line_id, gain_db: g }]);
          else if (p) await batch([{ cmd: 'update_placement', placement_id: p.id, patch: { gain_db: g } }]);
        },
      );
    }
    if (app.tool === 'fade') {
      // drag in from an edge: the left half sets the fade in, the right half the fade out
      const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
      return fadeDrag(e, s, e.clientX - r.left < r.width / 2 ? 'in' : 'out', r);
    }
    if (app.tool !== 'pointer') return;
    const keys = selKeys.has(keyOf(s)) ? [...selKeys] : [keyOf(s)];
    track(
      e,
      (dx) => (drag = { keys, dx }),
      async (dx, _dy, u) => {
        // the preview holds until the new layout lands, so the clip doesn't snap back first
        try {
          await moveSel(dx / px, u.altKey);
        } finally {
          drag = null;
        }
      },
      () => {
        if (!e.shiftKey) selectOnly([s]);
      },
    );
  }

  /** Fade handles (and the fade tool): drag sets the fade length at that end. */
  function fadeDrag(e: MouseEvent, s: EditSel, which: 'in' | 'out', r: DOMRect) {
    e.stopPropagation();
    const k = keyOf(s);
    const l = s.kind === 'line' ? lineOf(s.id) : undefined;
    const p = s.kind === 'sound' ? soundOf(s.id) : undefined;
    const len = l ? l.end_s - l.start_s : (p?.len_s ?? 0);
    const f0 = { fade_in_s: l?.fade_in_s ?? p?.fade_in_s ?? 0, fade_out_s: l?.fade_out_s ?? p?.fade_out_s ?? 0 };
    const set = (dx: number) => {
      const base = which === 'in' ? (e.clientX - r.left + dx) / px : (r.right - e.clientX - dx) / px;
      const f = +clamp(base, 0, len / 2).toFixed(3);
      fadeEdit = { key: k, ...f0, ...(which === 'in' ? { fade_in_s: f } : { fade_out_s: f }) };
    };
    set(0);
    track(
      e,
      (dx) => set(dx),
      async () => {
        const f = fadeEdit;
        try {
          if (!f) return;
          if (l) await batch([{ cmd: 'set_clip_fades', line_id: l.line_id, fade_in_s: f.fade_in_s, fade_out_s: f.fade_out_s }]);
          else if (p) await batch([{ cmd: 'update_placement', placement_id: p.id, patch: { fade_in_s: f.fade_in_s, fade_out_s: f.fade_out_s } }]);
        } finally {
          fadeEdit = null;
        }
      },
      () => (fadeEdit = null),
    );
  }

  /** Trim handles: the edge moves, the audio stays; the hidden audio shows faintly. */
  function edgeDown(e: MouseEvent, s: EditSel, edge: 'start' | 'end') {
    if (e.button !== 0 || app.tool !== 'pointer') return;
    e.stopPropagation();
    pick(s, e);
    const k = keyOf(s);
    let at: number;
    let lo: number;
    let hi: number;
    let cands: number[] = [app.pos];
    let src: { id: string; a: number; z: number; t: number };
    if (s.kind === 'line') {
      const l = lineOf(s.id);
      const pt = l && partOf(l, s.part ?? 0);
      const own = l?.pieces.filter((pc) => pc.part === (s.part ?? 0)) ?? [];
      const pc = edge === 'start' ? own[0] : own[own.length - 1];
      if (!l || !pt || !pc) return;
      at = edge === 'start' ? pt.start_s : pt.end_s;
      const len = pt.end_s - pt.start_s - 0.05;
      lo = edge === 'start' ? -(s.part ? HANDLE_S : pc.src_in) : -len;
      hi = edge === 'start' ? len : HANDLE_S;
      // word edges of the take, where they fall on the timeline
      const take = l.takes.find((t) => t.id === pc.take_id);
      if (take) for (const w of take.words) for (const ws of [w.t0, w.t1]) cands.push(pc.start_s + (take.src_in + ws - pc.src_in));
      const a = edge === 'start' ? Math.max(0, pc.src_in - HANDLE_S) : pc.src_in;
      const z = edge === 'start' ? pc.src_out : pc.src_out + HANDLE_S;
      src = { id: pc.source_id, a, z, t: pc.start_s - (pc.src_in - a) };
    } else {
      const p = soundOf(s.id);
      if (!p) return;
      at = edge === 'start' ? p.start_s : p.start_s + p.len_s;
      lo = edge === 'start' ? -p.in_s : -(p.len_s - 0.05);
      hi = edge === 'start' ? p.len_s - 0.05 : Math.max(0, p.asset_len_s - p.in_s - p.len_s);
      cands = [...cands, ...wordStarts];
      const a = Math.max(0, p.in_s - (edge === 'start' ? HANDLE_S : 0));
      const z = edge === 'end' ? Math.min(p.asset_len_s, p.in_s + p.len_s + HANDLE_S) : p.in_s + p.len_s;
      src = { id: `asset:${p.asset_id}`, a, z, t: p.start_s - (p.in_s - a) };
    }
    trim = { key: k, edge, dt: 0, ghost: null };
    {
      const g = src;
      void call<number[][]>('peaks_batch', { ranges: [{ source_id: g.id, t0: g.a, t1: g.z, buckets: bucketsFor(g.z - g.a), normalize: true }] })
        .then((r) => {
          if (trim?.key === k) trim = { ...trim, ghost: { t: g.t, len: g.z - g.a, peaks: r[0] ?? [] } };
        })
        .catch(() => {});
    }
    track(
      e,
      (dx) => {
        if (!trim) return;
        const t = snapTo(at + dx / px, cands);
        trim = { ...trim, dt: clamp(t - at, lo, hi) };
      },
      async () => {
        const t = trim;
        try {
          if (!t || Math.abs(t.dt) < 0.0005) return;
          if (s.kind === 'line') await batch([{ cmd: 'trim_clip', line_id: s.id, part: s.part ?? 0, edge, delta_s: r3(t.dt), ripple: app.ripple }]);
          else await batch([{ cmd: 'trim_placement', placement_id: s.id, edge, delta_s: r3(t.dt) }]);
        } finally {
          trim = null;
        }
      },
      () => (trim = null),
    );
  }

  /** A clip's drawn span (with a trim in progress). */
  function geo(key: string, a: number, b: number) {
    if (trim?.key !== key) return { a, b };
    return trim.edge === 'start' ? { a: a + trim.dt, b } : { a, b: b + trim.dt };
  }
  const dragDx = (key: string) => (drag && app.tool === 'pointer' && drag.keys.includes(key) ? drag.dx : 0);

  // ---- ruler: the lower part moves the playhead (silent scrub), the strip on top sets the cycle ----
  function rulerDown(e: MouseEvent) {
    if (e.button !== 0) return;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    if (e.clientY - r.top < CYC) return cycleDown(e);
    scrubDown(e);
  }

  function scrubDown(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    const was = app.playing;
    if (was) void app.stop();
    scrubbing = true;
    app.scrubTo(timeAt(e.clientX));
    const mv = (m: MouseEvent) => app.scrubTo(timeAt(m.clientX));
    const up = () => {
      window.removeEventListener('mousemove', mv);
      window.removeEventListener('mouseup', up);
      scrubbing = false;
      if (was) void app.play(app.pos);
    };
    window.addEventListener('mousemove', mv);
    window.addEventListener('mouseup', up);
  }

  function cycleDown(e: MouseEvent) {
    const t0 = timeAt(e.clientX);
    track(
      e,
      (dx) => {
        const t1 = t0 + dx / px;
        app.cycle = { on: true, from: Math.max(0, Math.min(t0, t1)), to: Math.max(t0, t1) };
      },
      () => {},
      () => {
        if (app.cycle.to > app.cycle.from) app.cycle = { ...app.cycle, on: !app.cycle.on };
      },
    );
  }

  // ---- empty lanes: click moves the playhead; drag selects clips (marquee tool: cycle range) ----
  function laneDown(e: MouseEvent) {
    if (e.button !== 0) return;
    const t0 = timeAt(e.clientX);
    if (app.tool === 'marquee') {
      band = { x0: t0, x1: t0 };
      return track(
        e,
        (_dx, _dy, m) => band && (band = { ...band, x1: timeAt(m.clientX) }),
        () => {
          if (band && Math.abs(band.x1 - band.x0) * px > 4) app.cycle = { on: true, from: Math.min(band.x0, band.x1), to: Math.max(band.x0, band.x1) };
          band = null;
        },
        () => (band = null),
      );
    }
    const c0 = contentAt(e);
    track(
      e,
      (_dx, _dy, m) => {
        const c = contentAt(m);
        rubber = { x0: c0.x, y0: c0.y, x1: c.x, y1: c.y };
      },
      () => {
        const r = rubber;
        rubber = null;
        if (!r) return;
        const [ya, yb] = [Math.min(r.y0, r.y1), Math.max(r.y0, r.y1)];
        const [ta, tb] = [Math.min(r.x0, r.x1) / px, Math.max(r.x0, r.x1) / px];
        const keys = new Set(lanes.filter((l) => l.y < yb && l.y + l.h > ya).map((l) => l.key));
        const hit = clips.filter((c) => keys.has(c.lane) && c.end > ta && c.start < tb).map((c) => c.sel);
        selectOnly(e.shiftKey ? [...app.editMany, ...hit.filter((s) => !selKeys.has(keyOf(s)))] : hit);
      },
      () => {
        if (!e.shiftKey) selectOnly([]);
        app.seek(t0);
      },
    );
  }

  // ---- drops from the scene bucket ----
  let dropLane = $state<string | null>(null);
  const laneZone = (r: LaneRow): DropZone<'bucket'> | null =>
    r.lane === 'roomtone' || !r.lane
      ? null
      : {
          takes: 'bucket',
          over: () => {
            dropLane = r.key;
          },
          leave: () => {
            dropLane = null;
          },
          drop: (e, s) => void drop(e, r, s),
        };

  async function drop(e: MouseEvent, r: LaneRow, s: DragSound) {
    if (!v || r.lane === 'roomtone' || !r.lane) return;
    const t = timeAt(e.clientX);
    const snap = snapAt(t);
    const where = snap ? { anchor_line_id: snap.line_id, anchor_word: snap.word ?? undefined } : { at_s: +t.toFixed(3) };
    // a dialogue lane takes the sound as that character's (a vocal sits with their lines)
    const lane = r.lane;
    const character = r.lane === 'dialogue' ? r.char : undefined;
    if (s.asset_id) {
      await app.method('place_from_bucket', { scene: v.scene.id, asset_id: s.asset_id, cue_id: s.cue_id, lane, character, ...where });
      await app.loadEdit();
      return;
    }
    // a preview: downloads on drop, shown as a grey clip with a gold progress bar
    const gx = x(snap?.t ?? t);
    ghost = { x: gx, y: r.y + 4, w: Math.max(24, Math.min(x(s.duration_s || 1), width - gx)), name: s.name, progress: 0, snap: snap?.label ?? null };
    const job = await app.run(() => call<{ job_id: string }>('place_from_bucket', { scene: v!.scene.id, bucket_id: s.bucket_id, remote_id: s.remote_id, lane, character, ...where }));
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

  // ---- keys: Logic's key commands on the Edit page ----
  function onKey(e: KeyboardEvent) {
    if (app.page !== 'edit' || app.overlay) return;
    const t = e.target as HTMLElement | null;
    if (t && (t.tagName === 'INPUT' || t.tagName === 'SELECT' || t.tagName === 'TEXTAREA')) return;
    const k = e.key;
    const mod = e.metaKey || e.ctrlKey;
    const lr = k === 'ArrowLeft' || k === 'ArrowRight';
    const dir = k === 'ArrowLeft' ? -1 : 1;
    let handled = true;
    if (toolMenu) {
      // T, then the tool's letter (any other key closes the menu)
      const tool = TOOLS.find((x) => x.key.toLowerCase() === k.toLowerCase());
      if (tool) app.tool = tool.id;
      toolMenu = false;
      handled = !!tool || k === 'Escape';
    } else if (mod && lr) zoomBy(dir > 0 ? 1.5 : 1 / 1.5);
    else if (mod && (k === 'ArrowUp' || k === 'ArrowDown')) app.laneScale = clamp(app.laneScale * (k === 'ArrowUp' ? 1.25 : 0.8), 0.6, 3);
    else if (lr && e.altKey) void moveSel(dir * (e.shiftKey ? 0.1 : 0.01), false, false);
    else if (lr) stepClip(dir);
    else if (k === ',' || k === '.') stepWord(k === ',' ? -1 : 1);
    else if (mod && (k === 't' || k === 'T')) void splitAtPlayhead();
    else if (e.altKey && (e.code === 'BracketLeft' || e.code === 'BracketRight')) void trimToPlayhead(e.code === 'BracketLeft' ? 'start' : 'end');
    else if (e.ctrlKey && (k === 'm' || k === 'M')) void toggleMute();
    else if (mod && (k === 'u' || k === 'U')) cycleToSel();
    else if (mod && (k === 'a' || k === 'A')) selectOnly(clips.map((c) => c.sel));
    else if (k === 'Backspace' || k === 'Delete') void deleteSel();
    else if (mod) handled = false;
    else if (e.shiftKey && (k === 'f' || k === 'F')) selectFollowing();
    else if (k === 'z' || k === 'Z') void zoomToggle();
    else if (k === 't' || k === 'T') toolMenu = true;
    else if (k === 'm' || k === 'M') laneMix('mute');
    else if (k === 's' || k === 'S') laneMix('solo');
    else if (k === 'i' || k === 'I') app.editInspector = !app.editInspector;
    else if (k === 'x' || k === 'X') app.mixer = !app.mixer;
    else if (k === 'c' || k === 'C') {
      if (app.cycle.to <= app.cycle.from) {
        const l = curLine;
        app.cycle = { on: true, from: l?.start_s ?? app.pos, to: l?.end_s ?? app.pos + 4 };
      } else app.cycle = { ...app.cycle, on: !app.cycle.on };
    } else if (k === 'Escape' && app.tool !== 'pointer') app.tool = 'pointer';
    else if (k === 'Escape' && app.editMany.length) selectOnly([]);
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
  const selSound = $derived(app.editSel?.kind === 'sound' ? soundOf(app.editSel.id) : undefined);
  const selLine = $derived(app.editSel?.kind === 'line' ? lineOf(app.editSel.id) : undefined);
  const lcd = $derived.by(() => {
    const sc = `Sc. ${v?.scene.number ?? ''}`;
    const n = selection.length > 1 ? ` · ${selection.length} clips` : '';
    if (selSound) return `${sc} · ${laneLabel(placementLane(selSound))} · ${selSound.name.replace(/\.[a-z0-9]+$/i, '')}${n}`;
    if (selLine) return `${sc} · ${charLabel(v, selLine.character)} · T${(selLine.take_index ?? 0) + 1}${(app.editSel?.part ?? 0) > 0 ? ` · part ${(app.editSel?.part ?? 0) + 1}` : ''}${n}`;
    if (app.editSel?.kind === 'roomtone') return `${sc} · Room tone`;
    return `${sc} · ${v?.scene.place ?? ''}`;
  });
  const clipBg = (c: Rgb, strong: boolean) => `background: linear-gradient(180deg, ${rgba(c, strong ? 0.5 : 0.32)}, ${rgba(c, 0.14)})`;
  const ticks = $derived.by(() => {
    const step = [0.1, 0.2, 0.5, 1, 2, 5, 10, 15, 30, 60, 120].find((s) => s * px >= 70) ?? 120;
    const a = Math.max(0, Math.floor(vis0 / step));
    const b = Math.min(Math.floor(duration / step), Math.ceil(vis1 / step));
    return Array.from({ length: Math.max(0, b - a + 1) }, (_, i) => {
      const t = +((a + i) * step).toFixed(3);
      return { t, label: step < 1 ? fmt(t).replace(/^0/, '') : fmt(t).replace(/\.\d$/, '').replace(/^0/, '') };
    });
  });
  const left = $derived(app.editInspector ? 284 : 20);
  const right = $derived(app.editBucket ? 404 : 60);
  const partText = (l: ItemView, i: number) => lineText(l, i);
  const flat = Array.from({ length: 60 }, (_, i) => 0.18 + 0.1 * Math.abs(Math.sin(i * 1.7)));
  const firstKept = (l: ItemView) => kept(l)[0]?.index ?? -1;
  const lastKept = (l: ItemView) => kept(l).at(-1)?.index ?? -1;
  /** Which group a sound belongs to (for the collapsed preview). */
  const groupOf = (p: PlacementView): LaneGroup => (p.lane === 'dialogue' ? 'dialogue' : p.lane === 'sfx' ? 'sfx' : p.lane === 'music' ? 'music' : 'ambience');
  /** Words that fit their own width get their text; the rest only a tick. */
  const fits = (w: ShownWord) => (w.end - w.start) * px >= w.text.length * 5.6 + 6;
</script>

{#snippet edges(s: EditSel, fadeIn: number | null, fadeOut: number | null, len: number, sel: boolean, k: string)}
  {#if app.tool === 'pointer'}
    <span class="eh l" role="slider" tabindex="-1" aria-label="Trim start" aria-valuenow={0} onmousedown={(e) => edgeDown(e, s, 'start')}></span>
    <span class="eh r" role="slider" tabindex="-1" aria-label="Trim end" aria-valuenow={0} onmousedown={(e) => edgeDown(e, s, 'end')}></span>
  {/if}
  {#if sel && (fadeIn != null || fadeOut != null)}
    {@const fe = fadeEdit?.key === k ? fadeEdit : { fade_in_s: fadeIn ?? 0, fade_out_s: fadeOut ?? 0 }}
    {@const fi = Math.min(50, (fe.fade_in_s / Math.max(0.01, len)) * 100)}
    {@const fo = Math.max(50, 100 - (fe.fade_out_s / Math.max(0.01, len)) * 100)}
    <svg class="fx" viewBox="0 0 100 40" preserveAspectRatio="none" aria-hidden="true">
      {#if fadeIn != null}<polyline points="0,40 {fi},4" />{/if}
      {#if fadeOut != null}<polyline points="{fo},4 100,40" />{/if}
    </svg>
    {#if fadeIn != null}<span class="fh" role="slider" tabindex="-1" aria-label="Fade in" aria-valuenow={fe.fade_in_s} style="left: {fi}%" onmousedown={(e) => fadeDrag(e, s, 'in', (e.currentTarget as HTMLElement).parentElement!.getBoundingClientRect())}></span>{/if}
    {#if fadeOut != null}<span class="fh" role="slider" tabindex="-1" aria-label="Fade out" aria-valuenow={fe.fade_out_s} style="left: {fo}%" onmousedown={(e) => fadeDrag(e, s, 'out', (e.currentTarget as HTMLElement).parentElement!.getBoundingClientRect())}></span>{/if}
  {/if}
{/snippet}

{#snippet soundClip(p: PlacementView, rgb: Rgb, vocal: boolean)}
  {@const s = { kind: 'sound', id: p.id } as EditSel}
  {@const k = `sound:${p.id}`}
  {@const sel = selKeys.has(k)}
  {@const g = geo(k, p.start_s, p.start_s + p.len_s)}
  <div class="clip" class:sel class:vocal class:mclip={p.muted} role="button" tabindex="-1" onmousedown={(e) => clipDown(e, s)} title={p.name} style="left: {x(g.a) + dragDx(k)}px; width: {Math.max(4, x(g.b - g.a))}px; {clipBg(rgb, sel)}; box-shadow: inset 0 0 0 {sel ? '1.5px var(--playhead)' : `1px ${rgba(rgb, 0.45)}`}" data-testid={vocal ? 'vocal-clip' : 'sound-clip'}>
    <div class="pc" style="left: {x(p.start_s - g.a)}px; width: {x(p.len_s)}px"><Waveform peaks={soundPeaks(p)} fill={rgba(rgb, 0.75)} gain={app.waveGain} /></div>
    <span class="cl ellip">{p.muted ? 'Muted · ' : ''}{p.name.replace(/\.[a-z0-9]+$/i, '')}</span>
    {#if sel}<span class="gl" style="top: {clamp(50 - p.gain_db * 3, 6, 94)}%"></span>{/if}
    {@render edges(s, p.fade_in_s, p.fade_out_s, p.len_s, sel, k)}
  </div>
{/snippet}

<EditBar onZoom={zoomSlider} lcdDetail={lcd} />

{#if toolMenu}
  <div class="glass tmenu" role="menu" aria-label="Tools">
    {#each TOOLS as t (t.id)}
      <button role="menuitem" class:on={app.tool === t.id} onclick={() => { app.tool = t.id; toolMenu = false; }}><Icon name={t.id} size={14} stroke={1.8} /><span>{t.label}</span><span class="kb">{t.key}</span></button>
    {/each}
  </div>
{/if}

{#if app.editInspector}<EditInspector />{/if}

<section class="tl" aria-label="Timeline" style="left: {left}px; right: {right}px">
  <div class="scroll" bind:this={scroller} onscroll={() => (scrollX = scroller?.scrollLeft ?? 0)}>
    <div class="content" style="width: {HEAD + width + 24}px; height: {totalH}px">
      <!-- ruler, sentences, action lines and words (sticky on top, whatever is collapsed) -->
      <div class="top" style="width: {HEAD + width + 24}px; height: {TOP}px">
        <div class="hcol" style="height: {TOP}px"><span class="label stl">Script</span></div>
        <div class="area" style="left: {HEAD}px; width: {width}px; height: {TOP}px">
          <div class="ruler" role="slider" tabindex="-1" aria-label="Ruler" aria-valuenow={app.pos} onmousedown={rulerDown} data-testid="ruler">
            <div class="cstrip"></div>
            {#if app.cycle.to > app.cycle.from}<span class="cyc" class:off={!app.cycle.on} style="left: {x(app.cycle.from)}px; width: {x(app.cycle.to - app.cycle.from)}px"></span>{/if}
            {#each ticks as tk (tk.t)}<span class="tick muted tn" style="left: {x(tk.t)}px">{tk.label}</span>{/each}
          </div>
          {#each lines as l (l.line_id)}
            {#if !l.dropped}
              {@const c = charColor(v, l.character).rgb}
              {@const cur = curLine?.line_id === l.line_id}
              <span class="si script" class:cur class:smuted={l.muted} style="left: {x(l.start_s)}px; width: {Math.max(8, x(l.end_s - l.start_s))}px; top: 32px; {cur ? `background: ${rgba(c, 0.6)}` : `background: ${rgba(c, 0.22)}`}" title={l.text}>{l.text}</span>
            {/if}
          {/each}
          {#each actions as a (a.line_id)}
            <span class="si script act" style="left: {x(a.start_s)}px; width: {Math.max(40, x(Math.max(0.6, a.end_s - a.start_s)))}px; top: 52px" title={a.text}>{a.text}</span>
          {/each}
          {#each lines as l (l.line_id)}
            {@const c = charColor(v, l.character).rgb}
            {#each shownOf(l).filter((w) => w.end >= vis0 && w.start <= vis1) as w (w.j)}
              {@const on = app.pos >= w.start && app.pos < w.end}
              <span class="wd" class:on class:wm={w.muted} style="left: {x(w.start)}px; width: {Math.max(1, x(w.end - w.start))}px; border-color: {rgba(c, 0.8)}; {on ? `background: ${rgba(c, 0.55)}` : ''}" title={w.text} data-testid="strip-word">{fits(w) ? w.text : ''}</span>
            {/each}
          {/each}
          <div class="ph top-ph" style="left: {x(app.pos)}px"><span role="slider" tabindex="-1" aria-label="Playhead" aria-valuenow={app.pos} onmousedown={scrubDown}></span></div>
        </div>
      </div>

      <!-- lane groups -->
      {#each rows as r (r.key)}
        <div class="row" style="top: {r.y}px; height: {r.h}px; width: {HEAD + width + 24}px">
          {#if r.kind === 'group'}
            <button class="gh" onclick={() => (collapsed = { ...collapsed, [r.group]: !collapsed[r.group] })} aria-expanded={!collapsed[r.group]}>
              <span style="display: inline-flex; transform: rotate({collapsed[r.group] ? -90 : 0}deg)"><Icon name="chevron" size={11} stroke={2.4} /></span>{r.label}
            </button>
            {#if collapsed[r.group]}
              <!-- collapsed: where the group's clips are -->
              <div class="pv" style="left: {HEAD}px; width: {width}px" data-testid="group-preview" data-group={r.group}>
                {#if r.group === 'dialogue'}
                  {#each lines as l (l.line_id)}
                    {@const c = charColor(v, l.character).rgb}
                    {#each kept(l) as pt (pt.index)}
                      <span class="pb" class:mclip={pt.muted} style="left: {x(pt.start_s)}px; width: {Math.max(2, x(pt.end_s - pt.start_s))}px; background: {rgba(c, 0.55)}">{x(pt.end_s - pt.start_s) > 40 ? partText(l, pt.index) : ''}</span>
                    {/each}
                  {/each}
                {:else if r.group === 'ambience' && ev?.room_tone}
                  <span class="pb" style="left: 0; width: {x(v?.duration_s ?? 0)}px; background: {rgba(LANE_COLORS.room, 0.25)}"></span>
                {/if}
                {#each (ev?.placements ?? []).filter((p) => groupOf(p) === r.group) as p (p.id)}
                  <span class="pb" class:mclip={p.muted} style="left: {x(p.start_s)}px; width: {Math.max(2, x(p.len_s))}px; background: {rgba(laneRgb(placementLane(p)), 0.55)}">{x(p.len_s) > 50 ? p.name.replace(/\.[a-z0-9]+$/i, '') : ''}</span>
                {/each}
              </div>
            {/if}
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
            <div class="lane" class:muted-lane={m.mute} class:drop-on={dropLane === r.key} style="left: {HEAD}px; width: {width}px" role="application" aria-label="{r.label} lane" data-testid="edit-lane" data-lane={r.key} onmousedown={laneDown} use:dropzone={laneZone(r)}>
              {#if trim?.ghost && clips.find((c) => c.key === trim!.key)?.lane === r.key}
                <div class="tghost" style="left: {x(trim.ghost.t)}px; width: {x(trim.ghost.len)}px"><Waveform peaks={trim.ghost.peaks} fill={rgba(r.rgb, 0.5)} gain={app.waveGain} /></div>
              {/if}
              {#if r.lane === 'dialogue'}
                {#each lines.filter((l) => l.character === r.char) as l (l.line_id)}
                  {@const c = r.rgb}
                  {#each kept(l) as pt (pt.index)}
                    {@const s = { kind: 'line', id: l.line_id, part: pt.index } as EditSel}
                    {@const k = `line:${l.line_id}:${pt.index}`}
                    {@const sel = selKeys.has(k)}
                    {@const g = geo(k, pt.start_s, pt.end_s)}
                    {@const first = pt.index === firstKept(l)}
                    {@const last = pt.index === lastKept(l)}
                    <div class="clip" class:sel class:mclip={pt.muted} class:free={l.pinned} role="button" tabindex="-1" onmousedown={(e) => clipDown(e, s)} title="{l.text} (T{(l.take_index ?? 0) + 1}){pt.muted ? ' · muted' : ''}{l.pinned ? ' · unlocked' : ''}" style="left: {x(g.a) + dragDx(k)}px; width: {Math.max(4, x(g.b - g.a))}px; {clipBg(c, sel)}; box-shadow: inset 0 0 0 {sel ? '1.5px var(--playhead)' : `1px ${rgba(c, 0.45)}`}" data-testid="dialogue-clip" data-line={l.line_id} data-part={pt.index}>
                      {#each l.pieces.filter((pc) => pc.part === pt.index) as pc, pi (pi)}
                        <div class="pc" style="left: {x(pc.start_s - g.a)}px; width: {x(pc.len_s)}px"><Waveform peaks={piecePeaks(l, pc)} fill={rgba(c, 0.75)} gain={app.waveGain} /></div>
                      {/each}
                      {#each shownOf(l).filter((w) => w.part === pt.index) as w (w.j)}
                        <span class="wk" style="left: {x(w.start - g.a)}px; width: {x(w.end - w.start)}px">{fits(w) ? w.text : ''}</span>
                      {/each}
                      <span class="cl ellip">{pt.muted ? 'Muted · ' : ''}{first ? lineText(l) : partText(l, pt.index)}</span>
                      {#if first}
                        <button class="lk" class:open={l.pinned} onmousedown={(e) => e.stopPropagation()} onclick={() => toggleLock(l)} title={l.pinned ? 'Unlocked: free in time. Click to lock it to the script where it is' : 'Locked to the script. Click to unlock (free in time)'} aria-label={l.pinned ? 'Lock clip' : 'Unlock clip'}><Icon name={l.pinned ? 'unlock' : 'lock'} size={11} /></button>
                      {/if}
                      {@render edges(s, first ? l.fade_in_s : null, last ? l.fade_out_s : null, g.b - g.a, sel, k)}
                    </div>
                  {/each}
                  {#each l.parts.filter((p) => p.dropped) as pt (pt.index)}
                    <button class="cutmark" style="left: {x(pt.start_s) - 6}px" onmousedown={(e) => e.stopPropagation()} onclick={() => restorePart(l, pt)} title="Ripple-deleted: click to restore" aria-label="Restore cut part" data-testid="cut-mark">✂</button>
                  {/each}
                {/each}
                {#each (ev?.placements ?? []).filter((p) => p.lane === 'dialogue' && p.character === r.char) as p (p.id)}{@render soundClip(p, r.rgb, true)}{/each}
              {:else if r.lane === 'roomtone'}
                {#if ev?.room_tone}
                  {@const sel = app.editSel?.kind === 'roomtone'}
                  <div class="clip" class:sel role="button" tabindex="-1" onmousedown={(e) => { e.stopPropagation(); selectOnly([{ kind: 'roomtone', id: 'roomtone' }]); }} style="left: 0; width: {x(v?.duration_s ?? duration)}px; {clipBg(r.rgb, sel)}; box-shadow: inset 0 0 0 {sel ? '1.5px var(--playhead)' : `1px ${rgba(r.rgb, 0.45)}`}">
                    <Waveform peaks={flat} fill={rgba(r.rgb, 0.75)} />
                    <span class="cl ellip">{v?.scene.place} room tone · loop</span>
                  </div>
                  {#each ev.room_tone.seams.filter((s) => s < (v?.duration_s ?? 0)) as s (s)}<span class="seam" style="left: {x(s)}px"></span>{/each}
                {/if}
              {:else}
                {#each (ev?.placements ?? []).filter((p) => placementLane(p) === r.key) as p (p.id)}{@render soundClip(p, r.rgb, false)}{/each}
              {/if}
            </div>
          {/if}
        </div>
      {/each}

      <!-- overlay: anchors, ghost of a downloading drop, marquee, rubber band, playhead -->
      <div class="ov" style="left: {HEAD}px; width: {width}px; height: {totalH}px">
        {#each ev?.placements ?? [] as p (p.id)}
          {@const a = p.anchor}
          {@const it = a ? v?.items.find((i) => i.line_id === a.line_id) : undefined}
          {@const at = !a || !it ? null : a.word != null ? (it.words.find((w) => w.script_idx === a.word)?.[a.edge === 'end' ? 'end_s' : 'start_s'] ?? null) : a.edge === 'end' ? it.end_s : it.start_s}
          {@const r = rowOf(placementLane(p))}
          {#if at != null && r}
            {@const ar = it?.character ? rowOf(it.character) : undefined}
            {@const y0 = ar ? ar.y + ar.h / 2 : TOP - 18}
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
        {#if rubber}<span class="rubber" style="left: {Math.min(rubber.x0, rubber.x1)}px; top: {Math.min(rubber.y0, rubber.y1)}px; width: {Math.abs(rubber.x1 - rubber.x0)}px; height: {Math.abs(rubber.y1 - rubber.y0)}px"></span>{/if}
        {#if trim}
          {@const c = clips.find((cc) => cc.key === trim!.key)}
          {#if c}<span class="trimline" style="left: {x((trim.edge === 'start' ? c.start : c.end) + trim.dt)}px"></span>{/if}
        {/if}
        <div class="ph" style="left: {x(app.pos)}px"></div>
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
  }
  .ruler {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    height: 30px;
    cursor: text;
    border-bottom: 1px solid rgb(var(--veil-rgb) / 0.08);
  }
  /* the cycle strip on top of the ruler */
  .cstrip {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    height: 10px;
    cursor: ew-resize;
  }
  .cyc {
    position: absolute;
    top: 1px;
    height: 8px;
    border-radius: 4px;
    background: rgb(var(--ink-rgb) / 0.32);
    pointer-events: none;
  }
  .cyc.off {
    background: rgb(var(--ink-rgb) / 0.08);
  }
  .tick {
    position: absolute;
    top: 13px;
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
  .lane.drop-on {
    box-shadow: inset 0 0 0 1.5px var(--sfx);
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
  /* a character's vocal on their dialogue lane: dashed, so it reads apart from the takes */
  .clip.vocal:not(.sel) {
    outline: 1px dashed rgb(var(--ink-rgb) / 0.4);
    outline-offset: -1px;
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
    width: 9px;
    height: 9px;
    margin-left: -4px;
    border-radius: 2px;
    background: var(--playhead);
    cursor: ew-resize;
    z-index: 2;
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
    top: 92px;
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
  /* the playhead handle sits in the ruler and can be dragged (silent scrub) */
  .top-ph {
    z-index: 3;
  }
  .top-ph span {
    position: absolute;
    top: 10px;
    left: -6px;
    width: 14px;
    height: 14px;
    border-radius: 3px 3px 7px 7px;
    background: var(--playhead);
    pointer-events: auto;
    cursor: ew-resize;
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
  .si.smuted {
    opacity: 0.4;
  }
  /* the words of the script strip, each at its time */
  .wd {
    position: absolute;
    top: 74px;
    height: 15px;
    box-sizing: border-box;
    border-left: 1.5px solid;
    padding-left: 3px;
    font-size: 10.5px;
    line-height: 15px;
    white-space: nowrap;
    overflow: hidden;
    color: rgb(var(--ink-rgb) / 0.7);
    border-radius: 0 4px 4px 0;
  }
  .wd.on {
    color: var(--text-strong);
    font-weight: 700;
  }
  .wd.wm {
    opacity: 0.4;
  }
  .pc {
    position: absolute;
    top: 0;
    bottom: 0;
    pointer-events: none;
  }
  /* word markers inside a dialogue clip */
  .wk {
    position: absolute;
    bottom: 1px;
    height: 12px;
    border-left: 1px solid rgb(var(--ink-rgb) / 0.45);
    padding-left: 2px;
    font-size: 9.5px;
    line-height: 12px;
    color: rgb(var(--ink-rgb) / 0.8);
    white-space: nowrap;
    overflow: hidden;
    pointer-events: none;
  }
  /* trim handles on both edges */
  .eh {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 7px;
    cursor: ew-resize;
    z-index: 1;
  }
  .eh.l {
    left: 0;
  }
  .eh.r {
    right: 0;
  }
  .clip:hover .eh,
  .clip.sel .eh {
    background: linear-gradient(90deg, rgb(var(--ink-rgb) / 0.28), transparent);
  }
  .clip:hover .eh.r,
  .clip.sel .eh.r {
    background: linear-gradient(270deg, rgb(var(--ink-rgb) / 0.28), transparent);
  }
  /* clip mute: kept in place, greyed */
  .clip.mclip,
  .pb.mclip {
    filter: grayscale(1);
    opacity: 0.4;
  }
  /* unlocked (free in time) */
  .clip.free:not(.sel) {
    outline: 1px dashed rgb(var(--ink-rgb) / 0.55);
    outline-offset: -1px;
  }
  .lk {
    position: absolute;
    right: 9px;
    top: 2px;
    width: 16px;
    height: 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 5px;
    color: rgb(var(--ink-rgb) / 0.75);
    opacity: 0;
    z-index: 2;
  }
  .lk.open,
  .clip:hover .lk,
  .clip.sel .lk {
    opacity: 1;
  }
  .lk:hover {
    background: rgb(var(--ink-rgb) / 0.15);
  }
  .cutmark {
    position: absolute;
    top: 50%;
    width: 12px;
    height: 18px;
    margin-top: -9px;
    border-radius: 4px;
    font-size: 10px;
    line-height: 18px;
    text-align: center;
    color: var(--text-on-pill);
    background: var(--warn);
    z-index: 2;
  }
  /* a collapsed group: its clips as thin blocks */
  .pv {
    position: absolute;
    top: 6px;
    bottom: 4px;
    border-radius: 6px;
    background: rgb(var(--veil-rgb) / 0.03);
    overflow: hidden;
    pointer-events: none;
  }
  .pb {
    position: absolute;
    top: 0;
    bottom: 0;
    border-radius: 3px;
    font-size: 10px;
    line-height: 20px;
    padding: 0 4px;
    box-sizing: border-box;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: rgb(var(--ink-rgb) / 0.9);
  }
  /* the audio around a clip while trimming */
  .tghost {
    position: absolute;
    top: 2px;
    bottom: 2px;
    border-radius: 8px;
    opacity: 0.35;
    background: rgb(var(--ink-rgb) / 0.06);
    pointer-events: none;
  }
  .trimline {
    position: absolute;
    top: 92px;
    bottom: 0;
    width: 0;
    border-left: 1px solid var(--playhead);
  }
  .rubber {
    position: absolute;
    background: rgb(var(--ink-rgb) / 0.08);
    box-shadow: inset 0 0 0 1px rgb(var(--ink-rgb) / 0.4);
  }
  /* T: the tool menu */
  .tmenu {
    position: absolute;
    top: 126px;
    left: 110px;
    z-index: 20;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px;
    border-radius: 14px;
    background: var(--bar-strong);
    min-width: 170px;
  }
  .tmenu button {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 30px;
    padding: 0 10px;
    border-radius: 9px;
    font-size: 12.5px;
  }
  .tmenu button span:nth-child(2) {
    flex: 1;
    text-align: left;
  }
  .tmenu button.on,
  .tmenu button:hover {
    background: rgb(var(--ink-rgb) / 0.12);
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
