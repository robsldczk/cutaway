<script lang="ts">
  // The show home (studio mode): the show's backdrop full bleed, its name, and the
  // episode dropdown. Opening an episode without a project makes one in its Sound folder.
  import { app, recentEpisodes } from '../lib/app.svelte';
  import { brand } from '../lib/brand';
  import { call, inTauri, pickPath } from '../lib/bridge';
  import Export from '../pages/Export.svelte';
  import Icon from './Icon.svelte';

  const show = $derived(app.show);
  const eps = $derived(show?.episodes ?? []);
  let picked = $state('');
  /** The episode last worked in, of this show. */
  const lastEp = $derived(recentEpisodes().map((r) => eps.find((e) => e.file === r.path)).find(Boolean));
  const ep = $derived(eps.find((e) => e.folder === picked) ?? lastEp ?? eps.find((e) => e.file) ?? eps[0]);
  const dir = $derived(ep && show ? ep.dir.replace(show.root, '').replace(/^[/\\]/, '') : '');
  let imagePath = $state('');
  let migrating = $state(false);
  let source = $state('');
  let plan = $state<MigratePlan | null>(null);
  let working = $state(false);

  interface MigratePlan {
    target: string;
    files: number;
    bytes: number;
    sounds_to_show_library: string[];
    vocals_stay: number;
    warnings: string[];
  }

  function size(b: number): string {
    return b > 1e9 ? `${(b / 1e9).toFixed(1)} GB` : `${Math.max(1, Math.round(b / 1e6))} MB`;
  }

  async function open() {
    if (ep) await app.openEpisode(ep.folder);
  }

  async function backdrop() {
    let p = imagePath;
    if (inTauri) {
      const r = await pickPath({ title: 'Backdrop for the show', filters: [{ name: 'Image', extensions: ['jpg', 'jpeg', 'png', 'webp'] }] });
      if (!r || !r.length) return;
      p = r[0];
    }
    if (!p) return app.say('Give the path of an image');
    await app.setBackdrop(p);
  }

  async function rescan() {
    const r = await app.run(() => call<{ files: number; folders: number; hashed: number }>('rescan_library'));
    if (r) app.say(`${r.files} sounds in ${r.folders} bucket folders${r.hashed ? `, ${r.hashed} new or changed` : ''}`);
  }

  async function pickSource() {
    if (!inTauri) return;
    const r = await pickPath({ title: 'A standalone episode to copy in', filters: [{ name: `${brand.displayName} episode`, extensions: [brand.episodeExt] }] });
    if (r && r.length) {
      source = r[0];
      plan = null;
    }
  }

  async function check() {
    if (!source || !ep) return;
    working = true;
    const r = await app.run(() => call<{ plan: MigratePlan }>('migrate_episode', { source, episode: ep.folder, dry_run: true }));
    working = false;
    plan = r?.plan ?? null;
  }

  async function copy() {
    if (!source || !ep) return;
    working = true;
    const r = await app.run(() => call<{ missing_sources: number; missing_sounds: string[] }>('migrate_episode', { source, episode: ep.folder }));
    working = false;
    if (!r) return;
    migrating = false;
    plan = null;
    await app.loadShow();
    const missing = r.missing_sources + r.missing_sounds.length;
    app.say(missing ? `Copied, but ${missing} files are missing: open the episode to relink them` : `Copied into ${ep.folder}. The original stays where it was.`);
  }
</script>

