<script lang="ts">
  import { app, recentEpisodes } from '../lib/app.svelte';
  import { brand } from '../lib/brand';
  import { call, inTauri, pickPath } from '../lib/bridge';

  let path = $state('');
  let name = $state('Ep01');
  let folder = $state('');
  let showPath = $state('');
  const recent = recentEpisodes();

  /** A show is a production folder (tm-2610): open it, or make it a show once. */
  async function show(create: boolean) {
    let target = showPath;
    if (inTauri) {
      const r = await pickPath({
        directory: true,
        title: create ? 'The production folder to make a show (e.g. tm-2610)' : 'A show folder',
      });
      if (!r || !r.length) return;
      target = r[0];
    }
    if (!target) return app.say('Give the show folder');
    await app.openShow(target, create);
  }

  async function open(p?: string) {
    let target = p ?? path;
    if (!target && inTauri) {
      const r = await pickPath({ title: 'Open an episode', filters: [{ name: `${brand.displayName} episode`, extensions: [brand.episodeExt] }] });
      if (!r || !r.length) return;
      target = r[0];
    }
    if (!target) return;
    await app.run(() => call('open_project', { path: target }));
    await app.boot();
  }

  /** The same package, read-only: for listening and leaving notes. */
  async function review() {
    let target = path;
    if (inTauri) {
      const r = await pickPath({ title: 'Open a project package for review', filters: [{ name: `${brand.displayName} episode or show`, extensions: [brand.episodeExt, brand.showExt] }] });
      if (!r || !r.length) return;
      target = r[0];
    }
    if (!target) return app.say('Give the path of an episode or a show');
    await app.openForReview(target);
  }

  async function create() {
    let parent = folder;
    if (!parent && inTauri) {
      const r = await pickPath({ directory: true, title: 'Folder for the new episode' });
      if (!r || !r.length) return;
      parent = r[0];
    }
    if (!parent) return app.say('Pick a folder');
    await app.run(() => call('create_episode', { parent, name }));
    await app.boot();
  }

  async function demo() {
    let parent = folder;
    if (!parent && inTauri) {
      const r = await pickPath({ directory: true, title: 'Folder for the demo session' });
      if (!r || !r.length) return;
      parent = r[0];
    }
    if (!parent) return app.say('Pick a folder');
    app.busy = true;
    await app.run(() => call('create_demo', { parent, name: `${brand.displayName} demo` }));
    app.busy = false;
    await app.boot();
  }
</script>

<div class="wrap">
  <div class="glass card">
    <h1>{brand.displayName}</h1>
    <p class="muted">{brand.tagline}</p>
    {#if recent.length}
      <div class="grp">
        <div class="label">Recent</div>
        <div class="recent">
          {#each recent as r (r.path)}
            <button class="rc" onclick={() => (r.show ? app.openShow(r.path) : open(r.path))} title={r.path}>{#if r.show}<span class="tag">Show</span>{/if}<span class="rn">{r.name}</span><span class="muted ellip rp">{r.path}</span></button>
          {/each}
        </div>
      </div>
    {/if}
    <div class="grp">
      <div class="label">Show</div>
      <p class="muted small">A series in its production folder: episodes in Episodes/…/Sound, one sound library in Sound/ for every episode.</p>
      <div class="row">
        {#if !inTauri}<input class="field" placeholder="Show folder (e.g. …/tm-2610)" bind:value={showPath} />{/if}
        <button class="btn primary" onclick={() => show(false)}>Open show…</button>
        <button class="btn" onclick={() => show(true)}>New show from a folder…</button>
      </div>
    </div>
    <div class="grp">
      <div class="label">Standalone episode</div>
      <div class="row">
        {#if !inTauri}<input class="field" placeholder="Path to an episode (.{brand.episodeExt}) or its folder" bind:value={path} />{/if}
        <button class="btn" onclick={() => open()}>Open episode…</button>
        <button class="btn" onclick={review} data-testid="open-review">Open project package in review mode</button>
      </div>
    </div>
    <div class="grp">
      <div class="row">
        <input class="field" style="width: 120px" bind:value={name} aria-label="Episode name" />
        {#if !inTauri}<input class="field" placeholder="Parent folder" bind:value={folder} />{/if}
        <button class="btn" onclick={create}>New episode…</button>
        <button class="btn" onclick={demo} disabled={app.busy}>{app.busy ? 'Building…' : 'Demo session'}</button>
      </div>
    </div>
  </div>
</div>

<style>
  .wrap {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 5;
  }
  .card {
    width: min(620px, 92%);
    padding: 32px 36px;
    border-radius: var(--radius-dialog);
    background: var(--panel);
    box-sizing: border-box;
  }
  h1 {
    margin: 0;
    font-size: 28px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .grp {
    margin-top: 22px;
  }
  .row {
    display: flex;
    gap: 8px;
    margin-top: 8px;
    flex-wrap: wrap;
  }
  .recent {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-top: 8px;
  }
  .rc {
    display: flex;
    align-items: baseline;
    gap: 12px;
    padding: 8px 12px;
    border-radius: 12px;
    text-align: left;
    min-width: 0;
  }
  .rc:hover {
    background: var(--glass-hover);
  }
  .rn {
    font-weight: 600;
    flex: none;
  }
  .rp {
    font-size: 11.5px;
    min-width: 0;
  }
  .small {
    font-size: 12px;
    margin: 6px 0 0;
  }
  .tag {
    flex: none;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    padding: 2px 6px;
    border-radius: 6px;
    background: var(--glass-fill-4);
    color: var(--text-3);
  }
  .row .field {
    flex: 1;
    min-width: 120px;
  }
</style>
