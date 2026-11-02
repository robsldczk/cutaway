<script lang="ts">
  // Learn: review one dialogue-heavy scene (mark everything wrong or missing in Under the
  // hood), then measure settings variants against those marks on it and on a held-out
  // scene, and adopt what helps: settings, worked examples for Claude, misheard words.
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import type { Job } from '../lib/types';
  import type { DevSession, DevSessionItem } from '../lib/dev';

  interface Props {
    session: DevSession | null;
    sessions: DevSessionItem[];
    reload: () => Promise<void>;
    open: (sceneNumber: number) => void;
  }
  let { session, open }: Props = $props();

  interface LearnScene { id: string; number: number; heading: string; own_lines: number; takes: number; reviewed: boolean }
  interface Glossary { heard: string; script: string; count?: number }
  interface LearnView {
    source_id: string;
    character: string;
    learn: { source_id: string; reference_scene: string; holdout_scene: string; reviewed: string[] };
    scenes: LearnScene[];
    examples: { candidates: string[]; adopted: string[] };
    glossary: { candidates: Glossary[]; adopted: Glossary[] };
    settings: { merge_gap_s: number; bleed_margin_db: number; match_offset: number; rescue: boolean };
  }
  interface EvalScene { scene_id: string; truth: number; found: number; wrong: number; missed: number }
  interface EvalRow { name: string; adopt: { key: string; value: unknown }[]; scenes: EvalScene[]; seconds: number; ai_calls: number; flagged: string[] }

  let v = $state<LearnView | null>(null);
  let ref = $state('');
  let hold = $state('');
  let job = $state<Job | null>(null);
  let results = $state<EvalRow[] | null>(null);
  let pickEx = $state<string[]>([]);
  let pickGl = $state<string[]>([]);

  async function load() {
    if (!session) return;
    v = (await app.run(() => call<LearnView>('dev_learn', { source_id: session!.source_id }))) ?? null;
    if (!v) return;
    const mine = v.learn.source_id === v.source_id;
    ref = (mine && v.learn.reference_scene) || ref || v.scenes[0]?.id || '';
    hold = (mine && v.learn.holdout_scene) || hold || v.scenes.find((s) => s.id !== ref)?.id || '';
  }
  $effect(() => {
    void session?.source_id;
    void app.status?.rev;
    void load();
  });

  const sceneBy = $derived(new Map((v?.scenes ?? []).map((s) => [s.id, s])));
  const key = (sceneId: string) => `${v?.source_id}/${sceneId}`;

  async function setCfg(k: string, value: unknown) {
    await app.run(() => call('set_config', { key: k, value }));
  }
  async function saveSetup() {
    if (!v) return;
    await setCfg('analysis.learn', { ...v.learn, source_id: v.source_id, reference_scene: ref, holdout_scene: hold });
    await load();
  }
  async function toggleReviewed(sceneId: string) {
    if (!v) return;
    const k = key(sceneId);
    const reviewed = v.learn.reviewed.includes(k) ? v.learn.reviewed.filter((x) => x !== k) : [...v.learn.reviewed, k];
    await setCfg('analysis.learn', { ...v.learn, source_id: v.source_id, reference_scene: ref, holdout_scene: hold, reviewed });
    await load();
  }
  async function measure() {
    if (!v || !ref) return;
    await saveSetup();
    const ids = [ref, hold].filter((x, i, a) => x && a.indexOf(x) === i);
    const r = await app.run(() => call<{ job_id: string }>('dev_eval', { source_id: v!.source_id, scene_ids: ids }));
    if (!r) return;
    const j = await app.waitJob(r.job_id, (x) => (job = x));
    job = null;
    if (j?.state === 'failed') return app.say(j.message || 'Measuring failed');
    results = ((j?.result as { variants?: EvalRow[] } | null)?.variants ?? null);
  }
  async function adopt(row: EvalRow) {
    for (const a of row.adopt) await setCfg(a.key, a.value);
    app.say(`Adopted: ${row.name}`);
    await load();
  }
  async function useExamples() {
    if (!v) return;
    const all = [...v.examples.adopted, ...pickEx.filter((x) => !v!.examples.adopted.includes(x))].slice(0, 5);
    await setCfg('ai.rescue_examples', all);
    pickEx = [];
    await load();
  }
  async function dropExample(x: string) {
    if (!v) return;
    await setCfg('ai.rescue_examples', v.examples.adopted.filter((e) => e !== x));
    await load();
  }
  async function addGlossary() {
    if (!v) return;
    const chosen = v.glossary.candidates.filter((g) => pickGl.includes(`${g.heard}→${g.script}`)).map((g) => ({ heard: g.heard, script: g.script }));
    const all = [...v.glossary.adopted, ...chosen.filter((c) => !v!.glossary.adopted.some((a) => a.heard === c.heard && a.script === c.script))];
    await setCfg('analysis.glossary', all);
    pickGl = [];
    await load();
  }
  async function dropGlossary(g: Glossary) {
    if (!v) return;
    await setCfg('analysis.glossary', v.glossary.adopted.filter((a) => !(a.heard === g.heard && a.script === g.script)));
    await load();
  }
  function cell(row: EvalRow, sceneId: string): EvalScene | undefined {
    return row.scenes.find((s) => s.scene_id === sceneId);
  }
  const best = $derived.by(() => {
    if (!results || !hold) return null;
    // the variant with the most right and the fewest wrong on the held-out scene
    let top: EvalRow | null = null;
    let sc = -Infinity;
    for (const r of results) {
      const c = cell(r, hold) ?? cell(r, ref);
      if (!c) continue;
      const s = c.found - c.wrong - c.missed;
      if (s > sc) {
        sc = s;
        top = r;
      }
    }
    return top;
  });
