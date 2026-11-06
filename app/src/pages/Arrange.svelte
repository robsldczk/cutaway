<script lang="ts">
  import { dropzone, type DropZone } from '../lib/drag.svelte';
  import type { ActionRow } from '../lib/sounds/snap';
  import ClipState from '../components/ClipState.svelte';
  import Icon from '../components/Icon.svelte';
  import LineMenu from '../components/LineMenu.svelte';
  import NoteBadge from '../components/NoteBadge.svelte';
  import { tick } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import { clamp, fmt } from '../lib/format';
  import { charColor, charLabel, itemShade, rgba } from '../lib/colors';
  import { openEditor } from '../lib/keys';
  import SoundsLayer from '../components/SoundsLayer.svelte';
  import Waveform from '../components/Waveform.svelte';
  import { rowSpans, TimeMap, type RowBox } from '../lib/sounds/timemap';
  import type { ItemView } from '../lib/types';
  import { depthText, rangeLines, sourceAt, timeAt, toggleLine } from '../lib/lines';
  import { lineWords } from '../lib/words';

  const sc = $derived(app.scale);
  const S = $derived(Math.round(92 * sc));

  /** With the sounds layer on, the dialogue column moves over to make room for it. */
  const COLUMN_LEFT = 92;
  const on = $derived(app.soundsOn);
  const columnW = $derived(Math.round(400 * sc));
  const spineX = $derived(COLUMN_LEFT + columnW + 32);

  interface Row {
    it: ItemView;
    j: number;
    gapAfter: number | null; // pause before the next line
    acts: ItemView[]; // action lines in that pause
    next: ItemView | null;
  }

  const rows = $derived.by(() => {
    const v = app.view;
    if (!v) return [] as Row[];
    const out: Row[] = [];
    const lines = v.items.filter((i) => i.kind === 'line');
    lines.forEach((it, j) => {
      const next = lines[j + 1] ?? null;
      const acts: ItemView[] = [];
      if (next) {
        const a = v.items.indexOf(it);
        const b = v.items.indexOf(next);
        for (const x of v.items.slice(a + 1, b)) if (x.kind === 'spacer') acts.push(x);
      }
      out.push({ it, j, gapAfter: next ? next.gap_before_s : null, acts, next });
    });
    return out;
  });

  /** A row, or two side by side (D125). */
  const groups = $derived.by(() => {
    const out: Row[][] = [];
    for (const r of rows) {
      if (r.it.beside && out.length) out[out.length - 1].push(r);
      else out.push([r]);
    }
    return out;
  });

  // actions before the first line and after the last have no pause to sit in
  const lineItems = $derived((app.view?.items ?? []).filter((i) => i.kind === 'line'));
  const head = $derived.by(() => {
    const items = app.view?.items ?? [];
    const first = lineItems[0];
    return (first ? items.slice(0, items.indexOf(first)) : items).filter((i) => i.kind === 'spacer');
  });
  const tail = $derived.by(() => {
    const items = app.view?.items ?? [];
    const last = lineItems[lineItems.length - 1];
    return last ? items.slice(items.indexOf(last) + 1).filter((i) => i.kind === 'spacer') : [];
  });
  let actEls: Record<string, HTMLDivElement> = $state({});
  let actionRows = $state.raw<ActionRow[]>([]);
  let dropOn = $state<string | null>(null);
  let actPreview = $state<{ id: string; s: number } | null>(null);
  const actLen = (a: ItemView) => (actPreview?.id === a.line_id ? actPreview.s : a.spacer_s);
  /** A direction's time, on top of its text: the block grows from the first tenth of a second.
   *  One turned off takes no time until it's dragged. */
  const ACT_PX_PER_S = 14;
  const actRoom = (a: ItemView) => Math.round((a.spacer_enabled || actPreview?.id === a.line_id ? actLen(a) : 0) * ACT_PX_PER_S * sc);

  /** Sounds hanging on an action. */
  const tagsOf = (a: ItemView) => (on ? (app.edit?.placements ?? []).filter((p) => p.anchor?.line_id === a.line_id) : []);

  const actionZone = (a: ItemView): DropZone<'bucket'> | null =>
    on
      ? {
          takes: 'bucket',
          over: () => {
            dropOn = a.line_id;
          },
          leave: () => {
            dropOn = null;
          },
          drop: (_, s) => app.soundsUi?.dropOnAction(a.line_id, s),
        }
      : null;

  // local previews while dragging; committed on mouse up
  let gapPreview = $state<{ id: string; g: number } | null>(null);
  // in and out in source seconds
  let trimPreview = $state<{ id: string; a: number; z: number } | null>(null);
  let drag = $state<{ id: string; dx: number; dy: number; y0: number } | null>(null);
  /** The line a dragged one would land next to. */
  let besideOn = $state<string | null>(null);
  let offPreview = $state<{ id: string; o: number } | null>(null);
  let rowEls: Record<string, HTMLDivElement> = $state({});
  let pairEls: Record<string, HTMLDivElement> = $state({});
  let off = $state(0);

  // Time on this page comes from where the cards sit: one map for the spine, the lines,
  // the balls and the snapping.
  let boxes = $state.raw<RowBox[]>([]);
  const map = $derived(new TimeMap(rowSpans(boxes), 14 * sc));

  $effect(() => {
    void app.view;
    void sc;
    void gapPreview;
    void actPreview;
    void offPreview;
    // action text wraps to the column's width, so it's measured once that has settled
    void stageW;
    void moving;
    void on;
    void tick().then(() => {
      const next: RowBox[] = [];
      // directions before the first line have time of their own (D94)
      for (const a of head) {
        const el = actEls[a.line_id];
        if (el) next.push({ start_s: a.start_s, end_s: a.end_s, top: el.offsetTop, height: el.offsetHeight });
      }
      for (const g of groups) {
        // two lines side by side stand for one stretch of time
        const el = g.length > 1 ? pairEls[g[0].it.line_id] : rowEls[g[0].it.line_id];
        const start_s = Math.min(...g.map((r) => r.it.start_s));
        const end_s = Math.max(...g.map((r) => r.it.end_s));
        if (el) next.push({ start_s, end_s, top: el.offsetTop, height: el.offsetHeight });
      }
      boxes = next;
      // the column takes its first place at once: there's nothing to glide from
      requestAnimationFrame(() => requestAnimationFrame(() => (placed = true)));
      const measure = (a: ItemView, until: number) => {
        const el = actEls[a.line_id];
        return el ? [{ line_id: a.line_id, text: a.text, start_s: a.start_s, until, y: el.offsetTop + el.offsetHeight / 2, height: el.offsetHeight }] : [];
      };
      actionRows = [...head.flatMap((a) => measure(a, a.start_s)), ...rows.flatMap((r) => r.acts.flatMap((a) => measure(a, r.next?.start_s ?? a.start_s))), ...tail.flatMap((a) => measure(a, a.start_s))];
    });
  });

  $effect(() => {
    void app.curId;
    void app.view;
    void sc;
    void on;
    void tick().then(() => {
      if (on) return;
      const el = app.curId ? rowEls[app.curId] : undefined;
      if (el) off = el.offsetTop + el.offsetHeight / 2;
    });
  });

  // the sounds layer scrolls in time, so the playhead stays on the centre line
  $effect(() => {
    if (!on || app.audition || (app.playing && !app.follow)) return;
    off = map.y(app.pos);
  });

  // The column moves between its two places as one thing: `left` and `width`, in pixels, on one
  // curve. A centring translate on its own timing adds up with `left` and swings the column
  // far past where it's going (D89). Only the toggle animates this: a window resize doesn't.
  let stageW = $state(window.innerWidth);
  const plainW = $derived(Math.min(Math.round(600 * sc), stageW * 0.86));
  let moving = $state(false);
  let placed = $state(false);
  let moveTimer: ReturnType<typeof setTimeout> | undefined;
  let wasOn: boolean | null = null;
  $effect(() => {
    const now = on;
    if (wasOn !== null && wasOn !== now) {
      moving = true;
      clearTimeout(moveTimer);
      moveTimer = setTimeout(() => (moving = false), 800);
    }
    wasOn = now;
  });

  /** While the playhead runs the column follows it closely; a jump glides; the toggle is one move. */
  const slide = $derived(drag || !placed ? 'none' : moving ? 'transform var(--dur-move) var(--ease-move)' : on && app.playing ? 'transform .1s linear' : 'transform .3s var(--ease-out)');

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

  /** Where a moment of the scene sits on a line's strip. A comp shows one take of several, so
   *  what comes from another recording goes by its share of the line. */
  function atTime(it: ItemView, t: number) {
    const tr = trims(it);
    return fx(it, sourceAt(it, t, takeOf(it)?.source_id) ?? tr.a + t - it.start_s);
  }

  // ---- the moved sound's marker, on the blocks and across the pauses (D128) ----

  const speaks = (it: ItemView, t: number) => t >= it.start_s && t <= it.end_s && it.end_s > it.start_s;
  /** No line speaks at that moment: the marker crosses the column at its height. */
  const cueGapY = $derived(app.soundCue && !rows.some((r) => speaks(r.it, app.soundCue!.t)) ? map.y(app.soundCue.t) : null);

  // ---- several lines, and the right click (D129, D130) ----

  let menu = $state<{ lineId: string; ids: string[]; x: number; y: number } | null>(null);

  function pick(e: MouseEvent, it: ItemView) {
    const cur = app.cur?.line_id ?? null;
    // the column stays where it is: the line clicked doesn't become the current one
    if (e.metaKey || e.ctrlKey) app.lineSel = toggleLine(app.lineSel, cur, it.line_id);
    else if (e.shiftKey) app.lineSel = rangeLines(rows.map((r) => r.it.line_id), cur, it.line_id);
    else app.select(it.line_id);
  }

  function openMenu(e: MouseEvent, it: ItemView) {
    e.preventDefault();
    e.stopPropagation();
    if (!it.takes.length) return;
    const inSel = app.lineSel.includes(it.line_id);
    if (!inSel) app.lineSel = [];
    menu = { lineId: it.line_id, ids: inSel ? [...app.lineSel] : [it.line_id], x: e.clientX, y: e.clientY };
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

  /** Pauses commit in 50 ms steps. */
  const snapGap = (g: number) => Math.round(g * 20) / 20;
  function gapLabel(g: number) {
    const gs = snapGap(g);
    return gs < 0 ? `overlap ${(-gs).toFixed(2)} s` : `‖ ${gs.toFixed(2)} s`;
  }

  function grabGap(e: MouseEvent, r: Row) {
    if (!r.next) return;
    const id = r.next.line_id;
    const g0 = r.gapAfter ?? 0;
    const y0 = e.clientY;
    track(
      e,
      // the rows follow the mouse freely; only the value snaps
      (ev) => (gapPreview = { id, g: clamp(g0 + (ev.clientY - y0) / (26 * sc), -1, 4) }),
      async () => {
        const g = gapPreview ? snapGap(gapPreview.g) : null;
        if (g == null || Math.abs(g - g0) < 0.001) {
          gapPreview = null;
          return;
        }
        // hold the released position until the new view lands, so the row doesn't jump back first
        gapPreview = { id, g };
        try {
          await app.cmd({ cmd: 'set_gap', line_id: id, gap_s: g });
        } finally {
          gapPreview = null;
        }
      },
    );
  }

  /** A direction's length: how much room it leaves for its sounds. Dragged like a pause. */
  function grabAction(e: MouseEvent, a: ItemView) {
    e.stopPropagation();
    const id = a.line_id;
    const s0 = a.spacer_s;
    const y0 = e.clientY;
    track(
      e,
      // the edge stays under the mouse
      (ev) => (actPreview = { id, s: clamp(s0 + (ev.clientY - y0) / (ACT_PX_PER_S * sc), 0.1, 30) }),
      async () => {
        const s = actPreview ? Math.round(actPreview.s * 20) / 20 : s0;
        if (Math.abs(s - s0) < 0.001) {
          actPreview = null;
          return;
        }
        // hold the released length until the new view lands, so the row doesn't jump back first
        actPreview = { id, s };
        try {
          await app.cmd({ cmd: 'set_spacer', spacer_id: id, enabled: true, duration_s: s });
        } finally {
          actPreview = null;
        }
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
        const t = takeOf(it);
        try {
          if (p && t) await app.cmd({ cmd: 'set_trim', line_id: it.line_id, head_s: +(p.a - t.src_in).toFixed(3), tail_s: +(t.src_out - p.z).toFixed(3) });
        } finally {
          trimPreview = null;
        }
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
      const at = timeAt(it, s, takeOf(it)?.source_id) ?? it.start_s + (s - tr.a);
      app.locator = at;
      if (!app.playing) app.pos = at;
    };
    set(e.clientX);
    track(e, (ev) => set(ev.clientX), () => {});
  }

  /** A line another can land next to: on the timeline, and not in a pair with a third. */
  function canTake(r: Row, id: string) {
    const after = rows[r.j + 1];
    return !r.it.beside && (!after?.it.beside || after.it.line_id === id) && r.it.pieces.length > 0 && !r.it.pinned;
  }

  /** The line under the mouse, when it's over the right half of its card. */
  function besideAt(ev: MouseEvent, id: string) {
    for (const r of rows) {
      if (r.it.line_id === id || !canTake(r, id)) continue;
      const box = rowEls[r.it.line_id]?.getBoundingClientRect();
      if (box && ev.clientY >= box.top && ev.clientY <= box.bottom && ev.clientX >= box.left + box.width / 2 && ev.clientX <= box.right + 40) return r.it.line_id;
    }
    return null;
  }

  function grabRow(e: MouseEvent, it: ItemView) {
    const x0 = e.clientX;
    const y0 = e.clientY;
    drag = { id: it.line_id, dx: 0, dy: 0, y0 };
    track(
      e,
      (ev) => {
        if (drag) drag = { ...drag, dx: ev.clientX - x0, dy: ev.clientY - y0 };
        besideOn = besideAt(ev, it.line_id);
      },
      async () => {
        const d = drag;
        const onto = besideOn;
        drag = null;
        besideOn = null;
        if (onto && d) {
          await app.cmd({ cmd: 'set_beside', line_id: d.id, beside: onto, offset_s: 0 });
          return;
        }
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
        // pulled out of its pair and put back in the same place: the two go one after the other
        else if (it.beside && Math.hypot(d.dx, d.dy) > PULL_PX) await app.cmd({ cmd: 'set_beside', line_id: d.id, beside: null });
      },
    );
  }

  /** Half a column is too narrow for the words beside the name: they go on a line of their own. */
  const PAIRED_EXTRA = 22;
  /** How far the second of a pair sits from the first. The card follows the mouse. */
  const PAIR_PX_PER_S = 26;
  /** A drag shorter than this was only a slip of the hand. */
  const PULL_PX = 30;
  const offsetOf = (r: Row) => (offPreview?.id === r.it.line_id ? offPreview.o : (r.it.beside?.offset_s ?? 0));
  const shiftPx = (o: number) => Math.round(Math.abs(o) * PAIR_PX_PER_S * sc);
  const offsetLabel = (o: number) => (Math.abs(o) < 0.005 ? 'together' : `${o < 0 ? '−' : '+'}${Math.abs(o).toFixed(2)} s`);

  function grabOffset(e: MouseEvent, r: Row) {
    const b = r.it.beside;
    if (!b) return;
    const id = r.it.line_id;
    const y0 = e.clientY;
    track(
      e,
      (ev) => (offPreview = { id, o: clamp(b.offset_s + (ev.clientY - y0) / (PAIR_PX_PER_S * sc), -30, 30) }),
      async () => {
        const o = offPreview ? snapGap(offPreview.o) : null;
        if (o == null || Math.abs(o - b.offset_s) < 0.001) {
          offPreview = null;
          return;
        }
        // hold the released place until the new view lands, so the card doesn't jump back first
        offPreview = { id, o };
        try {
          await app.cmd({ cmd: 'set_beside', line_id: id, beside: b.line_id, offset_s: o });
        } finally {
          offPreview = null;
        }
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
    if (!drag || besideOn) return null;
    const t = dropIndex(drag);
    const others = rows.filter((r) => r.it.line_id !== drag!.id);
    if (t == null) return null;
    const target = others[t];
    if (target) return (rowEls[target.it.line_id]?.offsetTop ?? 0) - 6;
    const last = others[others.length - 1];
    return last ? (rowEls[last.it.line_id]?.offsetTop ?? 0) + S + 6 : 0;
  });

  function rowStyle(r: Row, prevGap: number, shift = 0, paired = false) {
    const shd = itemShade(app.view, r.it);
    const isCur = r.it.line_id === app.curId;
    const dragging = drag?.id === r.it.line_id;
    const ov = prevGap < 0 ? Math.round(-prevGap * 44 * sc) : 0;
    return `height: ${S + (paired ? PAIRED_EXTRA : 0)}px; margin-top: ${shift - ov}px; z-index: ${dragging ? 30 : 2}; transform: translate(${dragging ? drag!.dx.toFixed(1) : 0}px, ${dragging ? drag!.dy.toFixed(1) : 0}px)${dragging ? ' rotate(-1deg)' : ''}; transition: ${dragging ? 'none' : 'transform .2s'}; mix-blend-mode: ${ov > 0 ? 'screen' : 'normal'}; background: linear-gradient(90deg, ${rgba(shd, isCur ? 0.44 : 0.28)}, ${rgba(shd, 0.1)} 60%, rgb(var(--veil-rgb) / 0.03)); box-shadow: inset 0 0 0 ${isCur ? 1.5 : 1}px ${rgba(shd, isCur ? 0.75 : 0.2)}${dragging ? ', 0 30px 60px rgb(var(--shadow-rgb) / 0.55)' : isCur ? `, 0 16px 60px ${rgba(shd, 0.22)}` : ''}`;
  }
</script>

{#snippet card(r: Row, prevGap: number, shift: number, paired: boolean)}
    {@const t = takeOf(r.it)}
    {@const tr = trims(r.it)}
    {@const hc = inWord(r.it, tr.a)}
    {@const tc = inWord(r.it, tr.z)}
    {@const h = fx(r.it, tr.a)}
    {@const tl = fx(r.it, tr.z)}
    {@const shd = itemShade(app.view, r.it)}
    <div class="snip" class:clip-muted={r.it.muted} class:clip-dropped={r.it.dropped} class:paired class:slotted={besideOn === r.it.line_id} class:picked={app.lineSel.includes(r.it.line_id) || menu?.ids.includes(r.it.line_id)} bind:this={rowEls[r.it.line_id]} style={rowStyle(r, prevGap, shift, paired)} onclick={(e) => pick(e, r.it)} oncontextmenu={(e) => openMenu(e, r.it)} ondblclick={() => { app.select(r.it.line_id); openEditor(); }} role="button" tabindex="-1" onkeydown={() => {}} data-testid="arrange-row">
      <div class="top">
        <span class="grip" data-tour="grip" onmousedown={(e) => grabRow(e, r.it)} title="Drag to reorder" aria-label="Drag to reorder" role="button" tabindex="-1">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><circle cx="9" cy="6" r="1.6" /><circle cx="15" cy="6" r="1.6" /><circle cx="9" cy="12" r="1.6" /><circle cx="15" cy="12" r="1.6" /><circle cx="9" cy="18" r="1.6" /><circle cx="15" cy="18" r="1.6" /></svg>
        </span>
        <span class="nm who" style="color: {charColor(app.view, r.it.character).name}">{charLabel(app.view, r.it.character)}</span>
        <span class="script ellip txt" style="font-size: {Math.round(15 * sc)}px">{#if r.it.words.length && !r.it.dropped}{#each lineWords(r.it).filter((w) => !w.deleted) as w, wi (w.j)}{wi ? ' ' : ''}<span class:wmuted={w.muted}>{w.text}</span>{/each}{:else}{r.it.text}{/if}</span>
        <ClipState it={r.it} />
        {#if r.it.moved}<span class="badge moved">Moved</span>{/if}
        <NoteBadge line={r.it.line_id} />
        {#if app.depthOf(r.it) > 0}<span class="badge back tn" title="Further back in the room. Right click to change it." data-testid="depth-chip">{depthText(app.depthOf(r.it))}</span>{/if}
        {#if !r.it.beside}<span class="dur" data-testid="row-dur">{(r.it.end_s - r.it.start_s).toFixed(1)} s</span>{/if}
        {#if r.it.beside}
          <span class="offp tn" role="slider" tabindex="-1" aria-label="Offset from the line beside it" aria-valuenow={offsetOf(r)} title="Drag up or down to start it earlier or later" onmousedown={(e) => grabOffset(e, r)} onclick={(e) => e.stopPropagation()} onkeydown={() => {}} data-testid="pair-offset">{offsetLabel(offsetOf(r))}</span>
          <button class="unpair" aria-label="Back in line, after the other" title="Back in line, after the other" onmousedown={(e) => e.stopPropagation()} onclick={(e) => { e.stopPropagation(); void app.cmd({ cmd: 'set_beside', line_id: r.it.line_id, beside: null }); }} data-testid="pair-undo"><Icon name="x" size={11} /></button>
        {/if}
      </div>
      <div class="strip" data-tour="trim" onmousedown={(e) => scrub(e, r.it)} onclick={(e) => e.stopPropagation()} onkeydown={() => {}} title="Click to set the locator, I / O to set in and out" style="height: {Math.round(30 * sc)}px" role="slider" tabindex="-1" aria-valuenow={0}>
        {#if t}
          <Waveform peaks={peaksOf(r.it)} fill={rgba(shd, 0.85 - 0.45 * app.depthOf(r.it))} />
          <div class="shade" style="left: 0; width: {h * 100}%"></div>
          <div class="shade" style="right: 0; width: {(1 - tl) * 100}%"></div>
          {#each [fx(r.it, t.src_in), fx(r.it, t.src_out)] as e, k (k)}{#if e > 0.001 && e < 0.999}<span class="tedge" style="left: {e * 100}%" title="Where the take was cut"></span>{/if}{/each}
          {#if r.it.line_id === app.curId}
            {@const lf = atTime(r.it, app.locator)}
            {@const pf = atTime(r.it, app.pos)}
            <span class="vline" style="background: var(--locator); left: {lf * 100}%"></span><span class="flag" style="left: {lf * 100}%"></span>
            {#if app.pos > r.it.start_s && app.pos <= r.it.end_s}<span class="vline phead" style="left: {pf * 100}%"></span>{/if}
          {/if}
          {#if app.soundCue && speaks(r.it, app.soundCue.t)}
            <span class="cue" style="left: {atTime(r.it, app.soundCue.t) * 100}%; --tone: var({app.soundCue.tone})" data-testid="sound-cue"></span>
          {/if}
          <span class="hdl" role="slider" tabindex="-1" aria-label="Clip start" aria-valuenow={h} onmousedown={(e) => grabTrim(e, r.it, 'h')} title="Drag to trim the head, or out to lengthen it" style="left: calc({h * 100}% - 5px); background: {hc ? 'var(--warn)' : 'rgb(var(--ink-rgb) / 0.9)'}"></span>
          <span class="hdl" role="slider" tabindex="-1" aria-label="Clip end" aria-valuenow={tl} onmousedown={(e) => grabTrim(e, r.it, 't')} title="Drag to trim the tail, or out to lengthen it" style="left: calc({tl * 100}% - 5px); background: {tc ? 'var(--warn)' : 'rgb(var(--ink-rgb) / 0.9)'}"></span>
        {:else}
          <span class="none">no takes</span>
        {/if}
      </div>
    </div>
{/snippet}

{#snippet action(a: ItemView)}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions (the playhead's keys reach it too) -->
  <div class="act" class:over={dropOn === a.line_id} class:off={!a.spacer_enabled} style="padding-bottom: {5 + actRoom(a)}px" bind:this={actEls[a.line_id]} use:dropzone={actionZone(a)} onclick={() => on && app.scrubTo(a.start_s)} data-testid="action-row" title={on ? 'Drop a sound here to tag it to this action' : undefined}>
    <span class="script atext">{a.text}</span>
    {#each tagsOf(a) as p (p.id)}<span class="tag ellip" data-testid="action-tag">{p.name}</span>{/each}
    <span class="aedge" role="slider" tabindex="-1" aria-label="Length of this direction" aria-valuenow={actLen(a)} title="Drag to change how long this direction takes" onmousedown={(e) => grabAction(e, a)} onclick={(e) => e.stopPropagation()} onkeydown={() => {}} data-testid="action-len"><span class="asec tn">{actLen(a).toFixed(1)} s</span></span>
  </div>
{/snippet}

<div class="col" class:masked={on} bind:clientWidth={stageW}>
<div class="arr" style="left: {on ? COLUMN_LEFT : (stageW - plainW) / 2}px; width: {on ? columnW : plainW}px; transform: translateY({(-off).toFixed(1)}px); transition: {slide}{moving ? ', left var(--dur-move) var(--ease-move), width var(--dur-move) var(--ease-move)' : ''}">
  {#each head as a (a.line_id)}{@render action(a)}{/each}
  {#each groups as grp, i (grp[0].it.line_id)}
    {@const r = grp[grp.length - 1]}
    {@const before = i > 0 ? groups[i - 1] : null}
    {@const prevGap = before && !before.some((x) => x.acts.length) ? gapOf(before[before.length - 1]) : 0}
    {#if grp.length > 1}
      {@const o = offsetOf(grp[1])}
      <div class="pair" bind:this={pairEls[grp[0].it.line_id]} data-testid="arrange-pair">
        {@render card(grp[0], prevGap, o < 0 ? shiftPx(o) : 0, true)}
        {@render card(grp[1], prevGap, o > 0 ? shiftPx(o) : 0, true)}
      </div>
    {:else}
      {@render card(r, prevGap, 0, false)}
    {/if}
    {#if r.next}
      {@const g = gapOf(r)}
      <div class="gaprow" data-tour="gap" onmousedown={(e) => grabGap(e, r)} title="Drag up or down to change the pause" style="height: {Math.round((18 + Math.max(g, 0) * 14) * sc)}px" role="slider" tabindex="-1" aria-valuenow={g} data-testid="gap-row">
        <div class="gapline"></div>
        <div class="gappill" style="color: {g < 0 ? 'var(--overlap)' : 'rgb(var(--ink-rgb) / 0.78)'}">
          <span class="num">{gapLabel(g)}</span>
        </div>
      </div>
      {#each grp.flatMap((x) => x.acts) as a (a.line_id)}{@render action(a)}{/each}
    {/if}
  {/each}
  {#each tail as a (a.line_id)}{@render action(a)}{/each}
  {#if dropY != null}<div class="drop" style="top: {dropY}px"></div>{/if}
  {#if cueGapY != null && app.soundCue}<div class="cuegap" style="top: {cueGapY}px; --tone: var({app.soundCue.tone})" data-testid="sound-cue-gap"></div>{/if}
</div>
</div>
{#if !on}<div class="side tchip" style="left: calc(50% - {Math.round(300 * sc)}px - 24px)">{fmt(app.pos)}</div>{/if}
<SoundsLayer {map} {spineX} {off} {slide} {on} lines={app.lines} actions={actionRows} />
{#if menu}<LineMenu {...menu} onclose={() => (menu = null)} />{/if}

<style>
  .col {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  /* beside the sounds the column sits under the header's left end and scrolls past the
     transport: it fades out before it reaches them */
  .col.masked {
    -webkit-mask-image: linear-gradient(to bottom, transparent 76px, #000 140px, #000 calc(100% - 110px), transparent calc(100% - 30px));
    mask-image: linear-gradient(to bottom, transparent 76px, #000 140px, #000 calc(100% - 110px), transparent calc(100% - 30px));
  }
  .arr {
    position: absolute;
    left: 50%;
    top: 50%;
    pointer-events: auto;
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
  /* two lines side by side: half a column each, the second can sit lower or higher */
  .pair {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }
  .pair .snip {
    flex: 1 1 0;
    min-width: 0;
    padding: 8px 10px;
  }
  .paired .top {
    flex-wrap: wrap;
    gap: 0 6px;
  }
  .paired .who {
    flex: 0 1 auto;
    width: auto;
    min-width: 0;
  }
  .paired .txt {
    flex: 1 0 100%;
    order: 5;
  }
  .paired .dur {
    margin-left: auto;
  }
  .offp {
    flex: none;
    margin-left: auto;
    padding: 1px 6px;
    border-radius: var(--radius-pill);
    font-size: 10.5px;
    color: var(--overlap);
    background: rgb(var(--veil-rgb) / 0.08);
    cursor: ns-resize;
  }
  .unpair {
    flex: none;
    width: 18px;
    height: 18px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 5px;
    color: rgb(var(--ink-rgb) / 0.5);
  }
  .unpair:hover {
    background: rgb(var(--veil-rgb) / 0.1);
    color: var(--text-strong);
  }
  /* where a dragged line would land beside this one */
  .slotted::after {
    content: 'Side by side';
    position: absolute;
    top: 6px;
    right: 6px;
    bottom: 6px;
    width: calc(50% - 6px);
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1.5px dashed var(--playhead);
    border-radius: 10px;
    background: rgb(var(--veil-rgb) / 0.08);
    font-size: var(--fs-small);
    color: var(--text-strong);
    pointer-events: none;
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
  .back {
    color: rgb(var(--ink-rgb) / 0.72);
    box-shadow: inset 0 0 0 1px rgb(var(--ink-rgb) / 0.22);
  }
  .picked {
    outline: 1.5px solid var(--playhead);
    outline-offset: 2px;
  }
  /* where the moved sound sits: its own colour, so it can't be taken for the locator */
  .cue {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 3px;
    margin-left: -1.5px;
    background: rgb(var(--tone));
    box-shadow: 0 0 0 1px rgb(var(--shadow-rgb) / 0.5), 0 0 8px rgb(var(--tone) / 0.8);
    pointer-events: none;
    z-index: 2;
  }
  .cue::before {
    content: '';
    position: absolute;
    top: 0;
    left: -4px;
    border: 5px solid transparent;
    border-top-color: rgb(var(--tone));
  }
  .cuegap {
    position: absolute;
    left: 0;
    right: 0;
    height: 2px;
    margin-top: -1px;
    border-radius: 1px;
    background: rgb(var(--tone));
    box-shadow: 0 0 8px rgb(var(--tone) / 0.8);
    pointer-events: none;
    z-index: 7;
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
  /* the script's stage directions, between the cards: what a sound gets tagged to */
  .act {
    position: relative;
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 8px;
    margin: 4px 14px;
    box-sizing: border-box;
    padding: 5px 48px 5px 10px;
    border-radius: 8px;
    border-left: 2px solid rgb(var(--ink-rgb) / 0.22);
    color: var(--text-3);
    transition:
      background var(--dur-fast),
      color var(--dur-fast);
  }
  .act.off {
    opacity: 0.6;
  }
  /* the bottom edge: drag it for a longer or shorter direction */
  .aedge {
    position: absolute;
    left: 0;
    right: 0;
    bottom: -4px;
    height: 12px;
    cursor: ns-resize;
  }
  .asec {
    position: absolute;
    right: 8px;
    bottom: 6px;
    padding: 0 6px;
    border-radius: var(--radius-pill);
    font-size: 10.5px;
    color: var(--text-4);
    transition: color var(--dur-fast);
  }
  .act:hover .asec {
    color: var(--text-strong);
  }
  .act.over {
    color: var(--text-strong);
    background: rgb(var(--sfx-rgb) / 0.14);
    border-left-color: var(--sfx);
  }
  .atext {
    flex: 1 1 100%;
    font-size: var(--fs-small);
    line-height: 1.4;
  }
  .tag {
    max-width: 100%;
    padding: 1px 8px;
    border-radius: var(--radius-pill);
    font-size: 10.5px;
    color: var(--on-sfx);
    background: var(--sfx);
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
  @media (prefers-reduced-motion: reduce) {
    .arr {
      transition: none !important;
    }
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
