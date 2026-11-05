<script lang="ts">
  // The mapping editor (D104, D105), on the Options page: when the matcher got it wrong, you read
  // and listen along a character's mic and mark in and out. That stretch changes a take, or
  // becomes a new take or a new line, and the text field says what's said in it. It only reads
  // the recording; nothing changes until one of the buttons at the top.
  import { onDestroy, onMount, tick } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import { charColor, rgba } from '../lib/colors';
  import { around, clock, fit, MIN_MARK_S, pan, placeWords, textBetween, ticks, withTake, zoom, type Span } from '../lib/mapping';
  import type { MappingOpen, MappingVoice, PerformanceHit, PerformanceResults, Recording } from '../lib/types';
  import { colorOf, solid } from '../theme';
  import DeleteSheet from './DeleteSheet.svelte';
  import Icon from './Icon.svelte';
  import Waveform from './Waveform.svelte';

  const PREVIEW_ID = 'mapping-editor';
  /** How far playing from the locator runs on. */
  const LISTEN_S = 120;
  const WORD_ROWS = 3;
  const PX_PER_CHAR = 7;
  const STEP_S = 0.01;
  const BIG_STEP_S = 0.1;
  /** Marks this close to a take's own haven't moved. */
  const SAME_MARK_S = 0.002;

  const target = app.mapping;
  const line = target?.kind === 'take' ? app.lines.find((l) => l.line_id === target.line_id) : undefined;
  const adding = target?.kind === 'line';
  const takes = line?.takes ?? [];

  let opened = $state<MappingOpen | null>(null);
  let rec = $state<Recording | null>(null);
  let character = $state<string | null>(null);
  let view = $state<Span>({ t0: 0, t1: 14 });
  let shape = $state<{ t0: number; t1: number; peaks: number[] } | null>(null);
  let marks = $state<Span | null>(null);
  let locator = $state(0);
  /** Where the stretch being played starts, in the recording. */
  let playFrom = $state(0);

  let query = $state('');
  let found = $state<PerformanceResults | null>(null);
  let searched = $state('');
  let picked = $state<string | null>(null);
  /** The take being edited. */
  let editing = $state<string | null>(line?.take_id ?? null);
  /** What's said, once typed over. */
  let typed = $state<string | null>(null);
  let saving = $state(false);
  let asking = $state(false);

  let lane = $state<HTMLElement>();
  let laneW = $state(1000);
  let searchEl = $state<HTMLInputElement>();

  const voice = $derived<MappingVoice | undefined>(opened?.cast.find((c) => c.id === character));
  const color = $derived(charColor(app.view, character));
  const span = $derived(view.t1 - view.t0);
  const pxPerS = $derived(laneW / span);
  const pct = (t: number) => ((t - view.t0) / span) * 100;
  const timeAt = (clientX: number) => {
    const box = lane?.getBoundingClientRect();
    return box ? view.t0 + ((clientX - box.left) / box.width) * span : view.t0;
  };
  const dur = $derived(rec?.duration_s ?? 0);

  const current = $derived(takes.find((t) => t.id === editing));
  /** What's said on this mic: what was heard, with the take's own words over its stretch. */
  const said = $derived(rec ? (current?.source_id === rec.source_id ? withTake(rec.words, current) : rec.words) : []);
  const words = $derived(placeWords(said, view, pxPerS, PX_PER_CHAR, WORD_ROWS));
  const hidden = $derived(!!rec && rec.heard && pxPerS < 18);
  const rulerTicks = $derived(ticks(view, pxPerS));
  const takesHere = $derived((rec?.takes ?? []).filter((t) => t.t1 > view.t0 && t.t0 < view.t1));
  const own = (lineId: string) => target?.kind === 'take' && target.line_id === lineId;
  const takeNo = (id: string) => line?.takes.findIndex((t) => t.id === id) ?? -1;

  const between = $derived(marks ? textBetween(said, marks.t0, marks.t1) : '');
  const text = $derived(typed ?? between);
  const length = $derived(marks ? marks.t1 - marks.t0 : 0);
  const ready = $derived(!!rec && length >= MIN_MARK_S && (!adding || text.trim().length > 0) && !saving);
  const moved = $derived(!!current && !!marks && (rec?.source_id !== current.source_id || Math.abs(marks.t0 - current.src_in) > SAME_MARK_S || Math.abs(marks.t1 - current.src_out) > SAME_MARK_S));
  const retyped = $derived(!!current && text.trim() !== current.words.map((w) => w.text).join(' '));
  const canSave = $derived(ready && !!current && (moved || retyped));
  const currentNo = $derived(takes.findIndex((t) => t.id === editing) + 1);

  const playing = $derived(app.previewing(PREVIEW_ID));
  const head = $derived(playing && app.preview ? playFrom + app.preview.pos : null);

  async function loadRecording(id: string) {
    rec = await app.run(() => call<Recording>('recording', { source_id: id })) ?? null;
    shape = null;
  }

  onMount(async () => {
    const params = target?.kind === 'take' ? { line_id: target.line_id } : { scene: app.sceneId, after: target?.after ?? undefined };
    const o = await app.run(() => call<MappingOpen>('mapping_open', params));
    if (!o) return;
    opened = o;
    character = o.character;
    if (!o.source_id) return;
    await loadRecording(o.source_id);
    if (o.t0 != null && o.t1 != null) marks = { t0: o.t0, t1: o.t1 };
    view = marks ? around(marks.t0, marks.t1, dur) : around(o.at_s, o.at_s, dur);
    locator = marks?.t0 ?? o.at_s;
    // a new line starts from a search; a take's marks are already there, and I, O and Space work
    if (adding) {
      await tick();
      searchEl?.focus();
    }
  });

  onDestroy(() => {
    if (app.preview?.id === PREVIEW_ID) {
      if (app.preview.playing) void app.stopSound();
      app.preview = null;
    }
  });

  // the shape of what's in view, and of as much again either side, so scrolling finds it there
  let fetching = 0;
  $effect(() => {
    const id = rec?.source_id;
    const { t0, t1 } = view;
    const w = laneW;
    if (!id || !dur) return;
    if (shape && t0 >= shape.t0 && t1 <= shape.t1 && (t1 - t0) / (shape.t1 - shape.t0) > 0.25) return;
    const n = ++fetching;
    const wait = setTimeout(async () => {
      const a = Math.max(0, t0 - (t1 - t0));
      const b = Math.min(dur, t1 + (t1 - t0));
      const peaks = await app.run(() => call<number[]>('peaks', { source_id: id, t0: a, t1: b, buckets: Math.round(w * 1.5), normalize: true }));
      if (n === fetching && peaks) shape = { t0: a, t1: b, peaks };
    }, 40);
    return () => clearTimeout(wait);
  });

  // what's playing stays in view
  $effect(() => {
    if (head == null || (head <= view.t1 - span * 0.05 && head >= view.t0)) return;
    const next = pan(view, head - view.t0 - span * 0.1, dur);
    // at the end of the file it can't move on
    if (next.t0 !== view.t0) view = next;
  });

  // ---- search ----

  $effect(() => {
    const q = query.trim();
    const who = character;
    if (!q) {
      found = null;
      searched = '';
      return;
    }
    const wait = setTimeout(async () => {
      const r = await app.run(() => call<PerformanceResults>('search_performances', { query: q, character: who }));
      if (r && query.trim() === q) {
        found = r;
        searched = q;
      }
    }, 200);
    return () => clearTimeout(wait);
  });

  const whoLabel = $derived(voice?.label ?? 'anyone');

  async function useHit(h: PerformanceHit) {
    picked = h.id;
    searchEl?.blur();
    if (h.character !== character) character = h.character;
    if (h.source_id !== rec?.source_id) await loadRecording(h.source_id);
    marks = { t0: h.t0, t1: h.t1 };
    locator = h.t0;
    typed = null;
    view = around(h.t0, h.t1, dur);
  }

  async function pickVoice(c: MappingVoice) {
    if (c.id === character) return;
    character = c.id;
    picked = null;
    const mic = c.mics[0];
    if (mic && mic.id !== rec?.source_id) await pickMic(mic.id);
  }

  /** Another mic, at the moment in view when it's of the same session. */
  async function pickMic(id: string) {
    const mics = opened?.cast.flatMap((c) => c.mics) ?? [];
    const here = mics.find((m) => m.id === rec?.source_id);
    const there = mics.find((m) => m.id === id);
    const mid = (view.t0 + view.t1) / 2;
    const at = here && there && here.session_id && here.session_id === there.session_id ? mid + there.offset_s - here.offset_s : mid;
    await loadRecording(id);
    marks = null;
    view = around(at, at, dur);
    locator = at;
  }

  // ---- marking ----

  const clampIn = (t: number) => Math.max(0, Math.min(t, dur));

  function setIn(t: number) {
    const a = clampIn(t);
    const end = marks?.t1 ?? Math.min(a + 2, dur);
    marks = { t0: Math.min(a, end - MIN_MARK_S), t1: Math.max(end, a + MIN_MARK_S) };
  }

  function setOut(t: number) {
    const b = clampIn(t);
    const start = marks?.t0 ?? Math.max(b - 2, 0);
    marks = { t0: Math.min(start, b - MIN_MARK_S), t1: Math.max(b, start + MIN_MARK_S) };
  }

  function dragEdge(e: MouseEvent, side: 'in' | 'out') {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const move = (ev: MouseEvent) => (side === 'in' ? setIn(timeAt(ev.clientX)) : setOut(timeAt(ev.clientX)));
    const up = () => {
      window.removeEventListener('mousemove', move);
      window.removeEventListener('mouseup', up);
    };
    window.addEventListener('mousemove', move);
    window.addEventListener('mouseup', up);
  }

  function edgeKey(e: KeyboardEvent, side: 'in' | 'out') {
    const by = e.shiftKey ? BIG_STEP_S : STEP_S;
    const d = e.key === 'ArrowLeft' ? -by : e.key === 'ArrowRight' ? by : 0;
    if (!d || !marks) return;
    e.preventDefault();
    e.stopPropagation();
    if (side === 'in') setIn(marks.t0 + d);
    else setOut(marks.t1 + d);
  }

  /** A click places the locator, a drag marks a stretch, ⇧ moves the nearer mark to the click. */
  function laneDown(e: MouseEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    (document.activeElement as HTMLElement | null)?.blur();
    const from = clampIn(timeAt(e.clientX));
    if (e.shiftKey && marks) {
      if (Math.abs(from - marks.t0) < Math.abs(from - marks.t1)) setIn(from);
      else setOut(from);
      return;
    }
    let moved = false;
    const x0 = e.clientX;
    const move = (ev: MouseEvent) => {
      if (!moved && Math.abs(ev.clientX - x0) < 4) return;
      moved = true;
      const to = clampIn(timeAt(ev.clientX));
      marks = { t0: Math.min(from, to), t1: Math.max(from, to) };
      picked = null;
    };
    const up = () => {
      window.removeEventListener('mousemove', move);
      window.removeEventListener('mouseup', up);
      if (!moved) {
        locator = from;
        if (playing) void listen(from, Math.min(from + LISTEN_S, dur));
      }
    };
    window.addEventListener('mousemove', move);
    window.addEventListener('mouseup', up);
  }

  /** A word double-clicked is marked. */
  function laneDouble(e: MouseEvent) {
    const t = timeAt(e.clientX);
    const w = said.find((x) => t >= x.t0 && t <= x.t1);
    if (w) marks = { t0: w.t0, t1: w.t1 };
  }

  function overviewDown(e: MouseEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const go = (ev: MouseEvent) => {
      const t = ((ev.clientX - box.left) / box.width) * dur;
      view = fit({ t0: t - span / 2, t1: t + span / 2 }, dur);
    };
    go(e);
    const up = () => {
      window.removeEventListener('mousemove', go);
      window.removeEventListener('mouseup', up);
    };
    window.addEventListener('mousemove', go);
    window.addEventListener('mouseup', up);
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    if (e.altKey || e.metaKey || e.ctrlKey) {
      view = zoom(view, timeAt(e.clientX), Math.exp(e.deltaY * 0.004), dur);
      return;
    }
    const d = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
    view = pan(view, (d / laneW) * span, dur);
  }

  // Svelte's own wheel handlers are passive: this one has to stop the page's
  $effect(() => {
    const el = lane;
    if (!el) return;
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  });

  // ---- listening ----

  async function listen(t0: number, t1: number) {
    if (!rec || t1 - t0 < MIN_MARK_S) return;
    playFrom = t0;
    const clip = { id: PREVIEW_ID, source_id: rec.source_id, t0: +t0.toFixed(3), t1: +t1.toFixed(3), text: '' };
    await app.playSound({ clip }, 0, { name: rec.name, duration_s: t1 - t0, peaks: [] });
  }

  function playMarks() {
    if (playing) return void app.stopSound();
    if (marks) return void listen(marks.t0, marks.t1);
    playLocator();
  }

  function playLocator() {
    if (playing) return void app.stopSound();
    void listen(locator, Math.min(locator + LISTEN_S, dur));
  }

  // ---- done ----

  function close() {
    app.overlay = null;
    app.mapping = null;
  }

  const r3 = (t: number) => +t.toFixed(3);

  async function create() {
    if (!ready || !marks || !rec) return;
    saving = true;
    const { t0, t1 } = marks;
    const id = rec.source_id;
    const ok =
      target?.kind === 'take'
        ? await app.addTake(target.line_id, id, r3(t0), r3(t1), text.trim() || undefined)
        : await app.addLine(target?.after ?? null, character ?? '', id, r3(t0), r3(t1), text.trim());
    saving = false;
    if (ok) close();
  }

  async function save() {
    if (!canSave || !marks || !rec || !current) return;
    saving = true;
    const stretch = moved ? { source_id: rec.source_id, t0: r3(marks.t0), t1: r3(marks.t1) } : {};
    const ok = await app.editTake(current.id, { ...stretch, text: text.trim() || undefined });
    saving = false;
    if (ok) close();
  }

  /** Moves the editor onto another take of the line. */
  async function editOther(id: string) {
    const t = takes.find((x) => x.id === id);
    if (!t) return;
    editing = id;
    typed = null;
    picked = null;
    if (t.source_id !== rec?.source_id) await loadRecording(t.source_id);
    marks = { t0: t.src_in, t1: t.src_out };
    locator = t.src_in;
    view = around(t.src_in, t.src_out, dur);
  }

  function onKey(e: KeyboardEvent) {
    if (asking) return;
    const t = e.target;
    const typing = t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement;
    const k = e.key;
    let handled = true;
    if (k === 'Escape') {
      if (t === searchEl && query) query = '';
      else if (typing) (t as HTMLElement).blur();
      else close();
    } else if (typing) {
      if (k !== 'Enter') return;
      if (e.metaKey || e.ctrlKey) void save();
      else if (t === searchEl && found?.hits[0] && picked !== found.hits[0].id) void useHit(found.hits[0]);
      else void create();
    } else if (t instanceof Element && t.hasAttribute('data-own-arrows') && (k === 'ArrowLeft' || k === 'ArrowRight')) return;
    else if (t instanceof HTMLButtonElement && t.matches(':focus-visible') && (k === 'Enter' || k === ' ')) return;
    else if (k === ' ' && e.shiftKey) playLocator();
    else if (k === ' ') playMarks();
    else if (k === 'Enter' && (e.metaKey || e.ctrlKey)) void save();
    else if (k === 'Enter') void create();
    else if (k === 'i' || k === 'I') setIn(head ?? locator);
    else if (k === 'o' || k === 'O') setOut(head ?? locator);
    else if (k === '/' || ((k === 'f' || k === 'F') && (e.metaKey || e.ctrlKey))) searchEl?.focus();
    else if (k === 'ArrowLeft' || k === 'ArrowRight') {
      const d = (k === 'ArrowLeft' ? -1 : 1) * (e.shiftKey ? 1 : 0.1);
      locator = clampIn(locator + d);
      if (locator < view.t0 || locator > view.t1) view = pan(view, d * 4, dur);
    } else handled = false;
    if (handled) {
      e.preventDefault();
      e.stopPropagation();
    }
  }

  onMount(() => {
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  });
</script>

