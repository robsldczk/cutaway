<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { clamp, firstSentence } from '../lib/format';
  import { charColor, charLabel, rgba, shade } from '../lib/colors';
  import Waveform from './Waveform.svelte';
  import type { CompSegment, ItemView, TakeView } from '../lib/types';
  import { lineText, lineWords, takeWords, type ShownWord } from '../lib/words';

  const it = $derived(app.cur as ItemView);
  const color = $derived(charColor(app.view, it?.character ?? null));
  const k = $derived(it?.take_index ?? 0);
  const sh = $derived(shade(color.rgb, k));
  const boxDur = $derived(Math.max(0.05, (it?.end_s ?? 0) - (it?.start_s ?? 0)));
  const nScript = $derived(Math.max(1, it?.tokens.length ?? 1));

  let mode = $state<'comp' | 'edit'>('comp');
  /** What Space plays: the comp, or one take alone (prototype `edSrc`). */
  let src = $state<'comp' | string>('comp');
  const srcTake = $derived(src === 'comp' ? undefined : it?.takes.find((t) => t.id === src));
  let showWords = $state(true);
  let selSeg = $state<number | null>(null);
  let selWords = $state<number[]>([]);
  let paint = $state<{ lane: number; a: number; b: number } | null>(null);
  let laneW = $state(1);
  let areaEl: HTMLDivElement | undefined = $state();

  const takeIdx = (id: string) => it.takes.findIndex((t) => t.id === id);
  const shadeOf = (id: string) => shade(color.rgb, Math.max(0, takeIdx(id)));

  /** The comp as script-word segments (the whole selected take when there is none). */
  const comp = $derived.by((): CompSegment[] => {
    if (!it) return [];
    if (it.comp.length) return it.comp;
    const t = it.takes[k];
    return t ? [{ take_id: t.id, from_word: 0, to_word: nScript, nudge_s: 0 }] : [];
  });

  const auditioning = $derived(!!app.audition && app.audition === srcTake?.id);
  const edPos = $derived(
    auditioning && srcTake ? clamp(app.pos / srcTake.duration, 0, 1) : clamp((app.pos - (it?.start_s ?? 0)) / boxDur, 0, 1),
  );
  const edLoc = $derived(clamp((app.locator - (it?.start_s ?? 0)) / boxDur, 0, 1));

  // comp lane pieces in box-relative fractions
  const pieces = $derived.by(() => {
    if (!it) return [];
    return it.pieces.map((p) => {
      const t = it.takes.find((x) => x.id === p.take_id);
      const a = (p.start_s - it.start_s) / boxDur;
      const w = p.len_s / boxDur;
      const seg = comp.find((c) => c.take_id === p.take_id);
      return { p, t, a, w, nudge: seg?.nudge_s ?? 0 };
    });
  });

  const seams = $derived.by(() => {
    const out: { x: number; warn: boolean }[] = [];
    for (let i = 1; i < pieces.length; i++) {
      const prev = pieces[i - 1];
      const next = pieces[i];
      const inWord = (t: TakeView | undefined, srcT: number) => !!t && t.words.some((w) => srcT - t.src_in > w.t0 + 0.003 && srcT - t.src_in < w.t1 - 0.003);
      out.push({ x: next.a, warn: inWord(prev.t, prev.p.src_out) || inWord(next.t, next.p.src_in) });
    }
    return out;
  });

  const edText = $derived(it ? lineText(it) : '');
  // the words as they were said
  const edWords = $derived(it ? lineWords(it) : []);
  const takeOfWord = (w: ShownWord) => it?.words.find((x) => x.idx === w.idxs[0])?.take_id ?? it?.take_id ?? '';
  const ticks = $derived.by(() => {
    const out: { x: number; label: string }[] = [];
    for (let t = 0; t <= boxDur + 0.001; t += 0.5) out.push({ x: t / boxDur, label: t.toFixed(1) });
    return out;
  });

  function usedBars(t: TakeView) {
    return comp
      .filter((c) => c.take_id === t.id)
      .map((c) => {
        const ws = t.words.filter((w) => w.script_idx != null && w.script_idx >= c.from_word && w.script_idx < c.to_word);
        if (!ws.length) return null;
        return { a: ws[0].t0 / t.duration, b: ws[ws.length - 1].t1 / t.duration };
      })
      .filter((x): x is { a: number; b: number } => !!x);
  }

  /** Replaces script words [a, b) of the comp with take `id` (prototype `applyPaint`). */
  function applyPaint(id: string, a: number, b: number): CompSegment[] {
    const out: CompSegment[] = [];
    for (const g of comp) {
      if (g.to_word <= a || g.from_word >= b) {
        out.push({ ...g });
        continue;
      }
      if (g.from_word < a) out.push({ ...g, to_word: a });
      if (g.to_word > b) out.push({ ...g, from_word: b });
    }
    out.push({ take_id: id, from_word: a, to_word: b, nudge_s: 0 });
    out.sort((x, y) => x.from_word - y.from_word);
    const merged: CompSegment[] = [];
    for (const g of out) {
      if (g.to_word <= g.from_word) continue;
      const p = merged[merged.length - 1];
      if (p && p.take_id === g.take_id && Math.abs(p.nudge_s - g.nudge_s) < 1e-6 && p.to_word === g.from_word) p.to_word = g.to_word;
      else merged.push(g);
    }
    return merged;
  }

  async function setComp(segs: CompSegment[]) {
    if (segs.length === 1 && segs[0].nudge_s === 0 && segs[0].from_word === 0 && segs[0].to_word >= nScript) {
      if (segs[0].take_id !== it.take_id) await app.cmd({ cmd: 'select_take', line_id: it.line_id, take_id: segs[0].take_id });
      else await app.cmd({ cmd: 'reset_comp', line_id: it.line_id });
      return;
    }
    await app.cmd({ cmd: 'set_comp', line_id: it.line_id, segments: segs });
  }

  function frac(e: MouseEvent, el: HTMLElement) {
    const r = el.getBoundingClientRect();
    laneW = r.width;
    return clamp((e.clientX - r.left) / r.width, 0, 1);
  }

  function track(move: (ev: MouseEvent) => void, up: (ev: MouseEvent) => void) {
    const mv = (ev: MouseEvent) => move(ev);
    const u = (ev: MouseEvent) => {
      window.removeEventListener('mousemove', mv);
      window.removeEventListener('mouseup', u);
      up(ev);
    };
    window.addEventListener('mousemove', mv);
    window.addEventListener('mouseup', u);
  }

  function laneDown(e: MouseEvent, lane: number) {
    e.preventDefault();
    const el = e.currentTarget as HTMLElement;
    const f0 = frac(e, el);
    const x0 = e.clientX;
    let painting = false;
    track(
      (ev) => {
        if (!painting && Math.abs(ev.clientX - x0) > 4) painting = true;
        if (painting) paint = { lane, a: f0, b: frac(ev, el) };
      },
      async () => {
        const p = paint;
        paint = null;
        const t = it.takes[lane];
        if (!painting || !p || !t) {
          // click: listen to this take from here, and put the locator at the same script word
          src = t.id;
          void app.playTake(t.id, f0 * t.duration);
          const at = f0 * t.duration;
          const w = t.words.find((x) => at >= x.t0 && at <= x.t1);
          const lw = w?.script_idx != null ? it.words.find((x) => x.script_idx === w.script_idx) : undefined;
          app.locator = lw ? lw.start_s : it.start_s + f0 * boxDur;
          return;
        }
        const [a, b] = [Math.min(p.a, p.b) * t.duration, Math.max(p.a, p.b) * t.duration];
        const idx = t.words.filter((w) => w.script_idx != null && (w.t0 + w.t1) / 2 >= a && (w.t0 + w.t1) / 2 <= b).map((w) => w.script_idx as number);
        if (!idx.length) return;
        const segs = applyPaint(t.id, Math.min(...idx), Math.max(...idx) + 1);
        selSeg = segs.findIndex((s) => s.take_id === t.id && s.from_word === Math.min(...idx));
        await setComp(segs);
      },
    );
  }

  function segDown(e: MouseEvent, take_id: string) {
    e.preventDefault();
    e.stopPropagation();
    const i = comp.findIndex((c) => c.take_id === take_id);
    selSeg = i;
    const x0 = e.clientX;
    const n0 = comp[i]?.nudge_s ?? 0;
    let moved = false;
    const el = areaEl;
    track(
      (ev) => {
        if (Math.abs(ev.clientX - x0) > 4) moved = true;
      },
      async (ev) => {
        if (!moved || !el || i < 0) {
          if (el) app.locator = it.start_s + frac(ev, el) * boxDur;
          return;
        }
        const dt = ((ev.clientX - x0) / Math.max(1, laneW)) * boxDur;
        await nudge(i, clamp(n0 + dt, -0.3, 0.3));
      },
    );
  }

  async function nudge(i: number, n: number) {
    const segs = comp.map((s, j) => (j === i ? { ...s, nudge_s: +n.toFixed(3) } : { ...s }));
    await app.cmd({ cmd: 'set_comp', line_id: it.line_id, segments: segs });
  }

  function rulerDown(e: MouseEvent) {
    e.preventDefault();
    const el = e.currentTarget as HTMLElement;
    const set = (ev: MouseEvent) => {
      app.locator = it.start_s + frac(ev, el) * boxDur;
      if (!app.playing) app.pos = app.locator;
    };
    set(e);
    track(set, () => {});
  }

  // ---- edit mode ----
  /** Contiguous runs of heard word indexes. */
  function runsOf(idxs: number[]): [number, number][] {
    const runs: [number, number][] = [];
    for (const i of [...idxs].sort((a, b) => a - b)) {
      const r = runs[runs.length - 1];
      if (r && r[1] === i - 1) r[1] = i;
      else runs.push([i, i]);
    }
    return runs;
  }

  function toggleWord(w: ShownWord) {
    if (!w.idxs.length) return;
    if (w.deleted) {
      const cmds = runsOf(w.idxs).map(([from, to]) => ({ cmd: 'restore_words', line_id: it.line_id, from, to }));
      void app.cmd(cmds.length === 1 ? cmds[0] : { cmd: 'batch', commands: cmds });
      return;
    }
    const all = w.idxs.every((i) => selWords.includes(i));
    selWords = all ? selWords.filter((x) => !w.idxs.includes(x)) : [...new Set([...selWords, ...w.idxs])].sort((a, b) => a - b);
  }

  async function deleteSelected() {
    if (!selWords.length) return;
    const runs = runsOf(selWords);
    selWords = [];
    for (const [a, b] of runs) await app.cmd({ cmd: 'delete_words', line_id: it.line_id, from: a, to: b });
  }

  const cuts = $derived.by(() => {
    if (!it) return [];
    const out: { idx: number; x: number; keep: boolean }[] = [];
    it.words.forEach((w, i) => {
      if (w.deleted && (i === 0 || !it.words[i - 1].deleted)) out.push({ idx: w.idx, x: (w.start_s - it.start_s) / boxDur, keep: it.edits.keep_pause.includes(w.idx) });
    });
    return out;
  });

  const pauses = $derived.by(() => {
    if (!it) return [];
    const out: { idx: number; x: number; w: number; trim: number }[] = [];
    for (let i = 0; i + 1 < it.words.length; i++) {
      const a = it.words[i];
      const b = it.words[i + 1];
      if (a.deleted || b.deleted) continue;
      const gap = b.start_s - a.end_s;
      if (gap > 0.08) out.push({ idx: a.idx, x: (a.end_s - it.start_s) / boxDur, w: gap / boxDur, trim: it.edits.pause_trims[String(a.idx)] ?? 0 });
    }
    return out;
  });

  function pauseDown(e: MouseEvent, p: { idx: number; trim: number }) {
    e.preventDefault();
    e.stopPropagation();
    const x0 = e.clientX;
    track(
      () => {},
      async (ev) => {
        const dt = ((x0 - ev.clientX) / Math.max(1, laneW)) * boxDur;
        const trim = clamp(p.trim + dt, 0, 2);
        await app.cmd({ cmd: 'set_pause_trim', line_id: it.line_id, word: p.idx, trim_s: +trim.toFixed(3) });
      },
    );
  }

  async function trimEdge(e: MouseEvent, edge: 'h' | 't') {
    e.preventDefault();
    e.stopPropagation();
    const x0 = e.clientX;
    track(
      () => {},
      async (ev) => {
        const dt = ((ev.clientX - x0) / Math.max(1, laneW)) * boxDur;
        const head = edge === 'h' ? it.head_s + dt : it.head_s;
        const tail = edge === 't' ? it.tail_s - dt : it.tail_s;
        await app.cmd({ cmd: 'set_trim', line_id: it.line_id, head_s: +head.toFixed(3), tail_s: +tail.toFixed(3) });
      },
    );
  }

  async function playSource(fromLocator = false) {
    if (app.playing) return app.stop();
    if (srcTake) return app.playTake(srcTake.id, 0);
    return playComp(fromLocator);
  }

  async function playComp(fromLocator = false) {
    if (app.playing) return app.stop();
    src = 'comp';
    const from = fromLocator || (app.locator > it.start_s && app.locator < it.end_s) ? app.locator : it.start_s;
    await app.play(from, it.end_s + 0.15);
  }

  function close() {
    app.overlay = null;
  }

  function onKey(e: KeyboardEvent) {
    const key = e.key;
    let h = true;
    if (key === 'Escape') close();
    else if (key === ' ' && e.shiftKey) void playComp(true);
    else if (key === ' ') void playSource();
    else if ((key === 'z' || key === 'Z') && (e.metaKey || e.ctrlKey)) void (e.shiftKey ? app.redo() : app.undo());
    else if ((key === 'ArrowLeft' || key === 'ArrowRight') && selSeg != null && mode === 'comp' && comp[selSeg]) {
      const d = (key === 'ArrowLeft' ? -1 : 1) * (e.shiftKey ? 0.02 : 0.005);
      void nudge(selSeg, clamp(comp[selSeg].nudge_s + d, -0.3, 0.3));
    } else if (key === 'ArrowLeft' || key === 'ArrowRight') {
      const d = (key === 'ArrowLeft' ? -1 : 1) * (e.shiftKey ? 0.1 : 0.02) * boxDur;
      app.locator = clamp(app.locator + d, it.start_s, it.end_s);
      if (!app.playing) app.pos = app.locator;
    } else if (key === 'Tab') selSeg = comp.length ? (selSeg == null ? 0 : (selSeg + (e.shiftKey ? comp.length - 1 : 1)) % comp.length) : null;
    else if (key === 'w' || key === 'W') showWords = !showWords;
    else if (key === 'm' || key === 'M') mode = mode === 'comp' ? 'edit' : 'comp';
    else if (key === 'r' || key === 'R') void app.cmd({ cmd: 'reset_comp', line_id: it.line_id });
    else if ((key === 'Backspace' || key === 'Delete') && mode === 'edit') void deleteSelected();
    else if (/^[1-9]$/.test(key) && +key <= it.takes.length) void app.cmd({ cmd: 'select_take', line_id: it.line_id, take_id: it.takes[+key - 1].id });
    else h = false;
    if (h) {
      e.preventDefault();
      e.stopPropagation();
    }
  }

  onMount(() => {
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  });

  const hl = (shd: [number, number, number]) => `color: var(--text-strong); font-weight: 700; text-shadow: 0 0 12px ${rgba(shd, 1)}, 0 1px 6px rgb(var(--shadow-rgb) / 0.9)`;
  const compInfo = $derived.by(() => {
    const used = [...new Set(comp.map((c) => takeIdx(c.take_id) + 1))];
    return comp.length <= 1 ? `whole T${used[0] ?? k + 1}` : `${comp.length} parts from ${used.map((u) => 'T' + u).join(' ')}`;
  });
