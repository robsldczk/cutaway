<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { tc } from '../lib/format';
  import Drawer from './Drawer.svelte';
  const it = $derived(app.cur);
  const t = $derived(it && it.takes.length ? it.takes[it.take_index ?? 0] : undefined);
</script>

<Drawer title="Inspector">
  {#if it && t}
    <dl>
      <dt>Take</dt><dd>T{t.take_no} · {t.take_type}{t.partial ? ' (partial)' : ''}</dd>
      <dt>Source</dt><dd class="ellip" title={t.source_name}>{t.source_name}</dd>
      <dt>Source TC in / out</dt><dd>{tc(t.tc_in)} – {tc(t.tc_in + t.duration)}</dd>
      <dt>Duration</dt><dd>{t.duration.toFixed(2)} s (box {(it.end_s - it.start_s).toFixed(2)} s)</dd>
      <dt>Match score</dt><dd>{t.score.toFixed(0)}%</dd>
      <dt>Loudness</dt><dd>{t.lufs != null ? `${t.lufs.toFixed(1)} LUFS` : '—'}</dd>
      <dt>Favorite</dt><dd>{t.favorite ? (t.favorite_origin === 'director' ? 'director pick' : 'yours') : '—'}{t.pick === 1 ? ' · director liked it' : ''}</dd>
      <dt>Tags</dt><dd>{t.tags.join(', ') || '—'}</dd>
      <dt>Heard</dt><dd class="script">{t.heard}</dd>
      <dt>Room before (direction)</dt><dd>{t.direction_raw || '—'}</dd>
      <dt>Room after (feedback)</dt><dd>{t.feedback_raw || '—'}</dd>
      {#if t.ai_flag}<dt>Flag</dt><dd class="warn">{t.ai_flag}</dd>{/if}
      {#if t.needs_rematch}<dt>Script</dt><dd class="warn">the line changed in a new script version; re-run the analysis</dd>{/if}
    </dl>
  {:else}
    <p class="muted">No take selected.</p>
  {/if}
</Drawer>

<style>
  dl {
    margin: 0;
  }
  dt {
    margin-top: 12px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: var(--name-tracking);
    text-transform: uppercase;
    color: var(--text-4);
  }
  dd {
    margin: 3px 0 0;
    color: var(--text-2);
    line-height: 1.45;
  }
  .warn {
    color: var(--warn);
  }
</style>