</script>

<div class="learn" data-testid="dev-learn">
  {#if !v}
    <p class="muted">Pick an actor session above.</p>
  {:else}
    <section class="glass card">
      <h3>1 · Review a scene</h3>
      <p class="muted">Pick a scene with a lot of {v.character}. Go through it in Under the hood and mark everything wrong or missing; then tick it reviewed. In a reviewed scene, what you did not mark counts as right.</p>
      <div class="pick">
        <label class="lab" for="ref">Reference scene</label>
        <select id="ref" class="field" bind:value={ref} onchange={saveSetup}>
          {#each v.scenes as s (s.id)}<option value={s.id}>Scene {s.number} · {s.heading} ({s.own_lines} lines, {s.takes} takes){s.reviewed ? ' ✓' : ''}</option>{/each}
        </select>
        <button class="btn" onclick={() => sceneBy.get(ref) && open(sceneBy.get(ref)!.number)}>Review in Under the hood</button>
        <label class="chk"><input type="checkbox" checked={sceneBy.get(ref)?.reviewed ?? false} onchange={() => toggleReviewed(ref)} /> reviewed</label>
      </div>
      <div class="pick">
        <label class="lab" for="hold">Held-out scene</label>
        <select id="hold" class="field" bind:value={hold} onchange={saveSetup}>
          {#each v.scenes.filter((s) => s.id !== ref) as s (s.id)}<option value={s.id}>Scene {s.number} · {s.heading} ({s.own_lines} lines, {s.takes} takes){s.reviewed ? ' ✓' : ''}</option>{/each}
        </select>
        <button class="btn" onclick={() => sceneBy.get(hold) && open(sceneBy.get(hold)!.number)}>Review</button>
        <label class="chk"><input type="checkbox" checked={sceneBy.get(hold)?.reviewed ?? false} onchange={() => toggleReviewed(hold)} /> reviewed</label>
      </div>
      <p class="muted small">The held-out scene checks that a change helps beyond the scene it was learned on.</p>
    </section>

    <section class="glass card">
      <h3>2 · Measure settings</h3>
      <p class="muted">Runs the analysis for these two scenes with one setting changed at a time (nothing is applied) and scores each against your marks. With Claude this takes a few minutes.</p>
      <div class="pick">
        <button class="btn primary" disabled={!!job || !ref} onclick={measure}>Try settings</button>
        {#if job}<span class="muted">{job.message || job.stage}… {Math.round(job.progress * 100)}%</span>{/if}
        <span class="muted small">Now: sentences merge at {v.settings.merge_gap_s.toFixed(1)} s · bleed margin {v.settings.bleed_margin_db} dB · matching {v.settings.match_offset >= 0 ? '+' : ''}{v.settings.match_offset} · rescue {v.settings.rescue ? 'on' : 'off'}</span>
      </div>
      {#if results}
        <table class="res">
          <thead>
            <tr>
              <th>Variant</th>
              {#each [ref, hold].filter((x) => x) as id (id)}<th>Sc. {sceneBy.get(id)?.number}{#if !sceneBy.get(id)?.reviewed} <span class="warn" title="Not reviewed: unmarked takes count as right">?</span>{/if}<br /><span class="sub">found · wrong · missed</span></th>{/each}
              <th>AI calls</th>
              <th>Time</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {#each results as r (r.name)}
              <tr class:best={best === r}>
                <td>{r.name}</td>
                {#each [ref, hold].filter((x) => x) as id (id)}
                  {@const c = cell(r, id)}
                  <td class="num">{#if c}<b>{c.found}</b>/{c.truth} · <span class:bad={c.wrong > 0}>{c.wrong}</span> · <span class:bad={c.missed > 0}>{c.missed}</span>{/if}</td>
                {/each}
                <td class="num">{r.ai_calls}</td>
                <td class="num">{r.seconds}s</td>
                <td>{#if r.adopt.length}<button class="btn" onclick={() => adopt(r)}>Adopt</button>{/if}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </section>

    <section class="glass card">
      <h3>3 · Worked examples for Claude</h3>
      <p class="muted">Your linked marks, written as examples. Up to five go into every rescue prompt.</p>
      {#each v.examples.adopted as x (x)}
        <div class="item on"><span>{x}</span><button class="btn" onclick={() => dropExample(x)}>Remove</button></div>
      {/each}
      {#each v.examples.candidates.filter((x) => !v!.examples.adopted.includes(x)) as x (x)}
        <label class="item"><input type="checkbox" checked={pickEx.includes(x)} onchange={() => (pickEx = pickEx.includes(x) ? pickEx.filter((y) => y !== x) : [...pickEx, x])} /><span>{x}</span></label>
      {:else}
        {#if !v.examples.adopted.length}<p class="muted small">No linked marks yet.</p>{/if}
      {/each}
      {#if pickEx.length}<button class="btn primary" onclick={useExamples}>Use as examples</button>{/if}
    </section>

    <section class="glass card">
      <h3>4 · Misheard words</h3>
      <p class="muted">Words the recogniser got wrong in takes that Claude rescued or you linked. Adopted words are told to Claude; when both sides have the same number of words they are also fixed before matching, everywhere, so leave out common words like “how”.</p>
      {#each v.glossary.adopted as g (g.heard + g.script)}
        <div class="item on"><span>“{g.heard}” → “{g.script}”</span><button class="btn" onclick={() => dropGlossary(g)}>Remove</button></div>
      {/each}
      {#each v.glossary.candidates.filter((g) => !v!.glossary.adopted.some((a) => a.heard === g.heard && a.script === g.script)) as g (g.heard + g.script)}
        {@const k = `${g.heard}→${g.script}`}
        <label class="item"><input type="checkbox" checked={pickGl.includes(k)} onchange={() => (pickGl = pickGl.includes(k) ? pickGl.filter((y) => y !== k) : [...pickGl, k])} /><span>“{g.heard}” → “{g.script}” <span class="muted">×{g.count}</span></span></label>
      {:else}
        {#if !v.glossary.adopted.length}<p class="muted small">Nothing yet: rescued and linked takes fill this.</p>{/if}
      {/each}
      {#if pickGl.length}<button class="btn primary" onclick={addGlossary}>Add to glossary</button>{/if}
    </section>
  {/if}
</div>

<style>
  .learn {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 14px;
    align-content: start;
  }
  .card {
    padding: 16px 18px;
    border-radius: var(--radius-panel);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .card:nth-child(-n + 2) {
    grid-column: span 2;
  }
  h3 {
    margin: 0;
    font-size: 14px;
  }
  p {
    margin: 0;
    line-height: 1.45;
  }
  .small {
    font-size: 11.5px;
  }
  .pick {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    align-items: center;
  }
  .pick select {
    flex: 1;
    min-width: 240px;
  }
  .lab {
    width: 120px;
    color: var(--text-3);
  }
  .chk {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--text-2);
  }
  .res {
    width: 100%;
    border-collapse: collapse;
    font-size: 12.5px;
  }
  .res th {
    text-align: left;
    font-weight: 500;
    color: var(--text-3);
    padding: 6px 8px;
    border-bottom: 1px solid var(--glass-outline);
  }
  .res td {
    padding: 6px 8px;
    border-bottom: 1px solid rgb(var(--ink-rgb) / 0.05);
  }
  .res tr.best td {
    background: rgb(var(--locator-rgb) / 0.12);
  }
  .num {
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .sub {
    font-size: 10.5px;
    color: var(--text-4);
  }
  .bad {
    color: var(--warn);
  }
  .warn {
    color: var(--warn);
  }
  .item {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    font-size: 12.5px;
    line-height: 1.4;
    color: var(--text-2);
  }
  .item.on span {
    flex: 1;
    color: var(--text-1);
  }
  .item input {
    margin-top: 2px;
  }
</style>
