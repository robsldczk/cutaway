<script lang="ts">
  // The review screen: the scene as cards with the dialogue spine, the notes in the margin beside
  // it, and only what a reviewer needs around them. Nothing here changes the cut, and the bus
  // wouldn't let it. It reads the same live scene as every other page.
  import { onDestroy, onMount, tick, untrack } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { call, savePath } from '../lib/bridge';
  import { brand } from '../lib/brand';
  import { charColor, charLabel, rgba } from '../lib/colors';
  import { clamp, fmt } from '../lib/format';
  import { filtered, placeCards, reviewRows, roleLabel, roundLine, tagLabel, TAGS, threadsOf, type Note, type NoteFilter, type NoteTag, type NoteTarget } from '../lib/review';
  import { spinePaths } from '../lib/sounds/paths';
  import type { ItemView } from '../lib/types';
  import { rowSpans, TimeMap } from '../lib/sounds/timemap';
  import { lineWords } from '../lib/words';
  import Icon from '../components/Icon.svelte';
  import NoteCard from '../components/NoteCard.svelte';
  import ReviewSearch from '../components/ReviewSearch.svelte';

  /** Where the rail sits, and the room it takes. */
  const RAIL = 124;
  const LEFT = 40;

  let stageW = $state(window.innerWidth);
  let stageH = $state(window.innerHeight);
  const sc = $derived(app.scale);
  const v = $derived(app.view);
  const r = $derived(app.review);

  // ---- the column ----
  const room = $derived(stageW - RAIL - LEFT);
  const cardW = $derived(Math.round(clamp(room * 0.4, 260, 440)));
  const spineX = $derived(LEFT + cardW + 50);
  const notesX = $derived(spineX + 64);
  const notesW = $derived(Math.max(240, Math.min(460, stageW - RAIL - notesX)));

  const laid = $derived(reviewRows(v?.items ?? [], sc));
  const map = $derived(new TimeMap(rowSpans(laid.rows.filter((x) => x.kind !== 'gap').map((x) => ({ start_s: x.start_s, end_s: x.end_s, top: x.top, height: x.height }))), 14 * sc));
  const spine = $derived([...spinePaths(app.lines, map, spineX, 11)].map(([who, d]) => ({ who, d, fill: rgba(charColor(v, who).rgb, 0.95) })));
  const end = $derived(v?.duration_s ?? 0);
  /** Where the wheel left the column. Between cards no time passes, so a height can't be got
   *  back from the playhead: the height itself is kept for as long as the playhead stays put. */
  let scroll = $state<{ y: number; t: number; line?: string } | null>(null);
  let gliding = $state(false);
  const off = $derived(scroll && (gliding || scroll.t === app.pos) ? scroll.y : map.y(Math.min(app.pos, end)));
  const sounds = $derived((app.edit?.placements ?? []).filter((p) => p.lane !== 'dialogue'));

  // a new layout has other heights; a card the arrows or a click brought up stays in the middle
  $effect(() => {
    void laid;
    untrack(() => {
      if (gliding) return;
      const held = scroll?.line && scroll.t === app.pos ? laid.rows.find((x) => x.kind === 'line' && x.it?.line_id === scroll?.line) : undefined;
      scroll = held && scroll ? { ...scroll, y: held.top } : null;
    });
  });

  // ---- the notes ----
  let by = $state<NoteFilter>('all');
  let kind = $state<NoteTag | null>(null);

  const earlier = $derived(!!r?.round && r.round !== r.current_round);
  const round = $derived(r?.rounds.find((x) => x.id === r.round) ?? null);
  /** The note being dragged, and where it would land. */
  let moving = $state<{ id: string; start: number; end: number | null; line?: string } | null>(null);
  const MOVING = '__moving';
  /** Today's view is the state of everything; an earlier round shows the notes it brought. A
   *  note on the move goes by itself, at the pointer. */
  const notes = $derived(
    (r ? filtered(r.notes, by, kind, earlier ? r.round : null) : []).map((n) =>
      n.id === moving?.id ? { ...n, start_s: moving.start, end_s: moving.end, time: fmt(moving.start), detached: false, on: n.on === 'scene' ? ('word' as const) : n.on, thread: MOVING } : n,
    ),
  );
  const open = $derived(r?.notes.filter((n) => n.status !== 'resolved').length ?? 0);

  let heights = $state<Record<string, number>>({});
  const heightOf = (id: string, draft = false) => heights[id] ?? (draft ? 150 : 104);

  interface Draft {
    params: Record<string, unknown>;
    target: NoteTarget;
    text: string;
    tag: NoteTag | null;
  }
  let draft = $state<Draft | null>(null);
  const DRAFT = '__draft';

  const threads = $derived(threadsOf(notes));
  const placed = $derived.by(() => {
    const live = threads.filter((t) => !t[0].detached);
    const cards: { item: string; y: number; height: number }[] = live.map((t) => ({ item: t[0].thread, y: map.y(t[0].start_s ?? 0), height: heightOf(t[0].thread) }));
    if (draft) cards.push({ item: DRAFT, y: map.y(draft.target.start_s ?? 0), height: heightOf(DRAFT, true) });
    const out = placeCards(cards);
    // what's left the cut sits below it, listed in its scene
    let y = Math.max(laid.height + 40, ...out.map((p) => p.top + heightOf(p.item)));
    for (const t of threads.filter((x) => x[0].detached)) {
      out.push({ item: t[0].thread, top: y });
      y += heightOf(t[0].thread) + 10;
    }
    return out;
  });
  const threadOf = (id: string) => threads.find((t) => t[0].thread === id) ?? [];
  const topOf = (id: string) => placed.find((p) => p.item === id)?.top ?? 0;

  function link(y: number, top: number) {
    const x = spineX + 18;
    return `M${x} ${y.toFixed(1)} C${x + 26} ${y.toFixed(1)} ${x + 16} ${(top + 20).toFixed(1)} ${notesX} ${(top + 20).toFixed(1)} `;
  }
  const located = $derived(notes.filter((n) => n.start_s !== null && n.on !== 'scene'));
  const links = $derived(threads.filter((t) => !t[0].detached && t[0].on !== 'scene').map((t) => link(map.y(t[0].start_s ?? 0), topOf(t[0].thread))).join(''));
  const dots = $derived(located.filter((n) => n.end_s === null));
  const brackets = $derived(located.filter((n) => n.end_s !== null));
  const dotAt = (n: Note) => map.y(n.start_s ?? 0);

  // ---- writing a note ----
  let field = $state<HTMLTextAreaElement>();
  let span = $state<{ a: number; b: number } | null>(null);
  let colEl = $state<HTMLDivElement>();

  const defaultTag = (t: NoteTarget): NoteTag => (t.on === 'pause' ? 'timing' : t.on === 'sound' ? 'sound' : 'performance');

  async function write(params: Record<string, unknown>) {
    const target = await app.run(() => call<NoteTarget>('note_target', params));
    if (!target) return;
    draft = { params, target, text: draft?.text ?? '', tag: defaultTag(target) };
    await tick();
    field?.focus();
  }

  const onScene = (extra: object = {}) => ({ scene: app.sceneId, ...extra });

  async function save() {
    const d = draft;
    if (!d || !d.text.trim()) return;
    if (app.tour) return app.say('Notes aren’t kept during the tour.');
    draft = null;
    await app.addNote(d.params, d.text.trim(), d.tag);
    app.say('Note added');
  }

  function fieldKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void save();
    } else if (e.key === 'Escape') draft = null;
  }

  function timeAt(e: MouseEvent) {
    const top = colEl?.getBoundingClientRect().top ?? 0;
    return clamp(map.t(e.clientY - top), 0, end);
  }

  /** A click on the wave is a marker; a drag along it is a range. */
  function spineDown(e: MouseEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    const a = timeAt(e);
    const y0 = e.clientY;
    let moved = false;
    span = { a, b: a };
    const move = (ev: MouseEvent) => {
      if (Math.abs(ev.clientY - y0) > 3) moved = true;
      if (moved) span = { a, b: timeAt(ev) };
    };
    const up = (ev: MouseEvent) => {
      window.removeEventListener('mousemove', move);
      window.removeEventListener('mouseup', up);
      const b = timeAt(ev);
      span = null;
      void write(moved ? onScene({ at_s: Math.min(a, b), to_s: Math.max(a, b) }) : onScene({ at_s: a }));
    };
    window.addEventListener('mousemove', move);
    window.addEventListener('mouseup', up);
  }

  // ---- moving a note ----

  interface Grab {
    note: Note;
    /** From the note's spot to the pointer, in column pixels. */
    offset: number;
    /** A range keeps its length. */
    len: number | null;
    y0: number;
    clientY: number;
    moved: boolean;
  }
  let grab: Grab | null = null;
  let areaEl = $state<HTMLDivElement>();
  let edgeFrame = 0;

  const colTop = () => colEl?.getBoundingClientRect().top ?? 0;

  function aim() {
    const g = grab;
    if (!g?.moved) return;
    const room = end - (g.len ?? 0);
    const start = clamp(map.t(g.clientY - colTop() - g.offset), 0, Math.max(0, room));
    // a whole line goes from line to line, so it shows where it'll land
    const line = g.note.on === 'line' ? nearestLine(start) : undefined;
    moving = line ? { id: g.note.id, start: line.start_s, end: line.end_s, line: line.line_id } : { id: g.note.id, start, end: g.len === null ? null : start + g.len };
  }

  function nearestLine(t: number) {
    const away = (l: ItemView) => (t < l.start_s ? l.start_s - t : t > l.end_s ? t - l.end_s : 0);
    return app.lines.filter((l) => l.end_s > l.start_s).reduce<ItemView | undefined>((a, l) => (!a || away(l) < away(a) ? l : a), undefined);
  }

  /** Near the top or bottom of the column it scrolls on, so a note can go anywhere in the scene. */
  function edgeScroll() {
    edgeFrame = 0;
    const g = grab;
    if (!g) return;
    const a = areaEl?.getBoundingClientRect();
    if (g.moved && a) {
      const d = g.clientY < a.top + 70 ? -9 : g.clientY > a.bottom - 50 ? 9 : 0;
      if (d) scrollBy(d);
      aim();
    }
    edgeFrame = requestAnimationFrame(edgeScroll);
  }

  function letGo() {
    window.removeEventListener('mousemove', dragMove);
    window.removeEventListener('mouseup', dragUp);
    cancelAnimationFrame(edgeFrame);
    grab = null;
  }

  function dragMove(e: MouseEvent) {
    const g = grab;
    if (!g) return;
    g.clientY = e.clientY;
    if (Math.abs(e.clientY - g.y0) > 3) g.moved = true;
    aim();
  }

  async function dragUp() {
    const g = grab;
    const m = moving;
    letGo();
    if (!g) return;
    if (!g.moved || !m) {
      // a press that went nowhere is a click: to the note
      void goTo(g.note);
      return;
    }
    // it stays where it was dropped until the new view lands
    const spot = m.line ? { line_id: m.line } : onScene(m.end === null ? { at_s: m.start } : { at_s: m.start, to_s: m.end });
    await app.note('move_note', { note: g.note.id, ...spot });
    moving = null;
  }

  function grabNote(e: MouseEvent, n: Note) {
    if (e.button !== 0 || !n.can_change) return;
    e.preventDefault();
    e.stopPropagation();
    // a note with no spot, on the scene or detached, starts from its card
    let at = n.start_s !== null && n.on !== 'scene' ? map.y(n.start_s) : topOf(n.thread) + 20;
    // a whole line is held by its middle, which is what picks the line it lands on
    if (n.on === 'line' && n.start_s !== null && n.end_s !== null) at = (map.y(n.start_s) + map.y(n.end_s)) / 2;
    const ranged = n.on === 'words' && n.start_s !== null && n.end_s !== null;
    grab = { note: n, offset: e.clientY - colTop() - at, len: ranged ? (n.end_s ?? 0) - (n.start_s ?? 0) : null, y0: e.clientY, clientY: e.clientY, moved: false };
    window.addEventListener('mousemove', dragMove);
    window.addEventListener('mouseup', dragUp);
    edgeFrame = requestAnimationFrame(edgeScroll);
  }

  // ---- the wheel ----

  /** A notch or a trackpad, the column glides to where it's asked to be. */
  const GLIDE = 0.3;
  const FAST = 4;
  /** Playing, it starts again from where the scroll stops. */
  const SEEK_MS = 180;
  let target: number | null = null;
  let frame = 0;
  let seekTimer: ReturnType<typeof setTimeout> | undefined;
  const still = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  function place(y: number) {
    const t = clamp(map.t(y), 0, end);
    app.scrubTo(t);
    scroll = { y, t };
  }

  function settle() {
    target = null;
    gliding = false;
    if (app.playing) {
      clearTimeout(seekTimer);
      seekTimer = setTimeout(() => app.seek(app.pos), SEEK_MS);
    }
  }

  function step() {
    frame = 0;
    if (target === null) return;
    const left = target - off;
    const y = Math.abs(left) < 0.5 ? target : off + left * GLIDE;
    place(y);
    if (y === target) settle();
    else frame = requestAnimationFrame(step);
  }

  function scrollBy(d: number) {
    const to = clamp((target ?? off) + d, map.y(0), map.y(end));
    if (still()) {
      place(to);
      return settle();
    }
    target = to;
    gliding = true;
    frame ||= requestAnimationFrame(step);
  }

  function wheel(e: WheelEvent) {
    let d = e.deltaY || e.deltaX;
    if (e.deltaMode === 1) d *= 40;
    scrollBy(e.shiftKey ? d * FAST : d);
    return true;
  }

  // ---- the arrows ----

  /** The cards in the order they're laid out. A line with no time of its own (missing, or no
   *  take), or one wholly under the line before it, has no height of its own in the time map:
   *  the playhead alone can't bring it to the middle, so the arrows go by the cards. */
  const cards = $derived(laid.rows.filter((x) => x.kind === 'line' && x.it));
  const focusId = $derived(app.cur?.line_id ?? null);

  /** The card nearest the middle of the column, when the current line isn't one of them. */
  function nearestCard(): number {
    let best = 0;
    cards.forEach((c, i) => {
      if (Math.abs(c.top - off) < Math.abs(cards[best].top - off)) best = i;
    });
    return best;
  }

  function stepCard(d: number) {
    if (!cards.length) return;
    const at = cards.findIndex((c) => c.it?.line_id === focusId);
    const row = cards[clamp((at < 0 ? nearestCard() : at) + d, 0, cards.length - 1)];
    const it = row.it;
    if (!it) return;
    cancelAnimationFrame(frame);
    frame = 0;
    target = null;
    gliding = false;
    // playing, it plays on from the line; the line is picked after the seek, which would pick
    // the line playing at that moment
    if (app.playing) app.seek(it.start_s);
    app.select(it.line_id);
    scroll = { y: row.top, t: app.pos, line: it.line_id };
  }

  // ---- going to a note ----
  async function goTo(n: Note) {
    if (n.scene_id !== app.sceneId) await app.loadScene(n.scene_id);
    if (n.line_id) app.select(n.line_id);
    if (n.start_s !== null) app.seek(n.start_s);
    app.noteFocus = n.id;
  }

  $effect(() => {
    if (!app.noteFocus) return;
    const t = setTimeout(() => (app.noteFocus = null), 1800);
    return () => clearTimeout(t);
  });

  // ---- the header ----
  let rounds = $state(false);
  let search = $state<ReturnType<typeof ReviewSearch>>();

  async function send() {
    const who = app.reviewer?.current;
    if (!who) return app.whoFirst(() => void send());
    const name = `${who.name} ${app.status?.name ?? ''}`.trim();
    const path = await savePath({ title: 'Send review', name: `${name}.${brand.reviewExt}`, filters: [{ name: `${brand.displayName} review`, extensions: [brand.reviewExt] }] });
    if (!path) return;
    const out = await app.run(() => call<{ file: string; notes: number; events: number }>('send_review', { path }));
    if (out) app.say(`Saved ${out.notes} ${out.notes === 1 ? 'note' : 'notes'} to ${out.file.split('/').pop()}`);
  }

  onMount(() => {
    app.reviewUi = {
      marker: () => void write(onScene({ at_s: app.pos })),
      note: () => void (app.cur ? write({ line_id: app.cur.line_id }) : write(onScene())),
      search: () => search?.focus(),
      wheel,
      step: stepCard,
      cancel: () => {
        if (grab) {
          letGo();
          moving = null;
          return true;
        }
        if (!draft) return false;
        draft = null;
        return true;
      },
    };
  });
  onDestroy(() => {
    app.reviewUi = null;
    letGo();
    cancelAnimationFrame(frame);
    clearTimeout(seekTimer);
  });
