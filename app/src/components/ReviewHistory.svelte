<script lang="ts">
  // What happened, without opening a file: rounds and arrivals, all from the merged events.
  // Add review lives here on the editing side; Send review on the reviewer's.
  import { onMount } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { brand } from '../lib/brand';
  import { BridgeError, call, inTauri, pickPath } from '../lib/bridge';
  import { arrivalLine, day, roundLine, type ReviewHistory, type Tally } from '../lib/review';
  import Sheet from './Sheet.svelte';

  let h = $state<ReviewHistory | null>(null);
  let path = $state('');
  let folder = $state('');
  let refused = $state<{ path: string; why: string } | null>(null);
  let added = $state('');

  const editing = $derived(!app.inReview);
  const rev = $derived(app.status?.rev);

  async function load() {
    h = (await app.run(() => call<ReviewHistory>('review_history'))) ?? h;
  }
  onMount(load);
  $effect(() => {
    if (rev !== undefined) void load();
  });

  const line = (t: Tally) => [t.open ? `${t.open} open` : '', t.resolved ? `${t.resolved} resolved` : '', t.reopened ? `${t.reopened} reopened` : ''].filter(Boolean).join(' · ') || 'no notes';

  async function add(p: string, anyway = false) {
    refused = null;
    added = '';
    try {
      const r = await call<{ who: string; notes: number; replies: number; notice: string | null }>('add_review', { path: p, anyway });
      added = `${r.who}: ${r.notes} new ${r.notes === 1 ? 'note' : 'notes'}, ${r.replies} ${r.replies === 1 ? 'reply' : 'replies'}${r.notice ? `. ${r.notice}` : ''}`;
      path = '';
      await app.refresh();
      await load();
    } catch (e) {
      const why = e instanceof BridgeError ? e.message : String(e);
      if (why.includes('Its notes would come in detached')) refused = { path: p, why };
      else app.say(why);
    }
  }

  async function pick() {
    if (inTauri) {
      const r = await pickPath({ title: 'Add a review', filters: [{ name: `${brand.displayName} review`, extensions: [brand.reviewExt] }] });
      if (r?.length) await add(r[0]);
    } else if (path.trim()) await add(path.trim());
  }

  async function moveFolder() {
    let p = folder.trim();
    if (inTauri) {
      const r = await pickPath({ directory: true, title: 'The shared folder for review files' });
      if (!r?.length) return;
      p = r[0];
    }
    if (!p) return;
    await app.method('set_reviews_folder', { path: p }, false);
    folder = '';
    await load();
  }
</script>

<Sheet title="Review history" width={680}>
  {#if h}
    {#if editing}
      <div class="add">
        {#if !inTauri}<input class="field" placeholder="Path to a .{brand.reviewExt} file" bind:value={path} data-testid="add-review-path" />{/if}
        <button class="btn primary" onclick={pick} data-testid="add-review">Add review…</button>
        <span class="muted small">or drop the file on the window</span>
      </div>
      {#if added}<p class="ok small" data-testid="add-review-done">{added}</p>{/if}
      {#if refused}
        <div class="warn small" data-testid="add-review-refused">{refused.why} <button class="link" onclick={() => refused && add(refused.path, true)}>Add it anyway</button></div>
      {/if}
    {/if}

    <div class="label">Rounds</div>
    {#each h.rounds as r (r.id)}
      <div class="round" data-testid="history-round">
        <div class="rh"><b>Round {r.number}</b> <span class="muted">{roundLine(r)}</span>{#if r.status === 'closed'}<span class="muted"> · closed</span>{/if}</div>
        <div class="rl muted">{r.reviewers.length ? r.reviewers.join(', ') : 'No reviewers yet'} · {line(r.summary.all)}</div>
        {#if r.notes}
          <div class="grid">
            {#each Object.entries(r.summary.scenes) as [n, t] (n)}<span>Sc. {n}</span><span class="muted">{line(t)}</span>{/each}
            {#each Object.entries(r.summary.people) as [who, t] (who)}<span>{who}</span><span class="muted">{line(t)}</span>{/each}
          </div>
        {/if}
      </div>
    {:else}
      <p class="muted">No rounds yet. The Export page opens scenes for review.</p>
    {/each}

    <div class="label">Arrivals</div>
    {#each h.arrivals as a (a.id)}
      <div class="row small" data-testid="history-arrival">{arrivalLine(a)}</div>
    {:else}
      <p class="muted small">Nothing has come in yet.</p>
    {/each}

    {#each h.troubles as t (t.file)}<p class="warn small">{t.file}: {t.text}</p>{/each}
    {#if h.detached}<p class="muted small">{h.detached} {h.detached === 1 ? 'note is' : 'notes are'} detached: {h.detached === 1 ? 'its element has' : 'their elements have'} left the cut. They're listed in their scenes.</p>{/if}

    <div class="label">Shared folder</div>
    <div class="fold"><span class="muted small ellip" title={h.folder}>{h.folder}</span>
      {#if editing && app.status?.show}
        {#if !inTauri}<input class="field" placeholder="Another shared folder" bind:value={folder} />{/if}
        <button class="btn" onclick={moveFolder}>Change…</button>
      {/if}
    </div>
    {#if h.rounds[0]}<p class="muted small">Latest round {day(h.rounds[0].created_at)}, by {h.rounds[0].created_by.name}.</p>{/if}
  {/if}
</Sheet>

<style>
  .add {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
  }
  .add .field {
    flex: 1;
  }
  .small {
    font-size: 12px;
  }
  .ok {
    color: var(--link);
    margin: 4px 0;
  }
  .warn {
    color: var(--warn);
    margin: 4px 0;
  }
  .link {
    color: var(--link);
    text-decoration: underline;
  }
  .label {
    margin-top: 16px;
  }
  .round {
    padding: 8px 0;
    border-bottom: 1px solid var(--glass-outline);
    font-size: 12.5px;
  }
  .rl {
    margin-top: 2px;
    font-size: 12px;
  }
  .grid {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 2px 14px;
    margin-top: 6px;
    font-size: 12px;
  }
  .row {
    padding: 5px 0;
    border-bottom: 1px solid var(--glass-outline);
  }
  .fold {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .fold .ellip {
    flex: 1;
    min-width: 0;
  }
</style>