{#snippet edge(side: 'in' | 'out', t: number)}
  <button class="edge" style="left: {pct(t)}%; color: {color.name}" role="slider" data-own-arrows aria-label="{side === 'in' ? 'In' : 'Out'} point" aria-valuemin={0} aria-valuemax={dur} aria-valuenow={+t.toFixed(2)} aria-valuetext={clock(t)} onmousedown={(e) => dragEdge(e, side)} onkeydown={(e) => edgeKey(e, side)} data-testid="mark-{side}"><span>{side === 'in' ? 'I' : 'O'}</span></button>
{/snippet}

<div class="dim" aria-hidden="true"></div>
<div class="glass ed" role="dialog" aria-modal="true" aria-label={adding ? 'New line' : 'Mapping editor'} data-testid="mapping-editor" style="background: linear-gradient(160deg, {rgba(color.rgb, 0.16)} 0%, var(--panel) 32%, var(--panel-solid) 100%)">
  <div class="head">
    <div class="title">
      <div class="r">
        <span class="nm" style="font-size: 12px; color: {color.name}">{voice?.label ?? ''}</span>
        <span class="info">{adding ? 'New line' : `Line ${line?.number ?? ''}`}</span>
        {#if takes.length}
          <div class="tks" role="group" aria-label="Take to edit">
            {#each takes as t, i (t.id)}
              <button class="tk" class:on={t.id === editing} onclick={() => editOther(t.id)} aria-label="Edit take {i + 1}" data-testid="mapping-take">{i + 1}</button>
            {/each}
          </div>
        {/if}
      </div>
      <textarea class="said script" rows="2" value={text} oninput={(e) => (typed = e.currentTarget.value)} placeholder={marks ? 'What’s said here' : 'Find it below, then mark in and out'} aria-label="What’s said" data-testid="line-text"></textarea>
      <div class="under">
        {#if line}<span class="ref ellip" title={line.text} data-testid="script-text"><span class="lab">Script</span> {line.text}</span>{/if}
        {#if typed != null && typed !== between}<button class="refill" onclick={() => (typed = null)} data-testid="use-heard">Use what’s heard</button>{/if}
      </div>
    </div>
    <div class="btns">
      {#if current}<button class="b" onclick={() => (asking = true)} data-testid="mapping-delete">Delete…</button>{/if}
      <button class="b" onclick={close}>Cancel <span class="kb">Esc</span></button>
      {#if current}<button class="b strong" disabled={!canSave} onclick={save} data-testid="mapping-save">Save take {currentNo} <span class="kb">⌘⏎</span></button>{/if}
      <button class="done" disabled={!ready} onclick={create} data-testid="mapping-create">{adding ? 'Add line' : current ? 'Save as new take' : 'Create take'} <span class="kb">⏎</span></button>
    </div>
  </div>

  <div class="find">
    <label class="bar">
      <Icon name="search" size={14} />
      <input bind:this={searchEl} bind:value={query} placeholder="Search what {voice?.label ?? 'they'} said" aria-label="Search the recording" data-testid="mapping-search" />
    </label>
    <div class="voices" role="group" aria-label="Whose mic">
      {#each opened?.cast ?? [] as c (c.id)}
        <button class="voice" class:on={c.id === character} onclick={() => pickVoice(c)} data-testid="mapping-voice"><span class="dot" style="background: {solid(colorOf(c.color_index).rgb)}"></span>{c.label}</button>
      {/each}
    </div>
    {#if voice && voice.mics.length > 1}
      <select class="mic" aria-label="Recording" value={rec?.source_id} onchange={(e) => pickMic(e.currentTarget.value)}>
        {#each voice.mics as m (m.id)}<option value={m.id}>{m.name}</option>{/each}
      </select>
    {/if}
  </div>

  {#if searched}
    <div class="hits" role="group" aria-label="Found">
      {#each found?.hits ?? [] as h (h.id)}
        <button class="hit" class:on={picked === h.id} onclick={() => useHit(h)} data-testid="mapping-hit">
          <span class="at">{clock(h.t0)}</span>
          <span class="said-it ellip"><span class="ctx">{h.before ? `${h.before} ` : ''}</span><b>{h.text || h.tag}</b><span class="ctx">{h.after ? ` ${h.after}` : ''}</span></span>
          <span class="where">{h.at}{h.close ? ' · sounds close' : ''}</span>
        </button>
      {:else}
        <p class="none">{found?.heard ? `Nothing like that from ${whoLabel}.` : 'No transcripts to search yet: run the analysis first.'}</p>
      {/each}
    </div>
  {/if}

  {#if opened && !opened.source_id}
    <p class="none empty">No recording for this character. Pick another voice above, or import their mic first.</p>
  {:else}
    {#if rec}
      <div class="over" role="slider" tabindex="-1" aria-label="The whole recording" aria-valuemin={0} aria-valuemax={dur} aria-valuenow={Math.round(view.t0)} onmousedown={overviewDown} data-testid="mapping-overview">
        <Waveform peaks={rec.overview} fill={rgba(color.rgb, 0.5)} />
        {#each rec.takes.filter((t) => own(t.line_id)) as t (t.id)}<span class="otake" style="left: {(t.t0 / dur) * 100}%; background: {color.name}"></span>{/each}
        <span class="port" style="left: {(view.t0 / dur) * 100}%; width: {(span / dur) * 100}%"></span>
      </div>
    {/if}

    <div class="lanes">
      <div class="ruler">
        {#each rulerTicks as t (t)}<span class="tick" style="left: {pct(t)}%">{clock(t).replace(/\.00$/, '')}</span>{/each}
      </div>
      <!-- svelte-ignore a11y_no_static_element_interactions (the mouse's; the keys are I, O and the marks) -->
      <div class="lane" bind:this={lane} bind:clientWidth={laneW} onmousedown={laneDown} ondblclick={laneDouble} data-testid="mapping-lane">
        {#if shape}
          <div class="wave" style="left: {pct(shape.t0)}%; width: {((shape.t1 - shape.t0) / span) * 100}%"><Waveform peaks={shape.peaks} fill={rgba(color.rgb, 0.8)} gain={1.4} /></div>
        {/if}
        {#each takesHere as t (t.id)}
          {@const mine = own(t.line_id)}
          <span class="take" class:mine title={mine ? `Take ${takeNo(t.id) + 1}` : t.text} style="left: {pct(t.t0)}%; width: {((t.t1 - t.t0) / span) * 100}%; {mine ? `background: ${rgba(color.rgb, 0.16)}; box-shadow: inset 0 2px 0 ${color.name}` : ''}" data-testid={mine ? 'mapping-own-take' : undefined}>{#if mine}<button class="tno" onmousedown={(e) => e.stopPropagation()} onclick={() => editOther(t.id)} aria-label="Edit take {takeNo(t.id) + 1}">T{takeNo(t.id) + 1}</button>{/if}</span>
        {/each}
        {#if marks}
          <span class="out" style="left: 0; width: {Math.max(0, pct(marks.t0))}%"></span>
          <span class="out" style="left: {Math.min(100, pct(marks.t1))}%; right: 0"></span>
        {/if}
        {#each words as w (w.t0)}
          <span class="w" class:on={head != null && head >= w.t0 && head <= w.t1} class:in={!!marks && w.t0 < marks.t1 && w.t1 > marks.t0} style="left: {pct(w.t0)}%; bottom: {6 + w.row * 18}px" data-testid="mapping-word">{w.text}</span>
        {/each}
        {#if marks}
          {@render edge('in', marks.t0)}
          {@render edge('out', marks.t1)}
        {/if}
        <span class="loc" style="left: {pct(locator)}%"></span>
        {#if head != null}<span class="ph" style="left: {pct(head)}%"></span>{/if}
        {#if hidden}<span class="hint">Zoom in to read the words</span>{/if}
        {#if rec && !rec.heard}<span class="hint">No transcript for this mic: listen and mark by the waveform</span>{/if}
      </div>
    </div>

    <div class="foot">
      <div class="keys">
        <span class="tn" data-testid="mapping-marks">{marks ? `${clock(marks.t0)} – ${clock(marks.t1)} · ${length.toFixed(2)} s` : 'Nothing marked yet'}</span>
        <span><span class="kb">I</span> <span class="kb">O</span> in, out</span>
        <span>Drag to mark, <span class="kb">⇧</span> click moves a mark</span>
        <span><span class="kb">⇧</span> <span class="kb">Space</span> from locator</span>
        <span><span class="kb">⌥</span> scroll zooms</span>
      </div>
      <button class="playb" onclick={playMarks} data-testid="mapping-play" style="background: {rgba(color.rgb, 0.22)}; box-shadow: inset 0 0 0 1px {rgba(color.rgb, 0.45)}">
        <span class="pc"><Icon name={playing ? 'stop' : 'play'} size={11} /></span>
        {marks ? 'Play marked' : 'Play'} <span class="kb">Space</span>
      </button>
    </div>
  {/if}
</div>

{#if asking && line}<DeleteSheet lineId={line.line_id} takeId={editing} onclose={() => (asking = false)} ondone={close} />{/if}

<style>
  .dim {
    position: absolute;
    inset: 0;
    z-index: 20;
    background: var(--scrim);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
  }
  .ed {
    position: absolute;
    z-index: 21;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    width: min(1220px, 96%);
    max-height: 92vh;
    overflow-y: auto;
    box-sizing: border-box;
    padding: 24px 28px 20px;
    border-radius: var(--radius-dialog);
    display: flex;
    flex-direction: column;
    gap: 14px;
    box-shadow:
      inset 0 1px 0 rgb(var(--veil-rgb) / 0.12),
      inset 0 0 0 1px rgb(var(--veil-rgb) / 0.05),
      0 40px 120px rgb(var(--shadow-rgb) / 0.6);
  }
  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 20px;
    flex-wrap: wrap;
  }
  .title {
    min-width: 0;
    flex: 1 1 520px;
  }
  .r {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .info {
    font-size: 12px;
    color: var(--text-3);
  }
  .said {
    display: block;
    width: 100%;
    box-sizing: border-box;
    margin-top: 6px;
    padding: 6px 10px;
    font-size: 19px;
    line-height: 1.4;
    color: var(--text-strong);
    border: 0;
    border-radius: 10px;
    background: var(--glass-fill-2);
    resize: none;
    user-select: text;
    -webkit-user-select: text;
  }
  .under {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-height: 22px;
    margin-top: 4px;
  }
  .ref {
    min-width: 0;
    font-size: 12px;
    color: var(--text-3);
  }
  .lab {
    margin-right: 4px;
    font-size: 10px;
    font-weight: 600;
    color: var(--text-4);
  }
  .refill {
    flex: none;
    height: 22px;
    padding: 0 10px;
    border-radius: var(--radius-pill);
    font-size: 11px;
    color: var(--text-2);
    box-shadow: inset 0 0 0 1px var(--text-5);
  }
  .tks {
    display: flex;
    gap: 4px;
  }
  .tk {
    min-width: 24px;
    height: 22px;
    padding: 0 6px;
    border-radius: var(--radius-pill);
    font-size: 11px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--text-3);
    background: var(--glass-fill-2);
  }
  .tk.on {
    color: var(--text-on-light);
    background: var(--button-light);
  }
  .btns {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
  }
  .b {
    height: 36px;
    padding: 0 14px;
    border-radius: var(--radius-pill);
    font-size: 12.5px;
    color: var(--text-2);
    background: var(--glass-fill-3);
  }
  .b.strong {
    color: var(--text-strong);
    font-weight: 600;
  }
  .b:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .done {
    height: 40px;
    padding: 0 20px;
    border-radius: var(--radius-pill);
    font-size: var(--fs-ui);
    font-weight: 600;
    background: var(--button-light);
    color: var(--text-on-light);
  }
  .done:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .done .kb {
    background: rgb(var(--shadow-rgb) / 0.1);
    color: var(--text-on-light);
  }
  .find {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .bar {
    flex: 1 1 320px;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 12px;
    border-radius: var(--radius-pill);
    background: var(--glass-fill-2);
    color: var(--text-3);
  }
  .bar input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: none;
    font-size: 13px;
    color: var(--text-1);
    user-select: text;
    -webkit-user-select: text;
  }
  .voices {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .voice {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
    border-radius: var(--radius-pill);
    font-size: 11.5px;
    color: var(--text-3);
    background: var(--glass-fill-1);
  }
  .voice.on {
    color: var(--text-strong);
    font-weight: 600;
    background: rgb(var(--ink-rgb) / 0.14);
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  .mic {
    height: 28px;
    max-width: 220px;
    font-size: 11.5px;
    color: var(--text-2);
    background: var(--glass-fill-2);
    border: 0;
    border-radius: 8px;
  }
  .hits {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 132px;
    overflow-y: auto;
  }
  .hit {
    display: grid;
    grid-template-columns: 64px minmax(0, 1fr) auto;
    gap: 10px;
    align-items: center;
    flex: none;
    height: 30px;
    padding: 0 10px;
    border-radius: 8px;
    text-align: left;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .hit:hover {
    background: var(--glass-hover);
  }
  .hit.on {
    background: var(--glass-fill-3);
  }
  .at,
  .where {
    font-size: 11px;
    color: var(--text-4);
    font-variant-numeric: tabular-nums;
  }
  .said-it {
    min-width: 0;
  }
  .said-it b {
    font-weight: 600;
    color: var(--text-strong);
  }
  .ctx {
    color: var(--text-4);
    white-space: pre;
  }
  .none {
    margin: 4px 2px;
    font-size: 12px;
    color: var(--text-4);
  }
  .none.empty {
    padding: 40px 0;
    text-align: center;
  }
  .over {
    position: relative;
    height: 26px;
    flex: none;
    overflow: hidden;
    border-radius: 6px;
    background: var(--glass-fill-1);
    cursor: pointer;
  }
  .otake {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    pointer-events: none;
  }
  .port {
    position: absolute;
    top: 0;
    bottom: 0;
    min-width: 3px;
    box-sizing: border-box;
    border-radius: 4px;
    background: rgb(var(--ink-rgb) / 0.1);
    box-shadow: inset 0 0 0 1px rgb(var(--ink-rgb) / 0.55);
    pointer-events: none;
  }
  .lanes {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .ruler {
    position: relative;
    height: 16px;
    overflow: hidden;
  }
  .tick {
    position: absolute;
    top: 0;
    padding-left: 4px;
    border-left: 1px solid var(--text-5);
    font-size: 10px;
    line-height: 16px;
    color: var(--text-4);
    font-variant-numeric: tabular-nums;
  }
  .lane {
    position: relative;
    height: 200px;
    overflow: hidden;
    border-radius: 12px;
    background: var(--glass-fill-1);
    cursor: crosshair;
  }
  .wave {
    position: absolute;
    top: 8px;
    bottom: 62px;
    pointer-events: none;
  }
  .take {
    position: absolute;
    top: 0;
    bottom: 0;
    padding: 4px 6px;
    box-sizing: border-box;
    font-size: 10px;
    font-weight: 700;
    color: var(--text-2);
    background: rgb(var(--ink-rgb) / 0.035);
    box-shadow: inset 1px 0 0 rgb(var(--ink-rgb) / 0.1);
    pointer-events: none;
  }
  .tno {
    font: inherit;
    color: inherit;
    pointer-events: auto;
    cursor: pointer;
  }
  .out {
    position: absolute;
    top: 0;
    bottom: 0;
    background: var(--bar-strong);
    opacity: 0.55;
    pointer-events: none;
  }
  .w {
    position: absolute;
    font-size: 12px;
    line-height: 16px;
    white-space: nowrap;
    color: var(--text-4);
    pointer-events: none;
  }
  .w.in {
    color: var(--text-1);
  }
  .w.on {
    color: var(--text-strong);
    font-weight: 700;
  }
  .edge {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 15px;
    margin-left: -7.5px;
    cursor: ew-resize;
  }
  .edge::after {
    content: '';
    position: absolute;
    left: 6px;
    top: 0;
    bottom: 0;
    width: 3px;
    background: currentColor;
  }
  .edge span {
    position: absolute;
    left: 0;
    top: 0;
    width: 15px;
    height: 15px;
    border-radius: 0 0 5px 5px;
    background: currentColor;
    font-size: 9px;
    line-height: 15px;
    font-weight: 700;
    text-align: center;
  }
  .edge span {
    color: var(--text-on-pill);
  }
  .edge:focus-visible {
    outline: 2px solid var(--text-strong);
    outline-offset: 1px;
    border-radius: 4px;
  }
  .loc,
  .ph {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    pointer-events: none;
  }
  .loc {
    background: rgb(var(--ink-rgb) / 0.5);
  }
  .ph {
    background: var(--text-strong);
    box-shadow: 0 0 8px rgb(var(--ink-rgb) / 0.6);
  }
  .hint {
    position: absolute;
    left: 12px;
    bottom: 8px;
    font-size: 11px;
    color: var(--text-4);
    pointer-events: none;
  }
  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }
  .keys {
    display: flex;
    align-items: center;
    gap: 18px;
    flex-wrap: wrap;
    font-size: 11.5px;
    color: var(--text-4);
  }
  .tn {
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .playb {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 38px;
    padding: 0 16px 0 8px;
    border-radius: var(--radius-pill);
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-1);
  }
  .pc {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--button-light);
    color: var(--text-on-light);
  }
</style>