</script>

{#if it}
  <div class="dim" aria-hidden="true"></div>
  <div class="glass ed" role="dialog" aria-modal="true" aria-label="Box editor" data-testid="box-editor" style="background: linear-gradient(160deg, {rgba(sh, 0.18)} 0%, var(--panel) 32%, var(--panel-solid) 100%); box-shadow: inset 0 1px 0 rgb(var(--veil-rgb) / 0.12), inset 0 0 0 1px rgb(var(--veil-rgb) / 0.05), 0 40px 120px rgb(var(--shadow-rgb) / 0.6)">
    <div class="head">
      <div class="title">
        <div class="r"><span class="nm" style="font-size: 12px; color: {color.name}">{charLabel(app.view, it.character)}</span><span class="info">Line {it.number} · {it.takes.length} takes · {boxDur.toFixed(1)} s</span></div>
        <p class="script txt">{edText}</p>
      </div>
      <div class="btns">
        <div class="modes" role="group" aria-label="Mode">
          <button class:on={mode === 'comp'} onclick={() => (mode = 'comp')}>Comp</button>
          <button class:on={mode === 'edit'} onclick={() => (mode = 'edit')} data-testid="edit-mode">Edit</button>
        </div>
        <button class="b" class:on={showWords} onclick={() => (showWords = !showWords)}>Words <span class="kb">W</span></button>
        <button class="b" onclick={() => app.cmd({ cmd: 'reset_comp', line_id: it.line_id })}>Reset <span class="kb">R</span></button>
        <button class="done" onclick={close}>Done <span class="kb">Esc</span></button>
      </div>
    </div>

    <div class="lanes">
      <div class="lane">
        <div class="lbl plain"><span class="t">{(edPos * boxDur).toFixed(2)} s</span><span class="sub">click or drag to place</span></div>
        <div class="area ruler" onmousedown={rulerDown} role="slider" tabindex="-1" aria-valuenow={edLoc}>
          {#each ticks as tk (tk.label)}<span class="tick" style="left: {tk.x * 100}%"></span><span class="tl" style="left: {tk.x * 100}%">{tk.label}</span>{/each}
          <span class="flag" style="left: {edLoc * 100}%"></span>
        </div>
      </div>
      <div class="lane">
        <button class="lbl comp" class:src={src === 'comp'} onclick={() => (src = 'comp')} title="Listen to the comp (Space)">
          <span class="t">{src === 'comp' ? '▶ ' : ''}Comp</span><span class="sub">{mode === 'edit' ? 'click words, ⌫ deletes' : compInfo}</span>
        </button>
        <div class="area complane" bind:this={areaEl} data-testid="comp-lane" style={src === 'comp' ? 'box-shadow: inset 0 0 0 1.5px rgb(var(--ink-rgb) / 0.7)' : ''}>
          {#each pieces as pc, i (i)}
            {@const gsh = shadeOf(pc.p.take_id)}
            <div class="seg" class:sel={selSeg != null && comp[selSeg]?.take_id === pc.p.take_id} onmousedown={(e) => mode === 'comp' && segDown(e, pc.p.take_id)} role="button" tabindex="-1" title="Click to listen from here, drag sideways to nudge" style="left: {pc.a * 100}%; width: {pc.w * 100}%; background: {rgba(gsh, 0.16)}; box-shadow: inset 0 -3px 0 {rgba(gsh, 0.95)}{selSeg != null && comp[selSeg]?.take_id === pc.p.take_id ? `, inset 0 0 0 2px ${rgba(gsh, 1)}` : ''}">
              {#if pc.t}
                <div class="segin" style="left: {(-(pc.p.src_in - pc.t.src_in) / pc.p.len_s) * 100}%; width: {(pc.t.duration / pc.p.len_s) * 100}%"><Waveform peaks={pc.t.peaks} fill={rgba(gsh, 0.85)} /></div>
              {/if}
              <span class="chip" style="background: {rgba(gsh, 0.92)}">T{takeIdx(pc.p.take_id) + 1}{pc.nudge ? ` ${pc.nudge > 0 ? '+' : ''}${Math.round(pc.nudge * 1000)} ms` : ''}</span>
            </div>
          {/each}
          {#if showWords || mode === 'edit'}
            {#each edWords as w (w.j)}
              {@const wsh = shadeOf(takeOfWord(w))}
              {@const on = app.pos >= w.start && app.pos <= w.end && !w.deleted}
              <button class="ew" class:editable={mode === 'edit'} class:deleted={w.deleted} class:picked={w.idxs.length > 0 && w.idxs.every((i) => selWords.includes(i))} data-testid="comp-word" data-deleted={w.deleted} onclick={() => mode === 'edit' && toggleWord(w)} style="left: {((w.start - it.start_s) / boxDur) * 100}%; min-width: {Math.max(0.5, ((w.end - w.start) / boxDur) * 100)}%; font-size: 14px{on ? '; ' + hl(wsh) : ''}">{w.text}</button>
            {/each}
          {/if}
          {#each seams as s, i (i)}<span class="seam" style="left: {s.x * 100}%; background: {s.warn ? 'var(--warn)' : 'rgb(var(--ink-rgb) / 0.75)'}{s.warn ? '; box-shadow: 0 0 10px var(--warn-glow)' : ''}"></span>{/each}
          {#if mode === 'edit'}
            {#each cuts as c (c.idx)}
              <button class="cut" style="left: {c.x * 100}%" title="Close or keep the pause at this cut" onclick={() => app.cmd({ cmd: 'set_cut_pause', line_id: it.line_id, cut: c.idx, keep: !c.keep })}>{c.keep ? 'keep pause' : 'close'}</button>
            {/each}
            {#each pauses as p (p.idx)}
              <span class="pause" role="slider" tabindex="-1" aria-valuenow={p.trim} title="Drag left to shorten the pause{p.trim ? ` (−${Math.round(p.trim * 1000)} ms)` : ''}" onmousedown={(e) => pauseDown(e, p)} style="left: {p.x * 100}%; width: {Math.max(0.6, p.w * 100)}%"></span>
            {/each}
            <span class="edge" role="slider" tabindex="-1" aria-valuenow={it.head_s} title="Drag to trim the head" onmousedown={(e) => trimEdge(e, 'h')} style="left: 0"></span>
            <span class="edge" role="slider" tabindex="-1" aria-valuenow={it.tail_s} title="Drag to trim the tail" onmousedown={(e) => trimEdge(e, 't')} style="right: 0"></span>
          {/if}
        </div>
      </div>
      <div class="lane hintrow"><span class="nm lbl0">Takes</span><span class="hintt">Click a take to listen from there. Drag across it to paint that stretch into the comp.</span></div>
      {#each it.takes as t, i (t.id)}
        {@const lsh = shade(color.rgb, i)}
        {@const used = usedBars(t)}
        <div class="lane">
          <button class="lbl" onclick={() => app.cmd({ cmd: 'select_take', line_id: it.line_id, take_id: t.id })} aria-label="Use whole take {i + 1}" title="Use the whole take" style="box-shadow: inset 3px 0 0 {rgba(lsh, 0.95)}">
            <span class="t2">{src === t.id ? '▶ ' : ''}T{i + 1}{t.favorite ? ' ★' : ''}{i === k ? ' · picked' : ''}</span><span class="sub ellip">{firstSentence(t.direction || t.feedback) || takeWords(t.words).map((w) => w.text).join(' ') || t.heard}</span>
          </button>
          <div class="area take" data-testid="take-lane" onmousedown={(e) => laneDown(e, i)} role="button" tabindex="-1" style="background: {rgba(lsh, src === t.id ? 0.1 : 0.05)}{src === t.id ? `; box-shadow: inset 0 0 0 1.5px ${rgba(lsh, 0.9)}` : ''}{t.partial ? '; outline: 1px dashed rgb(var(--ink-rgb) / 0.3); outline-offset: -1px' : ''}">
            <Waveform peaks={t.peaks} fill={rgba(lsh, used.length ? 0.72 : 0.42)} />
            {#each used as u, j (j)}<div class="used" style="left: {u.a * 100}%; width: {(u.b - u.a) * 100}%; background: {rgba(lsh, 1)}"></div>{/each}
            {#if showWords}{#each takeWords(t.words) as w (w.j)}<span class="ew static" style="left: {(w.start / t.duration) * 100}%; width: {((w.end - w.start) / t.duration) * 100}%{auditioning && src === t.id && app.pos >= w.start && app.pos <= w.end ? '; ' + hl(lsh) : ''}">{w.text}</span>{/each}{/if}
            {#if paint && paint.lane === i}<div class="paint" style="left: {Math.min(paint.a, paint.b) * 100}%; width: {Math.abs(paint.b - paint.a) * 100}%; background: {rgba(lsh, 0.25)}; box-shadow: inset 0 0 0 1px {rgba(lsh, 0.95)}"></div>{/if}
          </div>
        </div>
      {/each}
      <div class="loc" style="left: calc(162px + (100% - 162px) * {edLoc.toFixed(4)})"></div>
      <div class="ph" style="left: calc(162px + (100% - 162px) * {edPos.toFixed(4)})"><span></span></div>
    </div>

    <div class="foot">
      <div class="keys">
        <span><span class="kb">⇧</span> <span class="kb">Space</span> from locator</span>
        <span>Click a segment, <span class="kb">←</span> <span class="kb">→</span> nudge (<span class="kb">⇧</span> coarse)</span>
        <span><span class="kb">Tab</span> next segment</span>
        <span><span class="kb">M</span> comp / edit</span>
        <span class="leg"><span class="sw"></span>seam cuts a word</span>
      </div>
      <button class="playb" data-testid="play-source" onclick={() => playSource()} style="background: {rgba(sh, 0.22)}; box-shadow: inset 0 0 0 1px {rgba(sh, 0.45)}">
        <span class="pc">{#if app.playing}<svg width="12" height="12" viewBox="0 0 24 24" aria-hidden="true"><rect x="5" y="5" width="14" height="14" rx="2" fill="var(--text-on-light)" /></svg>{:else}<svg width="14" height="14" viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5v14l11-7z" fill="var(--text-on-light)" /></svg>{/if}</span>
        {srcTake ? `Play T${takeIdx(srcTake.id) + 1}` : 'Play comp'} <span class="kb">Space</span>
      </button>
    </div>
  </div>
{/if}

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
  .txt {
    margin: 8px 0 0;
    font-size: 21px;
    line-height: 1.4;
    color: var(--text-strong);
  }
  .btns {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
  }
  .modes {
    display: flex;
    padding: 3px;
    border-radius: var(--radius-pill);
    background: var(--glass-fill-2);
  }
  .modes button {
    height: 30px;
    padding: 0 12px;
    border-radius: var(--radius-pill);
    font-size: 12px;
    color: var(--text-3);
  }
  .modes button.on {
    background: rgb(var(--ink-rgb) / 0.14);
    color: var(--text-strong);
    font-weight: 600;
  }
  .b {
    height: 36px;
    padding: 0 14px;
    border-radius: var(--radius-pill);
    font-size: 12.5px;
    color: var(--text-2);
    background: var(--glass-fill-3);
  }
  .b.on {
    background: rgb(var(--ink-rgb) / 0.14);
    color: var(--text-strong);
    font-weight: 600;
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
  .done .kb {
    background: rgb(var(--shadow-rgb) / 0.1);
    color: var(--text-on-light);
  }
  .lanes {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .lane {
    display: flex;
    gap: 12px;
    align-items: stretch;
  }
  .lbl {
    flex: none;
    width: 150px;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 2px;
    padding: 6px 10px 6px 14px;
    border-radius: var(--radius-lane);
    text-align: left;
    background: var(--glass-fill-1);
  }
  .lbl:hover {
    background: var(--glass-hover);
  }
  .lbl.plain {
    background: transparent;
  }
  .lbl.comp {
    background: var(--glass-fill-3);
  }
  .lbl.comp.src {
    box-shadow: inset 3px 0 0 var(--text-strong);
    background: var(--glass-active);
  }
  .t {
    font-size: 12px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .comp .t {
    font-size: 13px;
    font-weight: 700;
  }
  .t2 {
    font-size: 13px;
    font-weight: 600;
  }
  .sub {
    font-size: 10.5px;
    color: var(--text-4);
    max-width: 126px;
  }
  .area {
    position: relative;
    flex: 1;
    min-width: 0;
    border-radius: var(--radius-lane);
    overflow: hidden;
    cursor: crosshair;
  }
  .ruler {
    height: 26px;
    cursor: ew-resize;
    background: rgb(var(--veil-rgb) / 0.045);
  }
  .tick {
    position: absolute;
    bottom: 0;
    width: 1px;
    height: 8px;
    background: rgb(var(--ink-rgb) / 0.35);
  }
  .tl {
    position: absolute;
    top: 4px;
    margin-left: 4px;
    font-size: 9.5px;
    color: var(--text-4);
    font-variant-numeric: tabular-nums;
  }
  .complane {
    height: 84px;
    cursor: default;
    background: var(--glass-fill-1);
    box-shadow: inset 0 0 0 1px var(--glass-outline);
  }
  .take {
    height: 58px;
  }
  .seg {
    position: absolute;
    top: 0;
    bottom: 0;
    overflow: hidden;
    cursor: grab;
  }
  .segin {
    position: absolute;
    top: 0;
    bottom: 0;
  }
  .chip {
    position: absolute;
    left: 6px;
    top: 5px;
    font-size: 10px;
    font-weight: 700;
    color: var(--text-on-pill);
    padding: 1px 6px;
    border-radius: 6px;
    white-space: nowrap;
    pointer-events: none;
  }
  .seam {
    position: absolute;
    top: -5px;
    bottom: -5px;
    width: 2px;
    margin-left: -1px;
    pointer-events: none;
    z-index: 3;
  }
  .ew {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    text-align: left;
    font-family: var(--font-script);
    font-size: 13px;
    color: rgb(var(--ink-rgb) / 0.86);
    text-shadow:
      0 1px 8px rgb(var(--shadow-rgb) / 0.95),
      0 0 2px rgb(var(--shadow-rgb) / 0.9);
    white-space: nowrap;
    pointer-events: none;
    transition: color 0.1s;
    z-index: 2;
  }
  .ew.static {
    text-align: center;
  }
  .ew.editable {
    pointer-events: auto;
    cursor: pointer;
    border-radius: 4px;
    padding: 1px 2px;
  }
  .ew.editable:hover {
    background: rgb(var(--veil-rgb) / 0.12);
  }
  .ew.picked {
    background: rgb(var(--ink-rgb) / 0.25);
  }
  .ew.deleted {
    color: var(--text-dim);
    text-decoration: line-through;
  }
  .cut {
    position: absolute;
    bottom: 4px;
    transform: translateX(-50%);
    z-index: 4;
    font-size: 9.5px;
    padding: 1px 6px;
    border-radius: var(--radius-pill);
    background: var(--pill);
    color: var(--warn);
  }
  .pause {
    position: absolute;
    top: 8px;
    height: 10px;
    border-radius: 3px;
    background: rgb(var(--locator-rgb) / 0.25);
    cursor: ew-resize;
    z-index: 4;
  }
  .edge {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 8px;
    background: rgb(var(--ink-rgb) / 0.85);
    cursor: ew-resize;
    z-index: 5;
    border-radius: 3px;
  }
  .used {
    position: absolute;
    bottom: 0;
    height: 3px;
    border-radius: 2px;
  }
  .paint {
    position: absolute;
    top: 0;
    bottom: 0;
    border-radius: 6px;
    pointer-events: none;
  }
  .hintrow {
    align-items: center;
    padding: 2px 0 0;
  }
  .lbl0 {
    width: 150px;
    flex: none;
    font-size: 10px;
    color: var(--text-4);
  }
  .hintt {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .loc {
    position: absolute;
    top: 34px;
    bottom: -6px;
    border-left: 2px dashed var(--locator-soft);
    margin-left: -1px;
    pointer-events: none;
    z-index: 7;
  }
  .ph {
    position: absolute;
    top: 0;
    bottom: -6px;
    width: 2px;
    margin-left: -1px;
    background: var(--playhead);
    box-shadow: 0 0 10px var(--playhead-glow);
    pointer-events: none;
    z-index: 8;
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
  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }
  .keys {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
    font-size: 12px;
    color: var(--text-3);
  }
  .leg {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .sw {
    width: 3px;
    height: 12px;
    border-radius: 2px;
    background: var(--warn);
  }
  .playb {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 44px;
    padding: 0 18px 0 8px;
    border-radius: var(--radius-pill);
    font-size: var(--fs-ui);
    font-weight: 600;
  }
  .pc {
    width: 30px;
    height: 30px;
    border-radius: 50%;
    background: var(--button-light);
    display: flex;
    align-items: center;
    justify-content: center;
  }
</style>
