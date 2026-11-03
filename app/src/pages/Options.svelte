<script lang="ts">
  import ClipState from '../components/ClipState.svelte';
  import { tick } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { fmt } from '../lib/format';
  import { base, charColor, charLabel, itemShade, mix, rgba, shade, solid } from '../lib/colors';
  import { openEditor } from '../lib/keys';
  import type { ItemView } from '../lib/types';
  import { lineText, lineWords, type ShownWord } from '../lib/words';

  const sc = $derived(app.scale);
  let activeEl: HTMLDivElement | undefined = $state();
  let off = $state(0);
  let ready = $state(false);

  type Row =
    | { kind: 'line'; it: ItemView; j: number }
    | { kind: 'gap'; h: number; j: number }
    | { kind: 'act'; it: ItemView; d: number; gap: number };

  const rows = $derived.by(() => {
    const out: Row[] = [];
    const v = app.view;
    if (!v) return out;
    let j = -1;
    let pending: ItemView[] = [];
    for (const it of v.items) {
      if (it.kind === 'spacer') {
        pending.push(it);
        continue;
      }
      j += 1;
      const gap = it.gap_before_s;
      if (pending.length) {
        for (const p of pending) out.push({ kind: 'act', it: p, d: Math.abs(j - 0.5 - app.curIndex), gap: j === 0 ? 0.4 : gap });
      } else if (j > 0) {
        out.push({ kind: 'gap', h: gap, j });
      }
      pending = [];
      out.push({ kind: 'line', it, j });
    }
    for (const p of pending) out.push({ kind: 'act', it: p, d: Math.abs(j + 0.5 - app.curIndex), gap: 0.6 });
    return out;
  });

  // keep the current box on the centre hairline
  $effect(() => {
    void app.curId;
    void app.view;
    void sc;
    void tick().then(() => {
      if (!activeEl) return;
      off = activeEl.offsetTop + activeEl.offsetHeight / 2;
      requestAnimationFrame(() => (ready = true));
    });
  });

  function lineStyle(it: ItemView, j: number) {
    const shd = itemShade(app.view, it);
    const d = Math.abs(j - app.curIndex);
    const nt = it.takes.length;
    const edges = nt > 2 ? `, 7px 0 0 -3px ${rgba(shd, 0.18)}, 14px 0 0 -6px ${rgba(shd, 0.1)}` : nt > 1 ? `, 7px 0 0 -3px ${rgba(shd, 0.18)}` : '';
    const dim = nt === 0 ? 0.5 : 1;
    return `border-radius: ${Math.round(24 * sc)}px; padding: ${Math.round(16 * sc)}px ${Math.round(22 * sc)}px ${Math.round(18 * sc)}px; opacity: ${(Math.max(0.35, 1 - d * 0.12) * dim).toFixed(2)}; background: linear-gradient(115deg, ${rgba(shd, 0.2)} 0%, ${rgba(shd, 0.06)} 40%, rgb(var(--veil-rgb) / 0.025) 100%); box-shadow: inset 0 1px 0 rgb(var(--veil-rgb) / 0.09)${edges}, 0 18px 50px rgb(var(--shadow-rgb) / 0.35)${it.takes[it.take_index ?? 0]?.partial ? '; outline: 1px dashed var(--text-5); outline-offset: -1px' : ''}`;
  }

  function textOf(it: ItemView): string {
    return it.takes.length === 0 ? it.text : lineText(it);
  }

  const cur = $derived(app.cur);
  const k = $derived(cur?.take_index ?? 0);
  const take = $derived(cur && cur.takes.length ? cur.takes[k] : undefined);
  const color = $derived(cur ? charColor(app.view, cur.character) : charColor(null, null));
  const sh = $derived(cur ? shade(color.rgb, k) : color.rgb);
  const live = $derived(!!cur && (app.playing || (app.pos > cur.start_s + 0.01 && app.pos < cur.end_s)));
  const R = $derived(Math.round(32 * sc));

  // the script's words, timed by what was heard
  const curWords = $derived(cur ? lineWords(cur) : []);

  function wordStyle(w: ShownWord) {
    if (w.deleted) return 'color: var(--text-dim); text-decoration: line-through';
    if (w.muted) return 'color: var(--text-dim)';
    if (!live || w.end < app.pos) return 'color: var(--text-strong)';
    if (w.start <= app.pos) return `transform: scale(1.22); color: var(--text-strong); text-shadow: 0 0 22px ${rgba(sh, 0.95)}`;
    return 'color: rgb(var(--ink-rgb) / 0.42)';
  }

  const deck = $derived.by(() => {
    if (!cur) return [];
    const n = cur.takes.length;
    const out: { num: number; style: string }[] = [];
    for (let di = 1; di <= Math.min(3, n - 1); di++) {
      const o = (k + di) % n;
      const osh = shade(color.rgb, o);
      out.push({
        num: o + 1,
        style: `border-radius: ${R}px; transform: translateX(${di * 16}px) scaleY(${(1 - di * 0.07).toFixed(2)}); z-index: ${4 - di}; background: linear-gradient(${110 + o * 14}deg, ${solid(mix(osh, base(), 0.5 - di * 0.08))}, ${solid(mix(osh, base(), 0.22))}); box-shadow: inset 0 0 0 1px rgb(var(--veil-rgb) / 0.08), 6px 0 18px rgb(var(--shadow-rgb) / 0.35)`,
      });
    }
    return out;
  });

  const boxStyle = $derived(
    `${cur && !cur.takes.length ? 'opacity: 0.55; ' : ''}min-height: ${Math.round(190 * sc)}px; border-radius: ${R}px; padding: ${Math.round(24 * sc)}px ${Math.round(34 * sc)}px; background: linear-gradient(${110 + k * 14}deg, ${solid(mix(sh, base(), 0.48))} 0%, ${solid(mix(sh, base(), 0.2))} 50%, var(--mix-base) 100%); box-shadow: inset 0 1px 0 rgb(var(--veil-rgb) / 0.13), inset 0 0 0 1px ${rgba(sh, 0.34)}, 0 24px 90px ${rgba(sh, app.playing ? 0.34 : 0.18)}, 0 18px 50px rgb(var(--shadow-rgb) / 0.4)${take?.partial ? '; outline: 1px dashed var(--text-4); outline-offset: -2px' : ''}`,
  );

  function pillStyle(i: number, partial: boolean) {
    const psh = shade(color.rgb, i);
    let st = i === k ? `background: ${rgba(psh, 0.92)}; color: var(--text-on-pill)` : `background: var(--glass-fill-3); color: var(--text-3); box-shadow: inset 0 -2px 0 ${rgba(psh, 0.85)}`;
    if (partial) st += '; outline: 1px dashed rgb(var(--ink-rgb) / 0.45); outline-offset: -1px';
    return st;
  }

  const inPassage = (id: string) => app.passage.some((p) => p.line_id === id);
