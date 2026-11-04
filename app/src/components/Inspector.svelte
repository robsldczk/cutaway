<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { tc } from '../lib/format';
  import Drawer from './Drawer.svelte';
  const it = $derived(app.cur);
  const t = $derived(it && it.takes.length ? it.takes[it.take_index ?? 0] : undefined);
  const story = $derived(app.view?.story);
  const who = $derived(it?.character ? app.view?.characters.find((c) => c.id === it.character) : undefined);
  const mood = (x: number) => (x <= -0.6 ? 'low point' : x < -0.15 ? 'going badly' : x <= 0.15 ? 'even' : x < 0.6 ? 'going well' : 'high point');
  const hasStory = $derived(!!story && !!(story.title || story.synopsis || story.act || story.cast.length || story.setting_description || story.setting_parent));
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
      <dt>Room before</dt><dd>{t.direction || '—'}</dd>
      <dt>Room after</dt><dd>{t.feedback || '—'}</dd>
      {#if app.dev}
        <dt>Room before (raw)</dt><dd>{t.direction_raw || '—'}</dd>
        <dt>Room after (raw)</dt><dd>{t.feedback_raw || '—'}</dd>
      {/if}
      {#if t.ai_flag}<dt>Flag</dt><dd class="warn">{t.ai_flag}</dd>{/if}
      {#if t.needs_rematch}<dt>Script</dt><dd class="warn">the line changed in a new script version; re-run the analysis</dd>{/if}
    </dl>
  {:else}
    <p class="muted">No take selected.</p>
  {/if}
  {#if who?.profile}
    <h3>{who.label}</h3>
    <p class="prose">{who.profile}</p>
  {/if}
  {#if story && hasStory}
    <h3>Scene{story.title ? `: ${story.title}` : ''}</h3>
    <dl>
      {#if story.act}<dt>Act</dt><dd>{story.act}</dd>{/if}
      {#if story.synopsis}<dt>Writers’ summary</dt><dd class="prose">{story.synopsis}</dd>{/if}
      {#if story.cast.length}<dt>In the scene</dt><dd>{story.cast.join(', ')}</dd>{/if}
      {#if story.tone != null}<dt>Story arc</dt><dd>{mood(story.tone)} ({story.tone > 0 ? '+' : ''}{story.tone.toFixed(2)})</dd>{/if}
      <dt>Setting</dt><dd>{story.setting}{story.setting_parent ? ` · part of ${story.setting_parent}` : ''}</dd>
      {#if story.setting_description}<dd class="prose">{story.setting_description}</dd>{/if}
      {#if story.parent_description}<dd class="prose">{story.setting_parent}: {story.parent_description}</dd>{/if}
    </dl>
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
  h3 {
    margin: 22px 0 0;
    padding-top: 14px;
    border-top: 1px solid var(--glass-outline);
    font-size: var(--fs-ui);
    font-weight: 600;
    color: var(--text-1);
  }
  .prose {
    white-space: pre-line;
  }
</style>