{#if show}
  <div class="home">
    {#if app.backdrop}
      <div class="bd" style="background-image: url({app.backdrop})" aria-hidden="true"></div>
    {/if}
    <div class="shade" aria-hidden="true"></div>

    <div class="tools">
      {#if !show.review}
      <button class="tb" onclick={backdrop} title="Set once for the show; it syncs to everyone">{app.backdrop ? 'Change backdrop…' : 'Set backdrop…'}</button>
      <button class="tb" onclick={rescan} title="Read Sound/ again (files from the pipeline)">Rescan library</button>
      <button class="tb" onclick={() => { migrating = !migrating; plan = null; }}>Copy in an episode…</button>
      {/if}
      <button class="tb" onclick={() => app.closeShow()}>Close show</button>
    </div>
    {#if !inTauri && !show.review}
      <input class="field imgpath" placeholder="Image path for the backdrop" bind:value={imagePath} />
    {/if}

    <div class="card">
      <div class="kicker">{show.review ? 'Show, for review' : 'Show'}</div>
      <h1>{show.name}</h1>
      <div class="muted ellip root" title={show.file}>{show.root}</div>

      {#if eps.length}
        <div class="pick">
          <label class="sel glass">
            <select aria-label="Episode" value={ep?.folder} onchange={(e) => (picked = (e.currentTarget as HTMLSelectElement).value)}>
              {#each eps as e (e.folder)}<option value={e.folder}>{e.label}{e.file ? '' : ' (no project yet)'}</option>{/each}
            </select>
            <span class="selv">{ep?.label}</span><Icon name="chevron" size={12} stroke={2.4} />
          </label>
          <button class="btn primary go" onclick={open} disabled={app.busy || (show.review && !ep?.file)}>{app.busy ? 'Opening…' : show.review ? 'Review' : ep?.file ? 'Open' : 'Create and open'}</button>
        </div>
        <div class="muted hint">
          {#if ep?.file}Project in {dir}{:else}Opening makes its project in {dir}{/if}
        </div>
        {#if eps.some((e) => e.file) && !show.review}
          <button class="btn deliver" onclick={() => (app.showExport = true)} data-testid="show-export">Export for the show…</button>
        {/if}
      {:else}
        <p class="muted">No episode folders in {show.root}/Episodes yet.</p>
      {/if}

      {#if migrating}
        <div class="mig glass">
          <div class="label">Copy a standalone episode into {ep?.label ?? 'an episode'}</div>
          <p class="muted small">The original stays as it is. Its sounds go into the show's library.</p>
          <div class="row">
            {#if inTauri}
              <button class="btn" onclick={pickSource}>Choose episode…</button>
              <span class="muted ellip small src">{source || 'nothing chosen'}</span>
            {:else}
              <input class="field" placeholder="Path to a .{brand.episodeExt} file or its folder" bind:value={source} oninput={() => (plan = null)} />
            {/if}
            <button class="btn" onclick={check} disabled={!source || working || !!ep?.file}>Check</button>
          </div>
          {#if ep?.file}<p class="warn small">{ep.label} already has a project.</p>{/if}
          {#if plan}
            <ul class="plan small">
              <li>{plan.files} files, {size(plan.bytes)}, into {plan.target}</li>
              <li>{plan.sounds_to_show_library.length} sounds into the show's library{plan.vocals_stay ? `, ${plan.vocals_stay} vocals stay with the episode` : ''}</li>
              {#each plan.warnings as w (w)}<li class="warn">{w}</li>{/each}
            </ul>
            <button class="btn primary" onclick={copy} disabled={working}>{working ? 'Copying…' : 'Copy'}</button>
          {/if}
        </div>
      {/if}
    </div>
  </div>
  {#if app.showExport}<Export show onclose={() => (app.showExport = false)} />{/if}
{/if}

<style>
  .home {
    position: absolute;
    inset: 0;
    z-index: 5;
    overflow: hidden;
  }
  .bd {
    position: absolute;
    inset: 0;
    pointer-events: none;
    background-size: cover;
    background-position: center;
    transform: scale(1.02);
    animation: settle 1.4s ease-out both;
  }
  @keyframes settle {
    from {
      opacity: 0;
      transform: scale(1.06);
    }
  }
  .shade {
    position: absolute;
    inset: 0;
    pointer-events: none;
    background:
      linear-gradient(90deg, rgb(var(--night-rgb) / 0.82) 0%, rgb(var(--night-rgb) / 0.45) 45%, transparent 75%),
      linear-gradient(0deg, rgb(var(--night-rgb) / 0.75) 0%, transparent 55%);
  }
  .tools {
    position: absolute;
    top: 18px;
    right: 24px;
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: flex-end;
    z-index: 2;
  }
  .tb {
    height: 34px;
    padding: 0 14px;
    border-radius: var(--radius-pill);
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-strong);
    background: rgb(var(--night-rgb) / 0.42);
    box-shadow: inset 0 0 0 1px var(--glass-outline);
    backdrop-filter: var(--glass-blur);
    -webkit-backdrop-filter: var(--glass-blur);
  }
  .tb:hover {
    background: rgb(var(--night-rgb) / 0.6);
  }
  .imgpath {
    position: absolute;
    top: 60px;
    right: 24px;
    width: 320px;
    z-index: 2;
  }
  .card {
    position: absolute;
    left: clamp(24px, 6vw, 88px);
    bottom: clamp(32px, 9vh, 96px);
    width: min(560px, calc(100% - 48px));
    color: var(--text-strong);
  }
  .kicker {
    font-size: 11px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--text-3);
  }
  h1 {
    margin: 4px 0 2px;
    font-size: clamp(36px, 6vw, 64px);
    line-height: 1;
    font-weight: 750;
    letter-spacing: -0.02em;
    text-shadow: 0 2px 24px rgb(var(--shadow-rgb) / 0.5);
  }
  .root {
    font-size: 12px;
    max-width: 100%;
  }
  .pick {
    display: flex;
    gap: 8px;
    margin-top: 22px;
    align-items: stretch;
  }
  .sel {
    position: relative;
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 14px;
    min-height: 40px;
    border-radius: var(--radius-pill);
    font-weight: 600;
    cursor: pointer;
    min-width: 0;
  }
  .sel select {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }
  .selv {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .go {
    min-width: 120px;
  }
  .hint {
    margin-top: 8px;
    font-size: 12px;
  }
  .mig {
    margin-top: 18px;
    padding: 14px 16px;
    border-radius: 16px;
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 8px;
  }
  .row .field {
    flex: 1;
  }
  .src {
    flex: 1;
    min-width: 0;
  }
  .small {
    font-size: 12px;
  }
  .plan {
    margin: 10px 0;
    padding-left: 18px;
  }
  .warn {
    color: var(--warn);
  }
  .deliver {
    margin-top: 14px;
  }
</style>