</script>

<div class="optmask">
  <div class="col" style="width: min({Math.round(640 * sc)}px, 88%); transform: translate(-50%, -{off.toFixed(1)}px); transition: {ready ? 'transform var(--dur-scroll) var(--ease-scroll)' : 'none'}">
    {#each rows as r, ri (r.kind === 'line' ? r.it.line_id : r.kind === 'act' ? r.it.line_id : 'g' + ri)}
      {#if r.kind === 'gap'}
        <div style="flex: none; height: {Math.round(Math.max(r.h, 0) * 10 * sc)}px; margin-top: {r.h < 0 ? Math.round(r.h * 14) : 0}px"></div>
      {:else if r.kind === 'act'}
        <div style="flex: none; opacity: {Math.max(0.35, 1 - r.d * 0.12).toFixed(2)}; margin: {Math.round(Math.max(r.gap, 0) * 6 * sc)}px 0">
          <button class="script act" class:off={!r.it.spacer_enabled} style="font-size: {Math.round(13 * sc)}px" title={r.it.text} onclick={(e) => (e.currentTarget as HTMLElement).classList.toggle('open')}>{r.it.text}</button>
        </div>
      {:else if cur && r.it.line_id === cur.line_id}
        <div class="active" bind:this={activeEl} data-testid="current-box">
          {#each deck as dk (dk.num)}
            <div class="deck" aria-hidden="true" style={dk.style}><span class="dnum">{dk.num}</span></div>
          {/each}
          {#key app.anim.id === cur.line_id ? app.anim.n : 0}
            <article class="glass abox {app.anim.id === cur.line_id && app.anim.n ? 'in' + app.anim.dir : ''}" class:clip-muted={cur.muted} class:clip-dropped={cur.dropped} ondblclick={openEditor} aria-label="Current line" style="position: relative; z-index: 5; {boxStyle}">
              <div class="hd">
                <div class="nmrow">
                  <span class="nm" style="font-size: 12px; color: {color.name}">{charLabel(app.view, cur.character)}</span>
                  {#if take?.favorite}<span style="font-size: 13px; color: {color.name}" title={take.favorite_origin === 'director' ? 'Director pick' : 'Your favorite'}>★</span>{/if}
                  {#if cur.low_confidence}<button class="lowc" title="Low confidence: open the inspector (⌘I)" aria-label="Low confidence" onclick={() => (app.drawer = 'inspector')}></button>{/if}
                  {#if cur.edited_in_edit}<button class="edited" onclick={() => app.setPage('edit')} title="Edited on the Edit page">edited in Edit</button>{/if}
                  <ClipState it={cur} />
                </div>
                <span class="pos" data-testid="take-pos">{cur.takes.length ? `${k + 1} / ${cur.takes.length}` : 'no takes'}</span>
              </div>
              {#if cur.paren}<p class="script paren" style="font-size: {Math.round(14 * sc)}px">({cur.paren})</p>{/if}
              <p class="script line" style="font-size: {Math.round(24 * sc)}px" data-testid="current-text">
                {#if curWords.length}
                  <!-- the space between words keeps line wrapping (Svelte trims plain whitespace here) -->
                  <!-- eslint-disable-next-line svelte/no-useless-mustaches -->
                  {#each curWords as w (w.j)}<span class="wd" style={wordStyle(w)}>{w.text}</span>{" "}{/each}
                {:else}
                  <span style="color: var(--text-4)">{cur.text}</span>
                {/if}
              </p>
            </article>
          {/key}
          {#if cur.takes.length}
            <div class="pills" role="group" aria-label="Takes">
              {#each cur.takes as t, i (t.id)}
                <button class="tk" data-testid="take-pill" data-selected={i === k} onclick={() => app.pickTake(cur.line_id, t.id)} aria-label="Take {i + 1}{t.favorite ? ', favorite' : ''}{t.partial ? ', partial' : ''}" style={pillStyle(i, t.partial)}>{i + 1}{t.favorite ? ' ★' : ''}</button>
              {/each}
            </div>
          {/if}
          <aside class="side notes" aria-label="Notes for this take">
            <div><div class="nm lab">Direction</div><p>{take?.direction || '—'}</p></div>
            <div><div class="nm lab">Feedback</div><p>{take?.feedback || '—'}</p></div>
            {#if take}<div class="meta">{(cur.end_s - cur.start_s).toFixed(1)} s · match {Math.round(take.score)}%{take.partial ? ` · ${take.take_type}` : ''}{take.needs_rematch ? ' · script changed' : ''}</div>{/if}
          </aside>
          <div class="side chip">{fmt(app.pos)}</div>
        </div>
      {:else}
        <div class="boxs" class:passage={inPassage(r.it.line_id)} class:clip-muted={r.it.muted} class:clip-dropped={r.it.dropped} onclick={() => app.select(r.it.line_id)} ondblclick={() => { app.select(r.it.line_id); openEditor(); }} style="flex: none; {lineStyle(r.it, r.j)}" role="button" tabindex="-1" onkeydown={() => {}}>
          <div class="hd small">
            <span class="nm" style="color: {charColor(app.view, r.it.character).name}; font-size: {Math.max(10, Math.round(11 * sc))}px">{charLabel(app.view, r.it.character)}</span>
            <span class="info">{r.it.takes[r.it.take_index ?? 0]?.favorite ? '★ ' : ''}{r.it.takes.length ? `${(r.it.take_index ?? 0) + 1}/${r.it.takes.length}` : '—'}{r.it.edited_in_edit ? ' · Edit' : ''}{r.it.dropped ? ' · cut' : r.it.muted ? ' · muted' : ''}</span>
          </div>
          {#if r.it.paren}<p class="script paren2" style="font-size: {Math.round(13 * sc)}px">({r.it.paren})</p>{/if}
          <p class="script txt" style="font-size: {Math.round(18 * sc)}px">{textOf(r.it)}</p>
        </div>
      {/if}
    {/each}
  </div>
</div>

<style>
  .optmask {
    position: absolute;
    inset: 0;
    overflow: hidden;
    -webkit-mask-image: linear-gradient(to bottom, transparent 0, #000 15%, #000 80%, transparent 94%);
    mask-image: linear-gradient(to bottom, transparent 0, #000 15%, #000 80%, transparent 94%);
  }
  .col {
    position: absolute;
    left: 50%;
    top: 50%;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .act {
    display: block;
    width: 100%;
    text-align: left;
    padding: 8px 16px;
    border-radius: 14px;
    background: var(--glass-fill-1);
    color: rgb(var(--ink-rgb) / 0.5);
    backdrop-filter: var(--glass-blur-soft);
    -webkit-backdrop-filter: var(--glass-blur-soft);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    box-sizing: border-box;
  }
  .act:global(.open) {
    white-space: normal;
  }
  .act.off {
    text-decoration: line-through;
    opacity: 0.6;
  }
  .boxs {
    cursor: pointer;
    backdrop-filter: var(--glass-blur);
    -webkit-backdrop-filter: var(--glass-blur);
    transition: opacity 0.3s;
  }
  .boxs:hover {
    filter: brightness(1.12);
  }
  .boxs.passage {
    outline: 1.5px solid var(--text-3);
    outline-offset: 2px;
  }
  .hd {
    position: relative;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .hd.small {
    margin-bottom: 6px;
  }
  .info,
  .pos {
    font-size: var(--fs-small);
    color: var(--text-4);
    font-variant-numeric: tabular-nums;
  }
  .pos {
    font-size: 12px;
    color: var(--text-3);
  }
  .nmrow {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .lowc {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--warn);
  }
  .edited {
    font-size: 10px;
    padding: 1px 7px;
    border-radius: var(--radius-pill);
    box-shadow: inset 0 0 0 1px var(--text-5);
    color: var(--text-3);
  }
  .paren2 {
    margin: 0 0 4px;
    font-style: italic;
    color: var(--text-4);
  }
  .txt {
    margin: 0;
    line-height: 1.45;
    color: var(--text-2);
  }
  .active {
    flex: none;
    position: relative;
    isolation: isolate;
    margin: 6px 0 56px;
  }
  .deck {
    position: absolute;
    inset: 0;
    transition:
      background 0.35s,
      transform 0.35s;
  }
  .dnum {
    position: absolute;
    right: 4px;
    top: 50%;
    transform: translateY(-50%);
    font-size: 10px;
    font-weight: 700;
    color: rgb(var(--veil-rgb) / 0.78);
  }
  .abox {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 12px;
    box-sizing: border-box;
    overflow: hidden;
    transition: box-shadow 0.35s;
  }
  .paren {
    position: relative;
    margin: 0;
    font-style: italic;
    color: rgb(var(--ink-rgb) / 0.58);
  }
  .line {
    position: relative;
    margin: 0;
    line-height: 1.5;
    color: var(--text-strong);
  }
  .wd {
    display: inline-block;
    transform-origin: 50% 72%;
    transition:
      transform 0.14s ease,
      color 0.15s ease,
      text-shadow 0.15s ease;
  }
  .pills {
    position: absolute;
    left: 50%;
    top: calc(100% + 14px);
    transform: translateX(-50%);
    display: flex;
    gap: 6px;
    z-index: 6;
  }
  .tk {
    height: 30px;
    min-width: 36px;
    padding: 0 10px;
    box-sizing: border-box;
    border-radius: var(--radius-pill);
    font-size: 12px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    transition:
      background 0.2s,
      color 0.2s;
  }
  .notes {
    position: absolute;
    left: calc(100% + 64px);
    top: 50%;
    transform: translateY(-50%);
    width: 220px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    z-index: 6;
  }
  .notes p {
    margin: 4px 0 0;
    font-size: 14px;
    line-height: 1.4;
    color: rgb(var(--ink-rgb) / 0.88);
  }
  .lab {
    font-size: 10px;
    color: var(--text-4);
  }
  .meta {
    font-size: var(--fs-small);
    line-height: 1.5;
    color: var(--text-4);
    font-variant-numeric: tabular-nums;
  }
  .chip {
    position: absolute;
    right: calc(100% + 40px);
    top: 50%;
    transform: translateY(-50%);
    font-size: var(--fs-small);
    font-variant-numeric: tabular-nums;
    color: var(--text-3);
    padding: 4px 8px;
    border-radius: var(--radius-chip);
    background: var(--chip);
    z-index: 6;
  }
</style>