</script>

<svelte:window bind:innerWidth={stageW} bind:innerHeight={stageH} />

<div class="review" data-testid="review">
  <header class="rh" data-keeps-sound>
    <span class="rv">Review</span>
    <span class="ep ellip" title={app.status?.path}>{r?.project && r.project !== r.episode ? `${r.project} ` : ''}{r?.episode ?? app.status?.name}</span>
    <div class="rp">
      <button class="round ellip" aria-haspopup="menu" aria-expanded={rounds} onclick={() => (rounds = !rounds)} data-testid="round-picker">
        {#if round}Round {round.number}{round.cut ? ` · cut ${round.cut}` : ''}{earlier ? ' · earlier' : ''}{:else}The live cut{/if}<Icon name="chevron" size={10} stroke={2.6} />
      </button>
      {#if rounds}
        <div class="scrim0" role="presentation" onclick={() => (rounds = false)}></div>
        <div class="pop" role="menu" aria-label="Review rounds" data-testid="round-menu">
          {#each r?.rounds ?? [] as x (x.id)}
            <button role="menuitemradio" aria-checked={x.id === r?.round} class:on={x.id === r?.round} onclick={() => { rounds = false; void app.pickRound(x.id); }} data-testid="round-item">
              <b>Round {x.number}{x.id === r?.current_round ? ' · current' : ''}{x.status === 'closed' ? ' · closed' : ''}</b>
              <span class="muted tn">{roundLine(x)}</span>
            </button>
          {:else}
            <p class="muted none">No rounds yet: you're looking at the live cut.</p>
          {/each}
        </div>
      {/if}
    </div>
    <div class="mid"><ReviewSearch bind:this={search} onnote={goTo} /></div>
    <button class="me" onclick={() => (app.asking = { closable: true })} title="Switch reviewer" data-testid="reviewer-chip">
      <span class="av">{app.reviewer?.current?.initials ?? '?'}</span>
      <span class="ellip name">{app.reviewer?.current?.name ?? 'Who’s reviewing?'}</span>
      {#if app.reviewer?.current}<span class="muted role">{roleLabel(app.reviewer.current.role)}</span>{/if}
      <Icon name="chevron" size={10} stroke={2.6} />
    </button>
    <button class="btnx help" onclick={() => (app.overlay = 'help')} aria-label="Keys and tour" title="Keys and tour" data-testid="review-help">?</button>
    <button class="btnx" onclick={() => (app.overlay = 'review-history')} data-testid="review-history">History</button>
    <button class="btnx" onclick={send} title="Save your notes as one file, ready to e-mail or hand over" data-testid="send-review" data-tour="finish">Send review</button>
    <button class="btnx pri" onclick={() => app.leaveReview()} data-testid="review-done">Done</button>
  </header>

  <div class="area" bind:this={areaEl} style="right: {RAIL - 16}px">
    <div class="col" bind:this={colEl} style="transform: translateY({(stageH / 2 - 64 - off).toFixed(1)}px); transition: {gliding ? 'none' : app.playing ? 'transform .1s linear' : 'transform .3s var(--ease-out)'}">
      {#each laid.rows as row (row.key)}
        {#if row.kind === 'line' && row.it}
          {@const it = row.it}
          {@const c = charColor(v, it.character).rgb}
          {@const cur = app.playing ? app.pos >= it.start_s && app.pos < it.end_s + 0.4 : it.line_id === focusId}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions (the arrows walk the lines) -->
          <div class="card glass" class:cur style="left: {LEFT}px; width: {cardW}px; top: {row.top}px; height: {row.height}px; background: linear-gradient(90deg, {rgba(c, cur ? 0.4 : 0.22)}, {rgba(c, 0.06)} 70%, var(--glass-fill-1)); box-shadow: inset 0 0 0 {cur ? 1.5 : 1}px {rgba(c, cur ? 0.7 : 0.16)}" onclick={() => { app.seek(it.start_s); app.select(it.line_id); scroll = { y: row.top, t: app.pos, line: it.line_id }; }} data-testid="review-line" data-line={it.line_id} data-tour="card">
            <div class="ch"><span class="nm" style="color: {charColor(v, it.character).name}">{charLabel(v, it.character)}</span><span class="tn tcd">{fmt(it.start_s)}</span></div>
            <div class="script ellip txt" title={it.text}>{it.words.length ? lineWords(it).filter((w) => !w.deleted).map((w) => w.text).join(' ') : it.text}</div>
            <button class="addn" aria-label="Note on this line" title="Note on this line (N)" onclick={(e) => { e.stopPropagation(); void write({ line_id: it.line_id }); }} data-testid="line-note" data-tour="add-note"><Icon name="plus" size={12} stroke={2.6} /></button>
          </div>
        {:else if row.kind === 'act' && row.it}
          <div class="act script ellip" style="left: {LEFT}px; width: {cardW}px; top: {row.top}px; height: {row.height}px" title={row.it.text}>{row.it.text}</div>
        {:else}
          <div class="gp tn" style="left: {LEFT}px; width: {cardW}px; top: {row.top}px; height: {row.height}px">{row.label}</div>
        {/if}
      {/each}

      <svg class="draw" width={spineX + 80} height={Math.max(1, laid.height)} aria-hidden="true">
        <line x1={spineX} y1="-300" x2={spineX} y2={laid.height + 300} stroke="var(--stage-hairline)" />
        {#each spine as s (s.who)}<path d={s.d} fill={s.fill} />{/each}
        <path d={links} stroke="rgb(var(--accent-rgb) / 0.55)" stroke-width="1.3" fill="none" />
        {#if draft && draft.target.start_s !== null}<path d={link(map.y(draft.target.start_s), topOf(DRAFT))} stroke="var(--link)" stroke-width="1.6" fill="none" />{/if}
      </svg>

      <!-- svelte-ignore a11y_no_static_element_interactions (the keyboard has M and N) -->
      <div class="hit" style="left: {spineX - 26}px; height: {laid.height}px" onmousedown={spineDown} title="Click for a marker, drag for a range" data-testid="review-spine" data-tour="spine"></div>

      {#each sounds as p (p.id)}
        <button class="snd" style="left: {spineX + 22}px; top: {map.y(p.start_s) - 6}px" title="Note on {p.name}" aria-label="Note on {p.name}" onclick={() => write({ placement_id: p.id })} data-testid="review-sound"></button>
      {/each}

      {#each dots as n (n.id)}
        <button class="dot" class:can={n.can_change} class:done={n.status === 'resolved'} style="left: {spineX}px; top: {dotAt(n).toFixed(1)}px" title={n.can_change ? 'Drag to move this note' : n.text} aria-label="Note {n.n}" onmousedown={(e) => grabNote(e, n)} onclick={() => !n.can_change && goTo(n)} data-testid="note-dot" data-spot={n.id}></button>
      {/each}
      {#each brackets as n (n.id)}
        <!-- svelte-ignore a11y_no_static_element_interactions (the CLI and MCP have move_note) -->
        <div class="br" class:can={n.can_change} class:done={n.status === 'resolved'} style="left: {spineX + 12}px; top: {map.y(n.start_s ?? 0).toFixed(1)}px; height: {Math.max(6, map.y(n.end_s ?? 0) - map.y(n.start_s ?? 0)).toFixed(1)}px" title={n.can_change ? 'Drag to move this note' : undefined} onmousedown={(e) => grabNote(e, n)} data-testid="note-bracket" data-spot={n.id}></div>
      {/each}
      {#if span}
        <div class="br live" style="left: {spineX + 12}px; top: {map.y(Math.min(span.a, span.b)).toFixed(1)}px; height: {Math.max(6, Math.abs(map.y(span.b) - map.y(span.a))).toFixed(1)}px"></div>
      {/if}
      {#if draft && draft.target.end_s !== null && draft.target.start_s !== null}
        <div class="br live" style="left: {spineX + 12}px; top: {map.y(draft.target.start_s).toFixed(1)}px; height: {Math.max(6, map.y(draft.target.end_s) - map.y(draft.target.start_s)).toFixed(1)}px"></div>
      {/if}

      {#each placed as p (p.item)}
        {#if p.item === DRAFT && draft}
          <div class="mn draft" style="left: {notesX}px; width: {notesW}px; top: {p.top.toFixed(0)}px" bind:clientHeight={heights[DRAFT]} data-testid="note-draft" data-tour="draft">
            <div class="dtop">
              <span class="av sm">{app.reviewer?.current?.initials ?? '?'}</span>
              <b>New note</b>
              {#if draft.target.time}<span class="tc tn">{draft.target.time}</span>{/if}
              <span class="muted ellip dq">{draft.target.speaker ? `${draft.target.speaker} · ` : ''}{draft.target.on === 'scene' ? 'the whole scene' : draft.target.quote}</span>
            </div>
            <textarea bind:this={field} bind:value={draft.text} onkeydown={fieldKey} aria-label="Your note" placeholder="What should change here?" data-testid="note-field"></textarea>
            <div class="dfoot">
              {#each TAGS as t (t.id)}<button class="tagsel" class:on={draft.tag === t.id} onmousedown={(e) => e.preventDefault()} onclick={() => draft && (draft.tag = draft.tag === t.id ? null : t.id)} data-testid="draft-tag-{t.id}">{t.label}</button>{/each}
              <span class="grow"></span>
              <button class="key" onmousedown={(e) => e.preventDefault()} onclick={save} aria-label="Add note" data-testid="note-save">↩</button>
            </div>
          </div>
        {:else}
          {@const thread = threadOf(p.item)}
          <div class="mn" class:held={p.item === MOVING} class:hot={thread.some((n) => n.id === app.noteFocus)} class:quiet={thread.every((n) => n.status === 'resolved')} style="left: {notesX}px; width: {notesW}px; top: {p.top.toFixed(0)}px" bind:clientHeight={heights[p.item]} data-testid="note-thread">
            {#each thread as n, i (n.id)}
              {#if i}<div class="sep"></div>{/if}
              <NoteCard note={n} onjump={goTo} ongrab={grabNote} />
            {/each}
          </div>
        {/if}
      {/each}
    </div>
  </div>

  <div class="line" style="right: {RAIL - 16}px" aria-hidden="true"></div>

  <div class="lead" style="left: {LEFT}px">
    <span class="nm">Sc. {v?.scene.number} · {v?.scene.place}</span>
    <span class="muted hint">Click the wave for a marker · drag along it for a range</span>
  </div>
  <div class="nhead" style="left: {notesX}px; width: {notesW}px" data-testid="notes-head" data-tour="notes">
    <b>Notes</b>
    <span class="muted count tn">{r?.notes.length ?? 0} · {open} open</span>
    <button class="plain" onclick={() => write(onScene())} title="A note on the whole scene" data-testid="scene-note">On this scene</button>
    <span class="segc" role="radiogroup" aria-label="Show">
      {#each [['all', 'All'], ['open', 'Open'], ['mine', 'Mine']] as [id, label] (id)}
        <button class:on={by === id} role="radio" aria-checked={by === id} onclick={() => (by = id as NoteFilter)} data-testid="filter-{id}">{label}</button>
      {/each}
    </span>
    <label class="kinds">
      {kind ? tagLabel(kind) : 'All kinds'}
      <select aria-label="Kind of note" value={kind ?? ''} onchange={(e) => (kind = (e.currentTarget.value || null) as NoteTag | null)} data-testid="filter-kind">
        <option value="">All kinds</option>
        {#each TAGS as t (t.id)}<option value={t.id}>{t.label}</option>{/each}
      </select>
    </label>
  </div>
  {#if r && !notes.length && !draft}
    <p class="empty muted" style="left: {notesX}px; width: {notesW}px" data-testid="notes-empty">{r.notes.length ? 'No notes match.' : 'No notes on this scene yet.'}</p>
  {/if}
</div>

<style>
  .review {
    position: absolute;
    inset: 0;
    overflow: hidden;
  }
  .rh {
    position: absolute;
    top: 18px;
    left: 24px;
    right: 24px;
    height: 40px;
    display: flex;
    align-items: center;
    gap: 12px;
    z-index: 15;
    white-space: nowrap;
  }
  .rv {
    flex: none;
    height: 24px;
    padding: 0 10px;
    border-radius: var(--radius-pill);
    display: inline-flex;
    align-items: center;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    background: rgb(var(--accent-rgb) / 0.2);
    color: var(--link);
    box-shadow: inset 0 0 0 1px rgb(var(--accent-rgb) / 0.5);
  }
  .ep {
    flex: 0 1 auto;
    min-width: 40px;
    font-weight: 600;
    font-size: 14px;
  }
  .rp {
    position: relative;
    flex: 0 1 auto;
    min-width: 60px;
  }
  .round {
    max-width: 100%;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    color: var(--text-3);
  }
  .round:hover {
    color: var(--text-1);
  }
  .scrim0 {
    position: fixed;
    inset: 0;
    z-index: 19;
  }
  .pop {
    position: absolute;
    top: 30px;
    left: 0;
    z-index: 20;
    width: 340px;
    padding: 6px;
    box-sizing: border-box;
    border-radius: 16px;
    background: var(--panel-solid);
    box-shadow: inset 0 0 0 1px var(--glass-outline), 0 30px 70px rgb(var(--shadow-rgb) / 0.6);
    white-space: normal;
  }
  .pop button {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 10px;
    border-radius: 11px;
    text-align: left;
    font-size: 12.5px;
  }
  .pop button:hover,
  .pop button.on {
    background: var(--glass-fill-4);
  }
  .none {
    margin: 8px 10px;
    font-size: 12.5px;
  }
  .mid {
    flex: 1 1 200px;
    min-width: 140px;
    display: flex;
    justify-content: center;
  }
  .me {
    flex: 0 1 auto;
    min-width: 60px;
    height: 34px;
    padding: 0 10px 0 4px;
    border-radius: var(--radius-pill);
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--glass-fill-3);
    font-size: 12.5px;
  }
  .me .name {
    font-weight: 600;
  }
  .av {
    flex: none;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    font-weight: 700;
    background: var(--accent);
    color: var(--text-on-pill);
  }
  .av.sm {
    width: 22px;
    height: 22px;
    font-size: 9px;
  }
  .btnx {
    flex: none;
    height: 32px;
    padding: 0 13px;
    border-radius: 9px;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-strong);
    background: var(--glass-hover);
  }
  .btnx.help {
    width: 32px;
    padding: 0;
    justify-content: center;
  }
  .btnx.pri {
    background: var(--accent);
    color: var(--text-on-pill);
  }
  @media (max-width: 1180px) {
    .role {
      display: none;
    }
  }
  @media (max-width: 980px) {
    .me .name {
      display: none;
    }
    .ep {
      display: none;
    }
  }

  .area {
    position: absolute;
    left: 0;
    top: 64px;
    bottom: 96px;
    overflow: hidden;
    -webkit-mask-image: linear-gradient(to bottom, transparent 0, #000 70px, #000 calc(100% - 40px), transparent 100%);
    mask-image: linear-gradient(to bottom, transparent 0, #000 70px, #000 calc(100% - 40px), transparent 100%);
  }
  .col {
    position: absolute;
    left: 0;
    top: 0;
    right: 0;
    height: 0;
  }
  .line {
    position: absolute;
    left: 0;
    top: 50%;
    height: 1px;
    background: linear-gradient(90deg, transparent, var(--stage-hairline) 8%, var(--stage-hairline) 92%, transparent);
    pointer-events: none;
  }
  .card {
    position: absolute;
    box-sizing: border-box;
    border-radius: 15px;
    padding: 0 14px;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 3px;
    cursor: pointer;
  }
  .ch {
    display: flex;
    gap: 8px;
    align-items: baseline;
  }
  .nm {
    font-weight: 600;
    font-size: 10.5px;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }
  .tcd {
    font-size: 10.5px;
    color: var(--text-5);
  }
  .txt {
    font-size: 14px;
    color: var(--text-2);
  }
  .addn {
    position: absolute;
    right: 8px;
    top: 8px;
    width: 22px;
    height: 22px;
    border-radius: 7px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-3);
    background: var(--glass-hover);
    opacity: 0;
  }
  .card:hover .addn,
  .addn:global([data-tour-lit]),
  .addn:focus-visible {
    opacity: 1;
  }
  .act {
    position: absolute;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    padding: 0 14px;
    font-size: 12.5px;
    font-style: italic;
    color: var(--text-4);
  }
  .gp {
    position: absolute;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 10.5px;
    color: var(--text-5);
  }
  .draw {
    position: absolute;
    left: 0;
    top: 0;
    overflow: visible;
    pointer-events: none;
  }
  .hit {
    position: absolute;
    top: 0;
    width: 52px;
    cursor: crosshair;
  }
  .snd {
    position: absolute;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: rgb(var(--bucket-2-rgb) / 0.75);
  }
  .snd:hover {
    background: rgb(var(--bucket-2-rgb));
    box-shadow: 0 0 0 3px rgb(var(--bucket-2-rgb) / 0.3);
  }
  .br {
    position: absolute;
    width: 4px;
    border-radius: 2px;
    background: var(--accent);
    pointer-events: none;
  }
  .br.done {
    opacity: 0.4;
  }
  .br.can {
    pointer-events: auto;
    cursor: grab;
  }
  /* wider to grab than to see */
  .br.can::before {
    content: '';
    position: absolute;
    inset: 0 -5px;
  }
  .dot {
    position: absolute;
    width: 12px;
    height: 12px;
    margin: -6px 0 0 -6px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 0 2px var(--stage-bg);
  }
  .dot.done {
    opacity: 0.4;
  }
  .dot.can {
    cursor: grab;
  }
  .br.live {
    background: var(--link);
    box-shadow: 0 0 12px rgb(var(--accent-rgb) / 0.7);
  }
  .mn {
    position: absolute;
    box-sizing: border-box;
    border-radius: 16px;
    padding: 11px 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: var(--panel-solid);
    box-shadow: inset 0 0 0 1px var(--glass-outline);
    transition: top 0.2s var(--ease-out);
  }
  .mn.quiet {
    background: var(--panel);
  }
  .mn.held {
    z-index: 4;
    transition: none;
    box-shadow: inset 0 0 0 1.5px rgb(var(--accent-rgb) / 0.8), 0 20px 50px rgb(var(--shadow-rgb) / 0.5);
  }
  .mn.hot {
    box-shadow: inset 0 0 0 1.5px rgb(var(--accent-rgb) / 0.8);
  }
  .sep {
    height: 1px;
    background: var(--glass-outline);
  }
  .mn.draft {
    gap: 6px;
    box-shadow: inset 0 0 0 1.5px rgb(var(--accent-rgb) / 0.8), 0 20px 50px rgb(var(--shadow-rgb) / 0.5);
    z-index: 3;
  }
  .dtop {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    font-size: 12.5px;
  }
  .dq {
    flex: 1;
    min-width: 0;
    font-size: 11.5px;
  }
  .tc {
    flex: none;
    font-size: 11px;
    font-weight: 600;
    padding: 2px 7px;
    border-radius: 6px;
    background: rgb(var(--accent-rgb) / 0.18);
    color: var(--link);
  }
  textarea {
    box-sizing: border-box;
    width: 100%;
    min-height: 56px;
    resize: none;
    border: 0;
    outline: none;
    border-radius: 12px;
    padding: 10px 12px;
    font: inherit;
    font-size: 13.5px;
    color: var(--text-strong);
    background: var(--glass-fill-3);
    box-shadow: inset 0 0 0 1.5px rgb(var(--accent-rgb) / 0.7);
    user-select: text;
    -webkit-user-select: text;
  }
  .dfoot {
    display: flex;
    gap: 5px;
    align-items: center;
  }
  .grow {
    flex: 1;
  }
  .tagsel {
    height: 24px;
    padding: 0 9px;
    border-radius: var(--radius-pill);
    display: inline-flex;
    align-items: center;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text-3);
    box-shadow: inset 0 0 0 1px var(--glass-outline);
  }
  .tagsel.on {
    background: rgb(var(--accent-rgb) / 0.22);
    color: var(--text-strong);
    box-shadow: none;
  }
  .key {
    display: inline-flex;
    align-items: center;
    height: 22px;
    padding: 0 7px;
    border-radius: 6px;
    background: var(--glass-hover);
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
  }
  .lead {
    position: absolute;
    top: 76px;
    display: flex;
    align-items: center;
    gap: 10px;
    z-index: 3;
    white-space: nowrap;
  }
  .lead .nm {
    color: var(--text-3);
  }
  .hint {
    font-size: 11.5px;
  }
  .nhead {
    position: absolute;
    top: 72px;
    display: flex;
    align-items: center;
    gap: 8px;
    z-index: 3;
    white-space: nowrap;
  }
  .nhead b {
    font-size: 14px;
  }
  .count {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 12px;
  }
  .plain {
    font-size: 11.5px;
    color: var(--text-4);
  }
  .plain:hover {
    color: var(--text-1);
  }
  .segc {
    display: inline-flex;
    padding: 2px;
    border-radius: 9px;
    background: var(--glass-fill-4);
  }
  .segc button {
    height: 24px;
    padding: 0 10px;
    border-radius: 7px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-3);
  }
  .segc button.on {
    background: var(--glass-active);
    color: var(--text-strong);
    font-weight: 600;
  }
  .kinds {
    position: relative;
    height: 24px;
    padding: 0 9px;
    border-radius: var(--radius-pill);
    display: inline-flex;
    align-items: center;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text-3);
    box-shadow: inset 0 0 0 1px var(--glass-outline);
    cursor: pointer;
  }
  .kinds select {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }
  .empty {
    position: absolute;
    top: 120px;
    margin: 0;
    font-size: 13px;
  }
</style>
