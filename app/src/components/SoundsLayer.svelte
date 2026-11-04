<script lang="ts">
  // Arrange's sounds layer: the spine, the length lines, the balls and the floating bucket.
  // A ball is the same sound clip the Edit page shows: everything here reads `app.edit`, and
  // every change is a command sent once, on release. What a gesture shows until then is a
  // local preview. The rules live in lib/sounds; this file draws them and wires up gestures.
  import { onMount, tick, untrack } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import { charColor, rgba } from '../lib/colors';
  import { dropzone } from '../lib/drag.svelte';
  import { clamp, fmt } from '../lib/format';
  import { depthAt, groups, layoutBalls, type Field, type Shown } from '../lib/sounds/balls';
  import { soundKey } from '../lib/sounds/keymap';
  import { assignLanes, type Lanes, type Settled } from '../lib/sounds/lanes';
  import { partAt, partText, reshape, reshapeCommands, type OpenState, type Ops, type Part } from '../lib/sounds/opened';
  import { arc, circle, focusBody, focusFade, linePath, panLine, restOfSource, ringPoint, spinePaths } from '../lib/sounds/paths';
  import { announce, autoFadeOut, ballsInBand, bandSelect, clickSelect, dbOfRing, dbText, depthCommands, GAIN_MAX_DB, GAIN_MIN_DB, gainCommands, groupDelta, moveCommands, normRect, oneStep, panCommands, panText, pressSelect, removeCommands, ringAt, ringOf, timeDrag, toggleHidden, zone, type Cmd, type Rect, type SoundState } from '../lib/sounds/select';
  import { settledOf, Settler, snapshot, withFresh } from '../lib/sounds/settle';
  import { actionTargets, nearestTarget, snapTargets, startLabel, type ActionRow, type SnapTarget } from '../lib/sounds/snap';
  import type { TimeMap } from '../lib/sounds/timemap';
  import type { Bucket, BucketsView, DragSound, ItemView, Job, PlacementView } from '../lib/types';
  import OpenedSound from './OpenedSound.svelte';
  import SceneBucket from './SceneBucket.svelte';

  let { map, spineX, off, slide, on, lines, actions = [] }: { actions?: ActionRow[]; map: TimeMap; spineX: number; off: number; slide: string; on: boolean; lines: ItemView[] } = $props();

  const OTHER = 'other';
  const TONES = 6;
  const DEADZONE_PX = 12;
  const PRESS_PX = 4;
  /** Keys and the wheel come in bursts: one command when they stop. */
  const BURST_MS = 350;
  const GROUP_LEAVE_MS = 420;
  const HOVER_LEAVE_MS = 90;
  const MIN_TRIM_S = 0.2;
  const SEEK_MS = 150;
  /** Where the opened view sits. */
  const CARD_GAP = 96;
  const CARD_W = 372;
  const CARD_MIN_W = 300;
  const CARD_TOP = 84;
  const CARD_BELOW = 124;

  type Sound = PlacementView & { bucket: string; bucketName: string; tone: string };
  type Patch = Partial<Pick<PlacementView, 'start_s' | 'len_s' | 'in_s' | 'depth' | 'pan' | 'gain_db' | 'fade_in_s' | 'fade_out_s' | 'auto_fade'>>;

  // ---- what the document says, with the gesture's preview on top ----

  let buckets = $state<BucketsView | null>(null);
  let preview = $state.raw<Record<string, Patch>>({});
  let stageW = $state(1440);
  let stageH = $state(900);
  let world = $state<HTMLDivElement>();
  let root = $state<HTMLDivElement>();

  async function loadBuckets() {
    if (!app.sceneId) return;
    buckets = (await app.run(() => call<BucketsView>('buckets', { scene: app.sceneId }))) ?? buckets;
  }
  $effect(() => {
    void app.sceneId;
    void app.status?.rev;
    if (on) void loadBuckets();
  });

  /** Vocal buckets are the actor's own and go on a dialogue lane: not here. */
  const placeable = $derived(buckets?.buckets.filter((b) => !b.character) ?? []);
  const toneOf = $derived(new Map(placeable.map((b, i) => [b.id, `--bucket-${(i % TONES) + 1}-rgb`])));
  /** A sound kept in several buckets counts for the first. */
  const bucketOfAsset = $derived(new Map(placeable.flatMap((b) => b.kept.map((k): [string, Bucket] => [k.id, b])).reverse()));

  const placed = $derived(app.edit?.scene_id === app.sceneId ? app.edit.placements.filter((p) => p.lane === 'sfx' || p.lane === 'ambience') : []);
  const sounds = $derived(
    placed.map((p): Sound => {
      const b = bucketOfAsset.get(p.asset_id);
      return { ...p, ...preview[p.id], bucket: b?.id ?? OTHER, bucketName: b?.name ?? 'Other sounds', tone: (b && toneOf.get(b.id)) ?? '--bucket-none-rgb' };
    }),
  );
  const byId = $derived(new Map(sounds.map((s) => [s.id, s])));
  const hidden = $derived(new Set(app.hiddenBuckets));
  const isHidden = (s: Sound) => hidden.has(s.bucket);
  const sel = $derived(app.soundSel);
  const selSet = $derived(new Set(sel));

  // what is selected must exist and be visible (handover 5.7)
  $effect(() => {
    const ok = sel.filter((id) => {
      const s = byId.get(id);
      return !!s && !isHidden(s);
    });
    if (ok.length !== sel.length) app.soundSel = ok;
  });

  // ---- layout ----

  const panelShown = $derived(app.editBucket && stageW > 1100);
  const rightEdge = $derived(stageW - (panelShown ? 400 : 28));
  const field = $derived<Field>({ x0: spineX + 126, width: Math.max(120, rightEdge - spineX - 214), y: (t) => map.y(t) });
  const laneX = (l: number) => spineX + 22 + l * 5;
  const targets = $derived([...snapTargets(lines), ...actionTargets(actions, map)].sort((a, b) => a.t - b.t));
  const spine = $derived([...spinePaths(lines, map, spineX)].map(([who, d]) => ({ who, d, fill: rgba(charColor(app.view, who).rgb, 0.95) })));
  const sceneEnd = $derived(app.view?.duration_s ?? 0);

  // ---- settling: lanes and groups read a snapshot, not the live values ----

  let snap = $state.raw(new Map<string, Settled>());
  let lanes = $state.raw<Lanes>(new Map());
  const settled = $derived(sounds.map((s) => ({ s, ...settledOf(s, snap) })));
  const liveLanes = $derived(
    assignLanes(
      settled.map((x) => x.z),
      lanes,
      snap,
    ),
  );

  function settle() {
    const next = snapshot(sounds);
    lanes = assignLanes([...next.values()], liveLanes, withFresh(snap, settled));
    snap = next;
  }
  const settler = new Settler(settle);

  // the sounds changed (a release, undo, the Edit page, the CLI)
  $effect(() => {
    void placed;
    untrack(() => (snap.size ? settler.soon() : settle()));
  });
  $effect(() => {
    void app.sceneId;
    untrack(() => {
      snap = new Map();
      lanes = new Map();
    });
  });

  const grouped = $derived(
    groups(
      settled.filter((x) => !x.fresh && !isHidden(x.s)).map((x) => x.z),
      field,
    ),
  );

  // ---- hover, open groups, where each ball shows ----

  let hover = $state<string | null>(null);
  let openGroup = $state<string | null>(null);
  let dragging = $state<string[]>([]);
  const dragSet = $derived(new Set(dragging));
  let hoverTimer: ReturnType<typeof setTimeout> | undefined;
  let groupTimer: ReturnType<typeof setTimeout> | undefined;

  const disp = $derived(layoutBalls(sounds, grouped, field, (g) => g.key === openGroup || g.members.some((x) => selSet.has(x.id) || x.id === hover || dragSet.has(x.id)), dragSet));
  const shown = (id: string): Shown => disp.get(id) ?? { x: 0, y: 0, r: 0, delay: 0 };

  /** While a group is open the pointer may wander between its balls. */
  const openArea = $derived.by(() => {
    const g = grouped.find((x) => x.key === openGroup);
    if (!g) return null;
    const ps = g.members.map((x) => shown(x.id));
    const x0 = Math.min(...ps.map((p) => p.x - p.r)) - 22;
    const y0 = Math.min(...ps.map((p) => p.y - p.r)) - 22;
    return { key: g.key, x0, y0, w: Math.max(...ps.map((p) => p.x + p.r)) + 22 - x0, h: Math.max(...ps.map((p) => p.y + p.r)) + 38 - y0 };
  });

  function keepGroup(key: string) {
    clearTimeout(groupTimer);
    if (!dragging.length) openGroup = key;
  }
  function leaveGroup() {
    clearTimeout(groupTimer);
    groupTimer = setTimeout(() => {
      if (!dragging.length) openGroup = null;
    }, GROUP_LEAVE_MS);
  }
  const groupOf = (id: string) => grouped.find((g) => g.members.some((m) => m.id === id));

  function hoverIn(id: string) {
    clearTimeout(hoverTimer);
    if (dragging.length) return;
    hover = id;
    const g = groupOf(id);
    if (g) keepGroup(g.key);
  }
  function hoverOut(id: string) {
    clearTimeout(hoverTimer);
    if (groupOf(id)) leaveGroup();
    hoverTimer = setTimeout(() => {
      if (!dragging.length && hover === id) hover = null;
    }, HOVER_LEAVE_MS);
  }

  // ---- what is drawn ----

  const primary = $derived(sel.length ? (byId.get(sel[sel.length - 1]) ?? null) : null);
  const openId = $derived(app.soundOpen);
  const opened = $derived(openId ? (byId.get(openId) ?? null) : null);
  const focusId = $derived(openId ?? dragging[0] ?? hover ?? (sel.length === 1 ? sel[0] : null));

  const lineEnd = (s: Sound) => Math.max(map.y(s.start_s + s.len_s), map.y(s.start_s) + 4);

  /** One path per bucket, and one for all the connecting lines. */
  const drawn = $derived.by(() => {
    const by: Record<string, { bucket: string; tone: string; faded: boolean; d: string }> = {};
    let tethers = '';
    for (const s of sounds) {
      const x = laneX(liveLanes.get(s.id) ?? 0);
      const y0 = map.y(s.start_s);
      const faded = isHidden(s);
      by[s.bucket] ??= { bucket: s.bucket, tone: s.tone, faded, d: '' };
      by[s.bucket].d += linePath(x, y0, lineEnd(s));
      if (faded || s.id === focusId) continue;
      const b = shown(s.id);
      tethers += `M${x + 3} ${y0.toFixed(1)}L${(b.x - b.r * (b.tucked ? 0.5 : 1)).toFixed(1)} ${b.y.toFixed(1)}`;
    }
    return { lines: Object.values(by), tethers };
  });

  const focus = $derived.by(() => {
    const s = focusId ? byId.get(focusId) : null;
    if (!s || isHidden(s) || !on) return null;
    const x = laneX(liveLanes.get(s.id) ?? 0);
    const y0 = map.y(s.start_s);
    const y1 = lineEnd(s);
    const yf = clamp(map.y(s.start_s + s.len_s - s.fade_out_s), y0 + 2, y1);
    const b = shown(s.id);
    const restEnd = s.asset_len_s > 0 && !s.looped ? map.y(s.start_s - s.in_s + s.asset_len_s) : y1;
    return { s, x, y0, y1, body: focusBody(x, y0, yf), fade: focusFade(x, yf, y1), rest: restOfSource(x, y1, restEnd), tether: `M${x + 4} ${y0.toFixed(1)}L${(b.x - b.r * (b.tucked ? 0.5 : 1)).toFixed(1)} ${b.y.toFixed(1)}` };
  });

  const ring = $derived.by(() => {
    const s = primary;
    if (!s || !on || isHidden(s)) return null;
    const b = shown(s.id);
    if (b.tucked) return null;
    const R = b.r + 13;
    const g = ringOf(s.gain_db);
    const k = ringPoint(b.x, b.y, R, g);
    return { x: b.x, y: b.y, R, track: circle(b.x, b.y, R), arc: arc(b.x, b.y, R, g), kx: k.x, ky: k.y, text: `Loudness ${dbText(s.gain_db)}`, many: sel.length };
  });

  const playing = (s: Sound) => app.playing && !app.audition && app.pos >= s.start_s && app.pos < s.start_s + s.len_s;

  // ---- Sounds toggle: the one playful moment ----

  let staggering = $state(false);
  let staggerTimer: ReturnType<typeof setTimeout> | undefined;
  let wasOn: boolean | null = null;
  $effect(() => {
    const now = on;
    if (wasOn !== null && wasOn !== now) {
      staggering = true;
      clearTimeout(staggerTimer);
      staggerTimer = setTimeout(() => (staggering = false), 1100);
      untrack(() => {
        hover = null;
        tip = null;
      });
    }
    wasOn = now;
  });
  // a ripple, not a parade: however many sounds there are, it's over in a moment
  const staggerOf = (i: number, n: number) => {
    const step = Math.min(0.02, 0.24 / Math.max(1, n));
    return on ? 0.08 + i * step : (n - 1 - i) * step * 0.5;
  };
  const inTimeOrder = $derived([...sounds].sort((a, b) => a.start_s - b.start_s).map((s) => s.id));

  // ---- gestures ----

  interface Gesture {
    /** `alt`: ⌥ is down and the direction hasn't said yet: sideways pans, up or down moves. */
    kind: 'ball' | 'head' | 'trim' | 'pan' | 'ring' | 'alt';
    id: string;
    base: SoundState[];
    x0: number;
    y0: number;
    /** How far below the start of the sound it was grabbed. */
    grab: number;
    moved: boolean;
    freeY: boolean;
    depth: number;
    time: number;
    target: SnapTarget | null;
    pan: number;
    gain: number;
    ring: number;
    len: number;
    size: number;
  }

  let gesture: Gesture | null = null;
  let tip = $state<{ x: number; y: number; text: string } | null>(null);
  let snapY = $state<number | null>(null);
  let band = $state<(Rect & { hit: string[] }) | null>(null);
  let popping = $state<string[]>([]);

  function stateOf(id: string): OpenState {
    const s = byId.get(id);
    if (!s) throw new Error(`no sound ${id}`);
    return { id, start_s: s.start_s, len_s: s.len_s, offset_s: s.offset_s, anchor: s.anchor, depth: s.depth, pan: s.pan, gain_db: s.gain_db, in_s: s.in_s, asset_len_s: s.asset_len_s, looped: s.looped, fade_in_s: s.fade_in_s, fade_out_s: s.fade_out_s, auto_fade: s.auto_fade };
  }

  function point(e: Pick<MouseEvent, 'clientX' | 'clientY'>) {
    const r = root?.getBoundingClientRect();
    const w = world?.getBoundingClientRect();
    return { x: e.clientX - (r?.left ?? 0), y: e.clientY - (w?.top ?? 0) };
  }

  /** The value a delta gives each sound, as the preview shows it. */
  function patchAll<B extends { id: string }>(base: B[], f: (b: B) => Patch) {
    const next = { ...preview };
    for (const b of base) next[b.id] = { ...next[b.id], ...f(b) };
    preview = next;
  }

  function clearPreview(ids: string[]) {
    const next = { ...preview };
    for (const id of ids) delete next[id];
    preview = next;
  }

  /** Commands on their way, per sound. */
  const landing: Record<string, number> = {};

  /** A preview lives while anything still needs it: a gesture, a burst or a command on its
   *  way. Whatever finishes calls this, and the last one out clears it, so a preview can
   *  neither outlive its owners nor go before the document has the value. */
  function letGo(ids: string[]) {
    const held = new Set([...(gesture?.base ?? []), ...(burst?.base ?? []), ...(shaping ? [{ id: shaping }] : [])].map((b) => b.id));
    clearPreview(ids.filter((id) => !held.has(id) && !landing[id]));
  }

  /** Sends what the gesture did as one undo step. The preview holds until it has landed, so
   *  nothing jumps back in between. */
  async function commit(cmds: Cmd[], ids: string[]) {
    const c = oneStep(cmds);
    for (const id of ids) landing[id] = (landing[id] ?? 0) + 1;
    try {
      if (c) await app.cmd(c);
    } finally {
      for (const id of ids) landing[id] -= 1;
      letGo(ids);
      settler.soon();
    }
  }

  function begin(e: MouseEvent, kind: Gesture['kind'], id: string, ids: string[], extra: Partial<Gesture> = {}) {
    e.preventDefault();
    e.stopPropagation();
    flushBurst();
    const p = point(e);
    const s = byId.get(id);
    if (!s) return;
    gesture = { kind, id, base: ids.map(stateOf), x0: p.x, y0: p.y, grab: p.y - map.y(s.start_s), moved: false, freeY: false, depth: 0, time: 0, target: null, pan: 0, gain: 0, ring: ringOf(s.gain_db), len: s.len_s, size: s.len_s, ...extra };
    settler.hold();
    if (kind !== 'ball') start();
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }

  function start() {
    if (!gesture) return;
    gesture.moved = true;
    // the grabbed one first: it is the one in focus
    const id = gesture.id;
    dragging = [id, ...gesture.base.map((b) => b.id).filter((x) => x !== id)];
  }

  /** What catches a sound here: nothing with snapping off, or while ⌥ is held. */
  const caught = (e: { altKey?: boolean }, t: number, y?: number) => (app.soundSnap && !e.altKey ? nearestTarget(targets, t, map, undefined, y) : null);

  function dragTime(g: Gesture, wanted: number, e: MouseEvent) {
    const t = Math.max(0, wanted);
    const r = timeDrag(g.base, g.id, t, caught(e, t));
    g.time = r.delta;
    g.target = r.target;
    snapY = r.target ? map.y(r.target.t) : null;
    return r;
  }

  function onMove(e: MouseEvent) {
    const g = gesture;
    if (!g) return;
    const p = point(e);
    const dx = p.x - g.x0;
    const dy = p.y - g.y0;
    const grabbed = g.base.find((b) => b.id === g.id);
    if (!grabbed) return;
    if (g.kind === 'alt') {
      if (Math.abs(dx) < PRESS_PX && Math.abs(dy) < PRESS_PX) return;
      g.kind = Math.abs(dx) >= Math.abs(dy) ? 'pan' : 'head';
    }
    if (g.kind === 'ball') {
      if (!g.moved) {
        if (Math.abs(dx) < PRESS_PX && Math.abs(dy) < PRESS_PX) return;
        start();
      }
      g.depth = groupDelta(
        g.base.map((b) => b.depth),
        dx / field.width,
        0,
        1,
      );
      const words = [`${zone(grabbed.depth + g.depth)} · ${Math.round((grabbed.depth + g.depth) * 100)}%`];
      if (g.freeY || Math.abs(dy) > DEADZONE_PX) {
        g.freeY = true;
        dragTime(g, map.t(p.y - g.grab), e);
        words.push(startLabel(g.target, grabbed.start_s + g.time));
      }
      patchAll(g.base, (b) => ({ depth: b.depth + g.depth, start_s: b.start_s + g.time }));
      tip = { x: p.x + 30, y: p.y - 40, text: words.join(' · ') };
    } else if (g.kind === 'head') {
      dragTime(g, map.t(p.y - g.grab), e);
      patchAll(g.base, (b) => ({ start_s: b.start_s + g.time }));
      tip = { x: spineX + 80, y: p.y - 14, text: startLabel(g.target, grabbed.start_s + g.time) };
    } else if (g.kind === 'trim') {
      const s = byId.get(g.id);
      if (!s) return;
      const max = s.asset_len_s > 0 && !s.looped ? s.asset_len_s - s.in_s : Infinity;
      const t = map.t(p.y);
      const target = caught(e, t);
      g.len = clamp((target?.t ?? t) - grabbed.start_s, Math.min(MIN_TRIM_S, g.size), max);
      const onTarget = !!target && Math.abs(grabbed.start_s + g.len - target.t) < 0.001;
      snapY = onTarget ? map.y(target.t) : null;
      const fade = s.auto_fade ? autoFadeOut(g.len) : s.fade_out_s;
      patchAll(g.base, () => ({ len_s: g.len, fade_out_s: fade }));
      const full = g.len >= max - 0.01;
      const lab = full ? 'Full length · ' : onTarget ? `${target.end} · ` : '';
      tip = { x: spineX + 80, y: p.y + 10, text: `${lab}${g.len.toFixed(1)}${max < Infinity ? ` of ${max.toFixed(1)}` : ''} s · fades out ${fade.toFixed(1)} s` };
    } else if (g.kind === 'pan') {
      const want = grabbed.pan + dx / (shown(g.id).r * 3 + 30);
      // a soft catch at the centre
      const pan = Math.abs(want) < 0.05 ? 0 : want;
      g.pan = groupDelta(
        g.base.map((b) => b.pan),
        pan - grabbed.pan,
        -1,
        1,
      );
      patchAll(g.base, (b) => ({ pan: b.pan + g.pan }));
      tip = { x: p.x + 22, y: p.y - 36, text: panText(grabbed.pan + g.pan) };
    } else {
      const b = shown(g.id);
      g.ring = ringAt(p.x, p.y, b.x, b.y, g.ring);
      g.gain = groupDelta(
        g.base.map((x) => x.gain_db),
        dbOfRing(g.ring) - grabbed.gain_db,
        GAIN_MIN_DB,
        GAIN_MAX_DB,
      );
      patchAll(g.base, (x) => ({ gain_db: x.gain_db + g.gain }));
    }
  }

  function endGesture() {
    window.removeEventListener('mousemove', onMove);
    window.removeEventListener('mouseup', onUp);
    gesture = null;
    dragging = [];
    tip = null;
    snapY = null;
    settler.release();
  }

  function onUp() {
    const g = gesture;
    if (!g) return;
    endGesture();
    const ids = g.base.map((b) => b.id);
    if (!g.moved) {
      // a plain click on a member of a selection selects just it
      if (g.kind === 'ball') app.soundSel = [g.id];
      letGo(ids);
      return;
    }
    const cmds: Cmd[] = [];
    if (g.depth) cmds.push(...depthCommands(g.base, g.depth));
    if (g.time || g.target) cmds.push(...moveCommands(g.base, g.time, g.id, g.target));
    if (g.pan) cmds.push(...panCommands(g.base, g.pan));
    if (g.gain) cmds.push(...gainCommands(g.base, g.gain));
    const trimmed = g.len - g.size;
    if (g.kind === 'trim' && trimmed) cmds.push({ cmd: 'trim_placement', placement_id: g.id, edge: 'end', delta_s: +trimmed.toFixed(3) });
    void commit(cmds, ids);
  }

  /** Esc during a gesture: as if it never happened. */
  function cancelGesture() {
    const ids = gesture?.base.map((b) => b.id) ?? [];
    endGesture();
    letGo(ids);
  }

  function ballDown(e: MouseEvent, id: string) {
    if (e.button !== 0) return;
    if (e.shiftKey) {
      e.preventDefault();
      e.stopPropagation();
      app.soundSel = clickSelect(sel, id, true);
      return;
    }
    const ids = pressSelect(sel, id);
    app.soundSel = ids;
    begin(e, e.altKey ? 'alt' : 'ball', id, ids);
  }

  function lineDown(e: MouseEvent, s: Sound) {
    if (e.button !== 0) return;
    const y = point(e).y;
    const end = map.y(s.start_s + s.len_s);
    const nearTail = y > end - 8 && end - map.y(s.start_s) > 14;
    if (nearTail) trimDown(e, s);
    else headDown(e, s, y - map.y(s.start_s));
  }
  function headDown(e: MouseEvent, s: Sound, grab = 0) {
    const ids = pressSelect(sel, s.id);
    app.soundSel = ids;
    begin(e, 'head', s.id, ids, { grab });
  }
  function trimDown(e: MouseEvent, s: Sound) {
    app.soundSel = [s.id];
    begin(e, 'trim', s.id, [s.id]);
  }
  function ringDown(e: MouseEvent) {
    if (primary) begin(e, 'ring', primary.id, sel);
  }

  // ---- the band, on the empty space beside the spine ----

  function bandDown(e: MouseEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    const a = point(e);
    const additive = e.shiftKey;
    let moved = false;
    const shownBalls = () => new Map([...disp].filter(([id]) => !hiddenOf(id)));
    const move = (ev: MouseEvent) => {
      const b = point(ev);
      if (!moved && Math.hypot(b.x - a.x, b.y - a.y) < PRESS_PX) return;
      moved = true;
      const r = normRect(a.x, a.y, b.x, b.y);
      band = { ...r, hit: ballsInBand(shownBalls(), r) };
    };
    const up = () => {
      window.removeEventListener('mousemove', move);
      window.removeEventListener('mouseup', up);
      if (band) app.soundSel = bandSelect(sel, band.hit, additive);
      else if (!moved && !additive) app.soundSel = [];
      band = null;
    };
    window.addEventListener('mousemove', move);
    window.addEventListener('mouseup', up);
  }
  const hiddenOf = (id: string) => {
    const s = byId.get(id);
    return !s || isHidden(s);
  };

  // ---- keys and the wheel: bursts that end in one command ----

  type BurstKind = 'gain' | 'depth' | 'pan' | 'time' | Part;
  let burst: { kind: BurstKind; base: OpenState[]; delta: number; timer: ReturnType<typeof setTimeout> } | null = null;

  const isPart = (kind: BurstKind): kind is Part => kind !== 'gain' && kind !== 'depth' && kind !== 'pan' && kind !== 'time';
  /** A key press on the opened sound: its part at the delta. */
  const shaped = (kind: Part, b: OpenState, delta: number) => reshape(kind, b, partAt(kind, b) + delta);

  const limit = (kind: BurstKind, base: OpenState[], delta: number) => {
    if (isPart(kind)) return shaped(kind, base[0], delta).at - partAt(kind, base[0]);
    if (kind === 'gain') return groupDelta(base.map((b) => b.gain_db), delta, GAIN_MIN_DB, GAIN_MAX_DB);
    if (kind === 'depth') return groupDelta(base.map((b) => b.depth), delta, 0, 1);
    if (kind === 'pan') return groupDelta(base.map((b) => b.pan), delta, -1, 1);
    return groupDelta(base.map((b) => b.start_s), delta, 0, Infinity);
  };

  function say(kind: BurstKind, b: OpenState, delta: number) {
    if (isPart(kind)) return partText(kind, b, shaped(kind, b, delta));
    if (kind === 'gain') return `Loudness ${dbText(b.gain_db + delta)}`;
    if (kind === 'depth') return `${zone(b.depth + delta)} · ${Math.round((b.depth + delta) * 100)}%`;
    if (kind === 'pan') return panText(b.pan + delta);
    return `Starts at ${fmt(b.start_s + delta)}`;
  }

  function nudge(kind: BurstKind, delta: number, ids: string[]) {
    if (burst && (burst.kind !== kind || burst.base.map((b) => b.id).join() !== ids.join())) flushBurst();
    if (!burst) {
      settler.hold();
      burst = { kind, base: ids.map(stateOf), delta: 0, timer: setTimeout(flushBurst, BURST_MS) };
    }
    const b = burst;
    clearTimeout(b.timer);
    b.delta = limit(kind, b.base, b.delta + delta);
    b.timer = setTimeout(flushBurst, BURST_MS);
    const d = b.delta;
    patchAll(b.base, (x) => (isPart(kind) ? shaped(kind, x, d).patch : kind === 'gain' ? { gain_db: x.gain_db + d } : kind === 'depth' ? { depth: x.depth + d } : kind === 'pan' ? { pan: x.pan + d } : { start_s: x.start_s + d }));
    // the opened view shows its own values as they change
    if (isPart(kind)) return;
    const lead = b.base[b.base.length - 1];
    const at = shown(lead.id);
    tip = { x: at.x + at.r + 12, y: at.y - 12, text: burstText(b, d) };
  }

  const burstText = (b: NonNullable<typeof burst>, d: number) => {
    const text = say(b.kind, b.base[b.base.length - 1], d);
    return b.base.length > 1 ? `${b.base.length} sounds · ${text}` : text;
  };

  function flushBurst() {
    const b = burst;
    if (!b) return;
    burst = null;
    clearTimeout(b.timer);
    tip = null;
    settler.release();
    const cmds = !b.delta ? [] : isPart(b.kind) ? reshapeCommands(b.kind, b.base[0], shaped(b.kind, b.base[0], b.delta), true) : b.kind === 'gain' ? gainCommands(b.base, b.delta) : b.kind === 'depth' ? depthCommands(b.base, b.delta) : b.kind === 'pan' ? panCommands(b.base, b.delta) : moveCommands(b.base, b.delta, null, null);
    if (cmds.length) speak(burstText(b, b.delta));
    void commit(cmds, b.base.map((x) => x.id));
  }

  function focusedBall(): string | null {
    const el = document.activeElement as HTMLElement | null;
    return el?.dataset.sound ?? null;
  }

  /** ⇧ Space with sounds selected: the timeline from the first of them, not the locator. */
  function playFromSound(ids: string[]) {
    const starts = ids.flatMap((id) => byId.get(id)?.start_s ?? []);
    if (!starts.length) return false;
    const t = Math.min(...starts);
    app.scrubTo(t);
    void app.play(t);
    return true;
  }

  function onKey(e: KeyboardEvent): boolean {
    // undo waits for what is on its way, so what is still in a burst goes first
    if ((e.metaKey || e.ctrlKey) && (e.key === 'z' || e.key === 'Z')) {
      flushBurst();
      return false;
    }
    if (!on) return false;
    if (gesture) {
      if (e.key !== 'Escape') return false;
      cancelGesture();
      return true;
    }
    const at = focusedBall();
    if (e.key === ' ' && e.shiftKey) return playFromSound(sel.length ? sel : at ? [at] : []);
    if (panel?.key(e)) return true;
    if ((e.metaKey || e.ctrlKey) && (e.key === 'a' || e.key === 'A')) {
      app.soundSel = inTimeOrder.filter((id) => !hiddenOf(id));
      return true;
    }
    if (e.key === 'Tab' && at) return tabTo(at, e.shiftKey ? -1 : 1);
    if (e.key === 'Enter' && !e.metaKey && !e.ctrlKey) {
      if (e.shiftKey && at) {
        app.soundSel = clickSelect(sel, at, true);
        return true;
      }
      const id = at ?? sel[sel.length - 1];
      if (id && !e.shiftKey) {
        openSound(id);
        return true;
      }
    }
    const ids = sel.length ? sel : at ? [at] : [];
    if (!ids.length) return false;
    const a = soundKey(e);
    if (!a) return false;
    if (a.kind === 'clear') app.soundSel = [];
    else if (a.kind === 'remove') {
      flushBurst();
      speak(ids.length > 1 ? `${ids.length} sounds removed` : `${byId.get(ids[0])?.name ?? 'Sound'} removed`);
      app.soundSel = [];
      void commit(removeCommands(ids), []);
    } else nudge(a.kind, a.delta, ids);
    return true;
  }

  // ---- reaching the balls: one Tab stop for the layer, then Tab goes through them in time ----

  let rover = $state<string | null>(null);
  const tabStop = $derived(!on ? null : rover && byId.has(rover) && !hiddenOf(rover) ? rover : (inTimeOrder.find((id) => !hiddenOf(id)) ?? null));
  const ballEl = (id: string) => root?.querySelector<HTMLElement>(`[data-sound="${id}"]`) ?? null;

  function tabTo(from: string, dir: 1 | -1) {
    const order = inTimeOrder.filter((id) => !hiddenOf(id));
    const next = order[order.indexOf(from) + dir];
    // past either end, Tab leaves the layer as usual
    if (!next) return false;
    ballEl(next)?.focus();
    return true;
  }

  // ---- the opened sound ----

  let panel = $state<{ key(e: KeyboardEvent): boolean } | null>(null);
  let shaping: string | null = null;
  let wasOpen: string | null = null;

  function openSound(id: string) {
    if (!byId.has(id) || hiddenOf(id)) return;
    if (gesture) cancelGesture();
    flushBurst();
    app.soundSel = [id];
    wasOpen = id;
    app.soundOpen = id;
    hover = null;
    tip = null;
    const s = byId.get(id);
    // the sound's line comes to the middle, unless the playhead is running
    if (s && !app.playing) app.scrubTo(clamp(s.start_s + Math.min(s.len_s, 6) / 2, 0, sceneEnd));
    speak(`Opened ${s?.name ?? 'sound'}`);
  }

  /** `refocus`: the keyboard came from the ball, so it goes back there. A click outside has
   *  put the focus where it wanted it. */
  function closeSound(refocus: boolean) {
    const id = wasOpen;
    if (!app.soundOpen) return;
    flushBurst();
    app.soundOpen = null;
    speak('Closed');
    if (!refocus) return;
    void tick().then(() => {
      const back = id && byId.has(id) && !hiddenOf(id) ? id : tabStop;
      if (back) ballEl(back)?.focus();
    });
  }

  // a sound that goes, or goes out of sight, closes (handover 5.7); if the focus goes with
  // it, it lands on the ball
  $effect(() => {
    if (!app.soundOpen || (on && opened && !isHidden(opened))) return;
    const el = document.activeElement;
    untrack(() => closeSound(!el || el === document.body || !!el.closest('[data-testid="opened-sound"]')));
  });

  // a click outside closes; the transport and the header are for listening meanwhile
  $effect(() => {
    if (!app.soundOpen) return;
    const down = (e: MouseEvent) => {
      const t = e.target as HTMLElement | null;
      if (t?.closest('[data-testid="opened-sound"], [data-keeps-sound]')) return;
      closeSound(false);
    };
    window.addEventListener('mousedown', down, true);
    return () => window.removeEventListener('mousedown', down, true);
  });

  const card = $derived.by(() => {
    const left = spineX + CARD_GAP;
    return { left, top: CARD_TOP, width: clamp(rightEdge - left - 12, CARD_MIN_W, CARD_W), height: Math.max(320, stageH - CARD_TOP - CARD_BELOW) };
  });
  const wedgeLine = $derived.by(() => {
    if (!opened) return { x: 0, y0: 0, y1: 0 };
    const y = (v: number) => stageH / 2 + v - off;
    return { x: laneX(liveLanes.get(opened.id) ?? 0), y0: y(map.y(opened.start_s)), y1: y(lineEnd(opened)) };
  });

  const ops: Ops = {
    begin() {
      flushBurst();
      shaping = app.soundOpen;
      if (shaping) dragging = [shaping];
      settler.hold();
    },
    preview(patch) {
      if (shaping) patchAll([{ id: shaping }], () => patch);
    },
    finish(cmds, said) {
      const id = shaping;
      shaping = null;
      dragging = [];
      settler.release();
      if (!id) return;
      if (said) speak(said);
      void commit(cmds, [id]);
    },
    cancel() {
      const id = shaping;
      shaping = null;
      dragging = [];
      settler.release();
      if (id) letGo([id]);
    },
    apply(patch, cmds, said) {
      const id = app.soundOpen;
      if (!id) return;
      flushBurst();
      patchAll([{ id }], () => patch);
      speak(said);
      void commit(cmds, [id]);
    },
    nudge(part, delta) {
      if (app.soundOpen) nudge(part, delta, [app.soundOpen]);
    },
  };

  // ---- what a screen reader hears ----

  let spoken = $state('');
  function speak(text: string) {
    // the same words twice must still be read out
    spoken = spoken === text ? `${text}\u00a0` : text;
  }

  let heardSel = 0;
  $effect(() => {
    const n = sel.length;
    const last = heardSel;
    heardSel = n;
    if (n === last && n !== 1) return;
    untrack(() => {
      // opening says so itself
      if (app.soundOpen) return;
      if (n > 1) speak(`${n} sounds selected`);
      else if (n === 1) speak(`${byId.get(sel[0])?.name ?? 'Sound'} selected`);
      else if (last) speak('Selection cleared');
    });
  });

  function ballAt(e: WheelEvent) {
    const p = point(e);
    for (const [id, b] of disp) {
      if (!b.tucked && !hiddenOf(id) && Math.hypot(p.x - b.x, p.y - b.y) <= b.r + 4) return id;
    }
    return null;
  }

  let seekTimer: ReturnType<typeof setTimeout> | undefined;

  function onWheel(e: WheelEvent): boolean {
    if (!on) return false;
    let d = e.deltaY || e.deltaX;
    if (e.deltaMode === 1) d *= 40;
    if (e.shiftKey) {
      // the view covers the balls behind it: over it, ⇧ scroll is the open sound's
      const over = !!(e.target as Element | null)?.closest?.('[data-testid="opened-sound"]');
      const id = over ? app.soundOpen : ballAt(e);
      if (!id) return false;
      nudge('gain', -d * 0.075, sel.includes(id) ? sel : [id]);
      return true;
    }
    const t = clamp(app.pos + d * 0.008, 0, sceneEnd);
    app.scrubTo(t);
    if (app.playing) {
      // playback restarts once, where the scrub stops
      clearTimeout(seekTimer);
      seekTimer = setTimeout(() => app.seek(app.pos), SEEK_MS);
    }
    return true;
  }

  onMount(() => {
    app.soundsUi = { key: onKey, wheel: onWheel, placeFile, dropOnAction };
    return () => {
      app.soundsUi = null;
      app.soundOpen = null;
      flushBurst();
      if (gesture) cancelGesture();
      settler.cancel();
      for (const t of [hoverTimer, groupTimer, staggerTimer, seekTimer]) clearTimeout(t);
    };
  });

  // ---- adding from the bucket ----

  let ghost = $state<{ x: number; y: number; tone: string } | null>(null);

  function dropSpot(e: Pick<MouseEvent, 'clientX' | 'clientY'> & { altKey?: boolean }) {
    const p = point(e);
    if (p.x < spineX - 20 || p.x > rightEdge) return null;
    const t = Math.max(0, map.t(p.y));
    const target = caught(e, t, p.y);
    return { p, t: target?.t ?? t, target };
  }

  function dragLeave() {
    ghost = null;
    tip = null;
    snapY = null;
  }

  function dragOver(e: MouseEvent, s: DragSound) {
    const at = on ? dropSpot(e) : null;
    if (!at) {
      dragLeave();
      return false;
    }
    ghost = { x: at.p.x, y: at.p.y, tone: (s.bucket_id && toneOf.get(s.bucket_id)) || '--bucket-none-rgb' };
    snapY = at.target ? map.y(at.target.t) : null;
    tip = { x: at.p.x + 26, y: at.p.y - 12, text: `${s.name} · ${at.target?.start ?? `at ${fmt(at.t)}`}` };
    return true;
  }

  function drop(e: MouseEvent, s: DragSound) {
    const at = on ? dropSpot(e) : null;
    if (at) void place(at, s);
  }

  /** A sound dropped on an action line in the column: it hangs on that action. */
  function dropOnAction(line_id: string, s: DragSound) {
    const target = targets.find((x) => x.anchor.line_id === line_id);
    if (target) void place({ p: { x: (spineX + rightEdge) / 2, y: map.y(target.t) }, t: target.t, target }, s);
  }

  /** A sound filed from the Finder, at the height it was dropped: over the column or the panel
   *  it's still the time there that counts. */
  async function placeFile(s: DragSound, x: number, y: number) {
    const left = root?.getBoundingClientRect().left ?? 0;
    const at = on ? dropSpot({ clientX: clamp(x, left + spineX, left + rightEdge), clientY: y }) : null;
    if (at) await place(at, s);
  }

  async function place(at: NonNullable<ReturnType<typeof dropSpot>>, sound: DragSound) {
    const where = at.target ? { anchor_line_id: at.target.anchor.line_id, anchor_word: at.target.anchor.word, anchor_edge: at.target.anchor.edge, offset_s: at.target.offset_s ?? 0 } : { at_s: +at.t.toFixed(3) };
    const params = { scene: app.sceneId, bucket_id: sound.bucket_id, asset_id: sound.asset_id, remote_id: sound.remote_id, depth: +depthAt(at.p.x, field).toFixed(3), ...where };
    let id: string | undefined;
    if (sound.asset_id) {
      const r = await app.method<{ id: string }>('place_from_bucket', params);
      id = r?.id;
    } else {
      // a preview downloads first
      const job = await app.run(() => call<{ job_id: string }>('place_from_bucket', params));
      if (!job) return;
      app.say(`Downloading ${sound.name}…`);
      const j: Job | undefined = await app.waitJob(job.job_id);
      if (j?.state === 'failed') app.say(`Download failed: ${j.message}`);
      await app.refresh();
      id = (j?.result as { placement?: { id: string } } | undefined)?.placement?.id;
    }
    if (!id) return;
    const placedHere = app.edit?.placements.find((p) => p.id === id);
    if (placedHere && placedHere.lane !== 'sfx' && placedHere.lane !== 'ambience') {
      app.say('That one plays outside the room: find it on the Edit page');
      return;
    }
    const added = id;
    app.soundSel = [added];
    popping = [...popping, added];
    setTimeout(() => (popping = popping.filter((x) => x !== added)), 1000);
    settler.soon();
  }

  // ---- the panel's eyes ----

  const allBuckets = $derived([...placeable.map((b) => b.id), ...(sounds.some((s) => s.bucket === OTHER) ? [OTHER] : [])]);
  const eyes = $derived({
    hidden: app.hiddenBuckets,
    toggle: (id: string, solo: boolean) => {
      hover = null;
      const next = toggleHidden(app.hiddenBuckets, allBuckets, id, solo);
      const name = placeable.find((b) => b.id === id)?.name ?? 'Other sounds';
      speak(!next.length && solo ? 'All buckets shown' : solo ? `Only ${name} shown` : next.includes(id) ? `${name} faded` : `${name} shown`);
      app.hiddenBuckets = next;
    },
    color: (id: string) => {
      const t = toneOf.get(id);
      return t ? `rgb(var(${t}))` : null;
    },
    used: new Set(sounds.map((s) => s.asset_id)),
    others: sounds.some((s) => s.bucket === OTHER) ? { id: OTHER, color: 'rgb(var(--bucket-none-rgb))' } : null,
  });

  const stageStyle = $derived(`transform: translateY(${(-off).toFixed(1)}px); transition: ${slide}`);
