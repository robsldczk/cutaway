<script lang="ts">
  // The marks of the open session: what was pointed at, Claude's proposal, Apply/Discard.
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import { fmt } from '../lib/format';
  import { kindLabel, type DevLine, type DevMark } from '../lib/dev';

  interface Props {
    marks: DevMark[];
    lineById: Map<string, { line: DevLine; scene: number }>;
    busy: boolean;
    onsend: (id: string, manual: boolean) => void;
    reload: () => Promise<void>;
  }
  let { marks, lineById, busy, onsend, reload }: Props = $props();

  const sorted = $derived([...marks].sort((a, b) => (a.status === 'applied' ? 1 : 0) - (b.status === 'applied' ? 1 : 0) || b.created_at.localeCompare(a.created_at)));

  function lineText(id: string | null): string {
    if (!id) return '';
    const l = lineById.get(id);
    return l ? `Sc. ${l.scene} · ${l.line.label} ${l.line.text}` : id;
  }

  async function act(name: string, id: string, msg: string) {
    const r = await app.run(() => call(name, { mark_id: id }));
    if (r === undefined) return;
    app.say(msg);
    await reload();
    await app.refresh();
  }
</script>

<section class="marks" aria-label="Marks">
  <h3>Marks <span class="muted">{marks.length}</span></h3>
  {#each sorted as m (m.id)}
    <div class="mark glass" class:done={m.status === 'applied' || m.status === 'discarded'} data-testid="mark">
      <div class="mh">
        <span class="k">{kindLabel(m.kind)}</span>
        <span class="t muted">{fmt(m.t0)}–{fmt(m.t1)}</span>
        <span class="st {m.status}">{m.status}</span>
      </div>
      <div class="q">“{m.text}”</div>
      {#if m.line_id}<div class="ln muted">→ {lineText(m.line_id)}</div>{/if}
      {#if m.note}<div class="note">{m.note}</div>{/if}
      {#if m.proposal}
        <ul class="sum">
          {#each m.proposal.summary as x, i (i)}<li>{x}</li>{/each}
          {#each m.proposal.remove as id (id)}<li class="rm">removes take {id}</li>{/each}
        </ul>
      {/if}
      <div class="acts">
        {#if m.status === 'proposed'}
          <button class="btn primary" disabled={busy || !m.proposal || (!m.proposal.add.length && !m.proposal.remove.length)} onclick={() => act('apply_mark', m.id, 'Applied')}>Apply</button>
          <button class="btn" disabled={busy} onclick={() => act('discard_mark', m.id, 'Proposal discarded')}>Discard</button>
        {:else if m.status !== 'applied'}
          <button class="btn" disabled={busy} onclick={() => onsend(m.id, false)}>Send to Claude</button>
          {#if m.line_id}<button class="btn" disabled={busy} onclick={() => onsend(m.id, true)}>Make take</button>{/if}
        {/if}
        <button class="btn" disabled={busy} onclick={() => act('delete_mark', m.id, 'Mark removed')}>Delete</button>
      </div>
    </div>
  {:else}
    <p class="muted empty">No marks in this session yet.</p>
  {/each}
</section>

<style>
  h3 {
    margin: 4px 0 8px;
    font-size: 13px;
    font-weight: 600;
  }
  .mark {
    padding: 12px;
    border-radius: var(--radius-panel);
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 8px;
  }
  .mark.done {
    opacity: 0.55;
  }
  .mh {
    display: flex;
    gap: 8px;
    align-items: baseline;
  }
  .k {
    font-weight: 600;
  }
  .t {
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
  }
  .st {
    margin-left: auto;
    font-size: 10.5px;
    padding: 1px 7px;
    border-radius: var(--radius-chip);
    box-shadow: inset 0 0 0 1px var(--text-5);
    color: var(--text-3);
  }
  .st.proposed {
    box-shadow: none;
    background: var(--sfx);
    color: var(--on-sfx);
  }
  .q,
  .note {
    font-size: 12.5px;
    line-height: 1.4;
    color: var(--text-2);
  }
  .note {
    color: var(--text-1);
  }
  .ln {
    font-size: 12px;
  }
  .sum {
    margin: 0;
    padding-left: 16px;
    font-size: 12px;
    color: var(--text-2);
    line-height: 1.45;
  }
  .rm {
    color: var(--warn);
  }
  .acts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .empty {
    font-size: 12px;
  }
</style>