</script>

<div class="snd" class:live={on} bind:this={root} bind:clientWidth={stageW} bind:clientHeight={stageH} use:dropzone={{ takes: 'bucket', over: dragOver, leave: dragLeave, drop }}>
  <div class="lasso" class:live={on} style="left: {spineX - 16}px; width: {Math.max(0, rightEdge - spineX + 16)}px" onmousedown={bandDown} role="presentation" data-testid="sounds-field"></div>

  <div class="world" bind:this={world} style={stageStyle} role="group" aria-label="Sounds of the scene">
    <svg class="art" aria-hidden="true">
      {#each spine as s (s.who)}<path d={s.d} fill={s.fill} />{/each}
      <g class="ribs">
        {#each drawn.lines as l (l.bucket)}<path d={l.d} class="thin" class:faded={l.faded} style="stroke: rgb(var({l.tone}) / {l.faded ? 0.1 : 0.55})" />{/each}
        {#if focus}
          {#key focus.s.id}
            <g class="grow" style="transform-origin: {focus.x}px {focus.y0.toFixed(1)}px">
              <path d={focus.body} style="fill: rgb(var({focus.s.tone}))" />
              <path d={focus.fade} style="fill: rgb(var({focus.s.tone}) / 0.55)" />
            </g>
          {/key}
          <path d={focus.rest} class="rest" style="stroke: rgb(var({focus.s.tone}))" data-testid="unused-source" />
        {/if}
        {#if snapY != null}<path d="M{spineX - 16} {snapY.toFixed(1)}H{spineX + 56}" class="snap" />{/if}
      </g>
    </svg>

    <div class="hits" class:off={!on}>
      {#each sounds as s (s.id)}
        {#if !isHidden(s)}
          {@const y0 = map.y(s.start_s)}
          <div class="hit" role="presentation" style="left: {laneX(liveLanes.get(s.id) ?? 0) - 4}px; top: {(y0 - 3).toFixed(1)}px; height: {(lineEnd(s) - y0 + 6).toFixed(1)}px" onmouseenter={() => hoverIn(s.id)} onmouseleave={() => hoverOut(s.id)} onmousedown={(e) => lineDown(e, s)} ondblclick={() => openSound(s.id)} data-testid="sound-line"></div>
        {/if}
      {/each}
      {#if focus}
        {#key focus.s.id}
          <div class="head" role="presentation" title="Drag to move the start" style="left: {focus.x - 6}px; top: {(focus.y0 - 6).toFixed(1)}px; --c: var({focus.s.tone})" onmousedown={(e) => focus && headDown(e, focus.s)} ondblclick={() => focus && openSound(focus.s.id)} onmouseenter={() => focus && hoverIn(focus.s.id)} onmouseleave={() => focus && hoverOut(focus.s.id)} data-testid="line-head"></div>
          <div class="tail" role="presentation" title="Drag to trim, the fade follows" style="left: {focus.x - 8}px; top: {(focus.y1 - 3).toFixed(1)}px" onmousedown={(e) => focus && trimDown(e, focus.s)} ondblclick={() => focus && openSound(focus.s.id)} onmouseenter={() => focus && hoverIn(focus.s.id)} onmouseleave={() => focus && hoverOut(focus.s.id)} data-testid="line-tail"></div>
        {/key}
      {/if}
    </div>

    <svg class="art tethers" class:off={!on} aria-hidden="true">
      <path d={drawn.tethers} class="tether" />
      {#if focus}<path d={focus.tether} class="tether on" style="stroke: rgb(var({focus.s.tone}) / 0.55)" />{/if}
    </svg>

    {#if openArea}
      <div class="halo" role="presentation" style="left: {openArea.x0.toFixed(0)}px; top: {openArea.y0.toFixed(0)}px; width: {openArea.w.toFixed(0)}px; height: {openArea.h.toFixed(0)}px" onmouseenter={() => keepGroup(openArea.key)} onmouseleave={leaveGroup}></div>
    {/if}

    {#each grouped as g (g.key)}
      {@const open = g.members.some((m) => !shown(m.id).tucked)}
      {@const tones = g.members.map((m) => `rgb(var(${byId.get(m.id)?.tone ?? '--bucket-none-rgb'}))`)}
      <div class="clu" class:open class:off={!on} role="presentation" title={g.members.map((m) => byId.get(m.id)?.name).join(', ')} style="left: {(g.x - g.r).toFixed(0)}px; top: {(g.y - g.r).toFixed(0)}px; width: {(2 * g.r).toFixed(0)}px; height: {(2 * g.r).toFixed(0)}px; --glow: {tones[0]}; opacity: {open || !on ? 0 : (1 - 0.45 * g.depth).toFixed(2)}; filter: blur({(4 * g.depth).toFixed(1)}px)" onmouseenter={() => keepGroup(g.key)} onmouseleave={leaveGroup} onmousedown={(e) => { e.stopPropagation(); e.preventDefault(); keepGroup(g.key); }} data-testid="sound-group">
        <div class="swirl" style="background: conic-gradient({[...tones, tones[0]].join(', ')})"></div>
        <div class="shine"></div>
        <span class="badge tn">{g.members.length}</span>
      </div>
      <span class="blab" style="left: {(g.x - 80).toFixed(0)}px; top: {(g.y + g.r + 5).toFixed(0)}px; opacity: {open || !on ? 0 : 0.75}">{byId.get(g.members[0].id)?.name} +{g.members.length - 1}</span>
    {/each}

    {#each sounds as s (s.id)}
      {@const b = shown(s.id)}
      {@const faded = isHidden(s)}
      {@const out = !on}
      {@const delay = staggering ? staggerOf(inTimeOrder.indexOf(s.id), sounds.length) : b.delay}
      {@const box = `left: ${(b.x - b.r).toFixed(1)}px; top: ${(b.y - b.r).toFixed(1)}px; width: ${(2 * b.r).toFixed(1)}px; height: ${(2 * b.r).toFixed(1)}px; --dl: ${delay}s; --tx: ${(laneX(liveLanes.get(s.id) ?? 0) - b.x).toFixed(0)}px`}
      <button type="button" class="ball" class:sel={selSet.has(s.id)} class:drag={dragSet.has(s.id)} class:faded class:tucked={b.tucked} class:out class:stagger={staggering} class:pulse={playing(s)} class:pop={popping.includes(s.id)} class:band={band?.hit.includes(s.id)} tabindex={s.id === tabStop ? 0 : -1} aria-pressed={selSet.has(s.id)} aria-describedby="sound-keys" aria-label={announce({ name: s.name, bucket: s.bucketName, start_s: s.start_s, len_s: s.len_s, gain_db: s.gain_db, pan: s.pan, depth: s.depth }, fmt)} data-sound={s.id} data-testid="sound-ball" data-depth={s.depth.toFixed(3)} data-start={s.start_s.toFixed(3)} style="{box}; --c: var({s.tone}); --d: {s.depth.toFixed(3)}; --pn: {s.pan.toFixed(3)}; --apn: {Math.abs(s.pan).toFixed(3)}; --g: {ringOf(s.gain_db).toFixed(3)}" onmouseenter={() => hoverIn(s.id)} onmouseleave={() => hoverOut(s.id)} onmousedown={(e) => ballDown(e, s.id)} ondblclick={() => openSound(s.id)} onfocus={() => { rover = s.id; hoverIn(s.id); }} onblur={() => hoverOut(s.id)}></button>
      <div class="mer" class:faded class:tucked={b.tucked} class:out class:drag={dragSet.has(s.id)} class:stagger={staggering} style={box}>
        <svg viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true"><path d={panLine(s.pan)} class:on={focusId === s.id || selSet.has(s.id)} vector-effect="non-scaling-stroke" /></svg>
      </div>
      <span class="blab" class:gone={faded || b.tucked || out} style="left: {(b.x - 80).toFixed(1)}px; top: {(b.y + b.r + 4).toFixed(1)}px; --c: var({s.tone}); --dl: {delay}s; opacity: {(1 - 0.6 * s.depth).toFixed(2)}">{s.name}</span>
    {/each}

    {#if ring}
      {#key primary?.id}
        <svg class="art" aria-hidden="true">
          <path d={ring.track} pathLength="100" class="rtrack" />
          <path d={ring.arc} pathLength="100" class="rarc" />
        </svg>
        <div class="knob" role="presentation" title="Turn to set loudness" style="left: {ring.kx.toFixed(1)}px; top: {ring.ky.toFixed(1)}px" onmousedown={ringDown} data-testid="loudness-knob"></div>
      {/key}
      <span class="tip solid tn" style="left: {(ring.x + ring.R + 14).toFixed(0)}px; top: {(ring.y - 13).toFixed(0)}px" data-testid="loudness">{ring.many > 1 ? `${ring.many} sounds · ` : ''}{ring.text}</span>
    {/if}

    {#if band}<div class="bandbox" style="left: {band.x0}px; top: {band.y0}px; width: {band.x1 - band.x0}px; height: {band.y1 - band.y0}px" data-testid="sound-band"></div>{/if}

    {#if ghost}
      <div class="ghost" style="left: {ghost.x}px; top: {ghost.y}px; --c: var({ghost.tone})"></div>
      <div class="ghostline" style="left: {spineX + 20}px; width: {Math.max(0, ghost.x - spineX - 40)}px; top: {ghost.y}px"></div>
    {/if}

    {#if tip}<div class="tip tn" style="left: {tip.x}px; top: {tip.y}px" data-testid="sound-tip">{tip.text}</div>{/if}
  </div>
  <p class="sr" id="sound-keys">Left and right arrows set depth, Option with them pan, Shift up and down loudness, comma and period move it, Enter opens it, Shift Enter adds it to the selection, Tab goes to the next sound.</p>
  <p class="sr" role="status" aria-live="polite" data-testid="sounds-said">{spoken}</p>
</div>

{#if opened}
  {#key opened.id}
    <OpenedSound bind:this={panel} s={stateOf(opened.id)} assetId={opened.asset_id} peaks={opened.peaks} name={opened.name} bucket={opened.bucketName} tone={opened.tone} {lines} {targets} box={card} line={wedgeLine} {ops} onclose={closeSound} />
  {/key}
{/if}

{#if app.editBucket}<SceneBucket given={buckets} reload={loadBuckets} {eyes} off={!on} />{/if}

<style>
  .snd {
    position: absolute;
    inset: 0;
    pointer-events: none;
    -webkit-mask-image: linear-gradient(to bottom, transparent 76px, #000 140px, #000 calc(100% - 110px), transparent calc(100% - 30px));
    mask-image: linear-gradient(to bottom, transparent 76px, #000 140px, #000 calc(100% - 110px), transparent calc(100% - 30px));
  }
  /* a sound on its way in: nothing here reacts until it lands */
  :global(body.sound-drag) .snd :not(.lasso) {
    pointer-events: none !important;
  }
  .lasso {
    position: absolute;
    top: 76px;
    bottom: 96px;
    display: none;
  }
  .lasso.live {
    display: block;
    pointer-events: auto;
  }
  .world {
    position: absolute;
    left: 0;
    top: 50%;
    width: 100%;
    height: 0;
  }
  .art {
    position: absolute;
    left: 0;
    top: 0;
    width: 1px;
    height: 1px;
    overflow: visible;
    pointer-events: none;
  }
  .snd:not(.live) .ribs,
  .snd:not(.live) .art path {
    opacity: 0;
  }
  .ribs,
  .art path {
    transition: opacity 0.45s ease;
  }
  .thin {
    fill: none;
    stroke-width: 2;
    stroke-linecap: round;
  }
  .rest {
    fill: none;
    stroke-width: 1.2;
    stroke-dasharray: 2 3;
    opacity: 0.7;
  }
  .snap {
    fill: none;
    stroke: var(--playhead);
    stroke-width: 2.5;
    stroke-linecap: round;
  }
  .grow {
    animation: rib 0.3s var(--ease-settle);
  }
  @keyframes rib {
    from {
      opacity: 0;
      transform: scale(0.55, 1);
    }
  }
  .tethers.off {
    opacity: 0;
  }
  .tether {
    fill: none;
    stroke: rgb(var(--ink-rgb) / 0.13);
    stroke-width: 1;
  }
  .tether.on {
    stroke-width: 1.5;
  }
  .hits {
    position: absolute;
    left: 0;
    top: 0;
    pointer-events: auto;
  }
  .hits.off {
    display: none;
  }
  .hit {
    position: absolute;
    width: 8px;
    cursor: ns-resize;
  }
  .head,
  .tail {
    position: absolute;
    animation: appear 0.3s ease;
  }
  @keyframes appear {
    from {
      opacity: 0;
    }
  }
  .head {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: rgb(var(--c));
    box-shadow:
      0 0 0 2px var(--stage-bg),
      0 0 0 4px rgb(var(--c) / 0.35);
    cursor: grab;
  }
  .tail {
    width: 16px;
    height: 6px;
    border-radius: 3px;
    background: var(--playhead);
    box-shadow: 0 0 0 1.5px var(--stage-bg);
    cursor: ns-resize;
    animation-delay: 0.08s;
    animation-fill-mode: both;
  }
  .halo {
    position: absolute;
    pointer-events: auto;
  }
  .ball,
  .mer {
    position: absolute;
    border-radius: 50%;
    transition:
      left 0.6s var(--ease-settle) var(--dl, 0s),
      top 0.6s var(--ease-settle) var(--dl, 0s),
      width 0.4s ease,
      height 0.4s ease,
      transform 0.5s var(--ease-settle) var(--dl, 0s),
      opacity 0.4s ease var(--dl, 0s),
      filter 0.3s,
      box-shadow 0.3s;
  }
  .ball {
    pointer-events: auto;
    cursor: grab;
    padding: 0;
    z-index: 2;
    background: radial-gradient(circle at calc(34% + var(--pn) * 30%) 30%, color-mix(in srgb, rgb(var(--c)) 50%, rgb(var(--highlight-rgb))), rgb(var(--c)) 55%, rgb(var(--c) / 0.7));
    box-shadow:
      inset 0 0 calc(var(--d) * 12px + 12px) rgb(var(--shadow-rgb) / 0.3),
      0 0 calc(10px + 34px * var(--g)) rgb(var(--c) / calc(0.15 + 0.45 * var(--g)));
    opacity: calc(1 - 0.55 * var(--d));
    filter: blur(calc(5px * var(--d))) saturate(calc(1 - 0.5 * var(--d) + 0.25 * var(--apn))) hue-rotate(calc(var(--pn) * 30deg));
  }
  .ball:focus-visible {
    outline: 2px solid var(--playhead);
    outline-offset: 3px;
  }
  .ball.sel {
    z-index: 4;
    box-shadow:
      inset 0 0 calc(var(--d) * 12px + 12px) rgb(var(--shadow-rgb) / 0.3),
      0 0 calc(10px + 34px * var(--g)) rgb(var(--c) / calc(0.15 + 0.45 * var(--g))),
      0 0 0 3px var(--stage-bg),
      0 0 0 4.5px rgb(var(--ink-rgb) / 0.7);
  }
  .ball.band {
    box-shadow: 0 0 0 2px var(--playhead);
  }
  .ball.drag,
  .mer.drag {
    z-index: 5;
    cursor: grabbing;
    transition:
      left 0.12s ease-out,
      top 0.12s ease-out,
      width 0.3s,
      height 0.3s,
      transform 0.25s ease,
      opacity 0.3s ease,
      filter 0.3s;
  }
  .ball.faded {
    opacity: 0.12;
    filter: blur(calc(5px * var(--d) + 3px)) saturate(0.3);
    pointer-events: none;
  }
  .ball.tucked {
    opacity: 0;
    transform: scale(0.6);
    pointer-events: none;
  }
  /* the Sounds toggle: they glide a little toward the spine and fade. No overshoot (D87). */
  .ball.out,
  .mer.out {
    opacity: 0;
    transform: translateX(calc(var(--tx) * 0.18)) scale(0.82);
    pointer-events: none;
  }
  .ball.stagger,
  .mer.stagger {
    transition:
      transform var(--dur-move) var(--ease-move) var(--dl),
      opacity 0.5s var(--ease-move) var(--dl),
      filter 0.3s;
  }
  .ball.out.stagger,
  .mer.out.stagger {
    transition:
      transform 0.5s var(--ease-move) var(--dl),
      opacity 0.4s var(--ease-move) var(--dl),
      filter 0.3s;
  }
  .ball.pulse {
    animation: pulse 1.4s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      transform: scale(1.06);
    }
  }
  .ball.pop {
    animation: pop 0.5s var(--ease-move);
  }
  @keyframes pop {
    from {
      transform: scale(0.82);
      opacity: 0;
    }
  }
  .mer {
    pointer-events: none;
    z-index: 3;
  }
  .mer.faded {
    opacity: 0.1;
  }
  .mer.tucked {
    opacity: 0;
  }
  .mer svg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    overflow: visible;
  }
  .mer path {
    fill: none;
    stroke: rgb(var(--highlight-rgb) / 0.55);
    stroke-width: 1.4;
    stroke-linecap: round;
  }
  .mer path.on {
    stroke: rgb(var(--highlight-rgb) / 0.9);
    stroke-width: 2;
  }
  .blab {
    position: absolute;
    width: 160px;
    text-align: center;
    font-size: 11.5px;
    font-weight: 600;
    white-space: nowrap;
    pointer-events: none;
    z-index: 2;
    color: color-mix(in srgb, rgb(var(--c, var(--ink-rgb))) 70%, rgb(var(--ink-rgb)));
    transition:
      left 0.6s var(--ease-settle) var(--dl, 0s),
      top 0.6s var(--ease-settle) var(--dl, 0s),
      opacity 0.4s ease var(--dl, 0s);
  }
  .blab.gone {
    opacity: 0 !important;
  }
  .clu {
    position: absolute;
    border-radius: 50%;
    cursor: pointer;
    pointer-events: auto;
    z-index: 1;
    box-shadow: 0 0 34px color-mix(in srgb, var(--glow) 45%, transparent);
    animation: wobble 3.2s ease-in-out infinite;
    transition:
      left 0.6s var(--ease-settle),
      top 0.6s var(--ease-settle),
      width 0.6s var(--ease-settle),
      height 0.6s var(--ease-settle),
      transform 0.6s var(--ease-settle),
      opacity 0.4s ease;
  }
  .clu.open,
  .clu.off {
    transform: scale(0.6);
    pointer-events: none;
  }
  @keyframes wobble {
    0%,
    100% {
      border-radius: 50%;
    }
    33% {
      border-radius: 47% 53% 52% 48%;
    }
    66% {
      border-radius: 53% 47% 48% 52%;
    }
  }
  .swirl {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    filter: blur(3px);
    animation: swirl 9s linear infinite;
  }
  @keyframes swirl {
    to {
      transform: rotate(360deg);
    }
  }
  .shine {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    background: radial-gradient(circle at 34% 30%, rgb(var(--highlight-rgb) / 0.6), rgb(var(--highlight-rgb) / 0.08) 42%, transparent 60%);
  }
  .badge {
    position: absolute;
    right: -4px;
    top: -4px;
    min-width: 20px;
    height: 20px;
    padding: 0 5px;
    box-sizing: border-box;
    border-radius: 10px;
    background: var(--playhead);
    color: var(--stage-bg);
    font-size: 11px;
    font-weight: 700;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 0 0 2px var(--stage-bg);
  }
  .rtrack,
  .rarc {
    fill: none;
    stroke-width: 5;
    stroke-dasharray: 100;
    animation: draw 0.7s var(--ease-settle) both;
  }
  .rtrack {
    stroke: rgb(var(--ink-rgb) / 0.18);
  }
  .rarc {
    stroke: var(--playhead);
    stroke-linecap: round;
  }
  @keyframes draw {
    from {
      stroke-dashoffset: 100;
      opacity: 0;
    }
    to {
      stroke-dashoffset: 0;
      opacity: 1;
    }
  }
  .knob {
    position: absolute;
    width: 18px;
    height: 18px;
    margin: -9px 0 0 -9px;
    border-radius: 50%;
    background: var(--playhead);
    box-shadow:
      0 0 0 2px var(--stage-bg),
      0 4px 12px rgb(var(--shadow-rgb) / 0.5);
    cursor: grab;
    pointer-events: auto;
    z-index: 6;
    animation: appear 0.35s ease 0.45s both;
  }
  .tip {
    position: absolute;
    padding: 6px 10px;
    border-radius: 10px;
    font-size: 11.5px;
    font-weight: 600;
    white-space: nowrap;
    pointer-events: none;
    z-index: 40;
    color: var(--playhead);
    background: var(--chip);
    box-shadow: inset 0 0 0 1px rgb(var(--ink-rgb) / 0.35);
  }
  .tip.solid {
    z-index: 6;
    color: var(--stage-bg);
    background: var(--playhead);
    box-shadow: none;
  }
  .bandbox {
    position: absolute;
    z-index: 7;
    pointer-events: none;
    border-radius: 6px;
    background: rgb(var(--ink-rgb) / 0.06);
    box-shadow: inset 0 0 0 1px rgb(var(--ink-rgb) / 0.4);
  }
  .ghost {
    position: absolute;
    width: 36px;
    height: 36px;
    margin: -18px 0 0 -18px;
    border-radius: 50%;
    pointer-events: none;
    z-index: 31;
    background: radial-gradient(circle at 34% 30%, color-mix(in srgb, rgb(var(--c)) 50%, rgb(var(--highlight-rgb))), rgb(var(--c)) 60%);
    box-shadow:
      0 0 24px rgb(var(--c) / 0.6),
      0 0 0 3px rgb(var(--ink-rgb) / 0.5);
  }
  .ghostline {
    position: absolute;
    height: 0;
    border-top: 1.5px dashed rgb(var(--ink-rgb) / 0.5);
    pointer-events: none;
    z-index: 30;
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  @media (prefers-reduced-motion: reduce) {
    .world,
    .ball,
    .mer,
    .blab,
    .clu,
    .ribs,
    .art path {
      transition: none !important;
    }
    .ball,
    .clu,
    .swirl,
    .grow,
    .head,
    .tail,
    .knob,
    .rtrack,
    .rarc {
      animation: none !important;
    }
  }
</style>
