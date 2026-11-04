<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { call, inTauri, pickPath } from '../lib/bridge';
  import { grab } from '../lib/drag.svelte';
  import type { Asset, CueView } from '../lib/types';
  import Drawer from './Drawer.svelte';

  let tab = $state<'scene' | 'library'>('scene');
  let query = $state('');
  let library = $state<Asset[]>([]);
  let picked = $state<string[]>([]);
  let adding = $state('');
  let busy = $state<string | null>(null);
  interface Remote { provider: string; title: string; remote_id: string | null; duration_s: number }
  /** Remote search results (Epidemic Sound) per cue, downloaded on demand. */
  let remote = $state<Record<string, Remote[]>>({});
  const cues = $derived<CueView[]>(app.edit?.cues ?? []);

  $effect(() => {
    if (!app.edit) void app.loadEdit();
  });
  $effect(() => {
    if (tab === 'library') void loadLibrary();
  });

  async function loadLibrary() {
    library = (await app.run(() => call<Asset[]>('list_library', query ? { query } : {}))) ?? [];
  }

  async function act(label: string, f: () => Promise<unknown>) {
    busy = label;
    await f();
    busy = null;
    await app.refresh();
    await app.loadEdit();
  }

  const spot = () => act('spot', () => app.method('spot_sound_effects', { scene: app.sceneId }, false));
  const search = (c: CueView) =>
    act(c.cue.id, async () => {
      const r = await app.method<{ results: Remote[]; unavailable: string[] }>('search_sounds', { cue_id: c.cue.id, attach: true }, false);
      remote[c.cue.id] = (r?.results ?? []).filter((x) => x.remote_id);
      const off = (r?.unavailable ?? []).filter((u) => !u.includes('not connected'));
      if (off.length) app.say(off.join('; '));
      if (!r?.results.length) app.say('Nothing found. Connect Epidemic Sound in Settings for more.');
    });
  const fetchRemote = (c: CueView, x: Remote) =>
    act(c.cue.id, async () => {
      await app.method('fetch_sound', { remote_id: x.remote_id, title: x.title, cue_id: c.cue.id }, false);
      remote[c.cue.id] = (remote[c.cue.id] ?? []).filter((y) => y.remote_id !== x.remote_id);
    });
  const generate = (c: CueView) => act(c.cue.id, () => app.method('generate_sound', { cue_id: c.cue.id }, false));
  const choose = (c: CueView, a: Asset) => act(c.cue.id, () => app.method('attach_sound', { cue_id: c.cue.id, asset_id: a.id, choose: true }, false));
  const accept = (c: CueView) => act(c.cue.id, () => app.cmd({ cmd: 'update_cue', cue_id: c.cue.id, patch: { accepted: true } }));
  const dismiss = (c: CueView) => act(c.cue.id, () => app.method('delete_cue', { cue_id: c.cue.id, confirm: true }, false));
  const place = (c: CueView) => {
    const asset = c.cue.chosen ?? c.candidates[0]?.id;
    if (!asset) return app.say('Choose a sound first');
    return act(c.cue.id, () => app.method('place_sound', { asset_id: asset, scene: app.sceneId, cue_id: c.cue.id }, false));
  };
  const merge = () => act('merge', async () => {
    await app.method('merge_cues', { cue_ids: picked, into_id: picked[0], show_level: true }, false);
    picked = [];
  });

  async function add() {
    if (!adding.trim()) return;
    const anchor = app.cur?.line_id;
    await act('add', () => app.method('add_cue', { scene: app.sceneId, description: adding.trim(), cue_type: 'hard', anchor_line_id: anchor }, false));
    adding = '';
  }

  async function importFiles(cue?: CueView) {
    const r = inTauri ? await pickPath({ multiple: true, title: 'Sounds to import', filters: [{ name: 'Audio', extensions: ['wav', 'aif', 'aiff', 'mp3', 'flac'] }] }) : null;
    const paths = r ?? (prompt('Paths of sounds to import, comma separated') ?? '').split(',').map((s) => s.trim()).filter(Boolean);
    if (!paths.length) return;
    await act('import', () => app.method('import_sounds', { paths, cue_id: cue?.cue.id }, false));
    if (tab === 'library') await loadLibrary();
  }

  /** The lanes are on the Edit page: the drag takes you there. */
  function drag(e: MouseEvent, a: Asset, cue_id?: string) {
    grab(e, { kind: 'bucket', asset_id: a.id, cue_id, name: a.file.split('/').pop() ?? a.id, duration_s: a.duration_s }, () => {
      if (app.page !== 'edit') app.setPage('edit');
    });
  }
</script>

<Drawer title="Sound bucket" width={380}>
  <div class="tabs">
    <button class:on={tab === 'scene'} onclick={() => (tab = 'scene')}>This scene</button>
    <button class:on={tab === 'library'} onclick={() => (tab = 'library')}>Show library</button>
  </div>
  {#if tab === 'scene'}
    <div class="row">
      <button class="btn primary" onclick={spot} disabled={busy === 'spot'}>{busy === 'spot' ? 'Spotting…' : 'Spot this scene'}</button>
      <button class="btn" onclick={() => importFiles()}>Import…</button>
      {#if picked.length > 1}<button class="btn" onclick={merge}>Merge {picked.length}</button>{/if}
    </div>
    <div class="row"><input class="field" placeholder="New cue, anchored to the current line" bind:value={adding} onkeydown={(e) => e.key === 'Enter' && add()} /><button class="btn" onclick={add}>Add</button></div>
    {#each cues as c (c.cue.id)}
      <div class="cue" class:new={c.cue.ai_suggested && !c.cue.accepted}>
        <div class="ch">
          <input type="checkbox" aria-label="Select for merge" checked={picked.includes(c.cue.id)} onchange={(e) => (picked = (e.currentTarget as HTMLInputElement).checked ? [...picked, c.cue.id] : picked.filter((x) => x !== c.cue.id))} />
          <span class="desc">{c.cue.description}</span>
          <span class="status {c.cue.status}">{c.cue.status}</span>
        </div>
        <div class="meta">{c.cue.cue_type} · P{c.cue.priority}{c.anchor_text ? ` · “${c.anchor_text.slice(0, 48)}${c.anchor_text.length > 48 ? '…' : ''}”` : ''}</div>
        {#if c.candidates.length}
          <div class="cands">
            {#each c.candidates as a (a.id)}
              <button class="cand" class:chosen={c.cue.chosen === a.id} onmousedown={(e) => drag(e, a, c.cue.id)} onclick={() => choose(c, a)} title="Click to choose, drag onto the timeline">{a.file.split('/').pop()}</button>
            {/each}
          </div>
        {/if}
        {#if remote[c.cue.id]?.length}
          <div class="remote">
            {#each remote[c.cue.id] as x (x.remote_id)}
              <div class="rrow"><span class="ellip">{x.title}</span><span class="muted">{x.duration_s ? `${x.duration_s.toFixed(0)} s · ` : ''}Epidemic</span><button onclick={() => fetchRemote(c, x)} disabled={busy === c.cue.id}>Get</button></div>
            {/each}
          </div>
        {/if}
        <div class="acts">
          {#if c.cue.ai_suggested && !c.cue.accepted}<button onclick={() => accept(c)}>Accept</button><button onclick={() => dismiss(c)}>Dismiss</button>{/if}
          <button onclick={() => search(c)} disabled={busy === c.cue.id}>Search</button>
          <button onclick={() => generate(c)} disabled={busy === c.cue.id}>Generate</button>
          <button onclick={() => importFiles(c)}>Attach file…</button>
          <button onclick={() => place(c)}>Place</button>
        </div>
      </div>
    {:else}
      <p class="muted">No cues yet. “Spot this scene” suggests them from the script.</p>
    {/each}
  {:else}
    <div class="row"><input class="field" placeholder="Search the library" bind:value={query} onkeydown={(e) => e.key === 'Enter' && loadLibrary()} /><button class="btn" onclick={() => importFiles()}>Import…</button></div>
    {#each library as a (a.id)}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions (dragging is the mouse's) -->
      <div class="asset" onmousedown={(e) => drag(e, a)} role="listitem" title="Drag onto the timeline">
        <span class="ellip">{a.file.split('/').pop()}</span><span class="muted">{a.category} · {a.duration_s.toFixed(1)} s</span>
      </div>
    {:else}
      <p class="muted">The library is empty. Import sounds or drop files on the window.</p>
    {/each}
  {/if}
</Drawer>

<style>
  .tabs {
    display: inline-flex;
    padding: 3px;
    border-radius: var(--radius-pill);
    background: var(--glass-fill-2);
    margin-bottom: 10px;
  }
  .tabs button {
    height: 28px;
    padding: 0 12px;
    border-radius: var(--radius-pill);
    font-size: 12px;
    color: var(--text-3);
  }
  .tabs button.on {
    background: var(--button-light-active);
    color: var(--text-on-light);
    font-weight: 600;
  }
  .row {
    display: flex;
    gap: 6px;
    margin-bottom: 8px;
  }
  .row .field {
    flex: 1;
    min-width: 0;
  }
  .row .btn {
    height: 32px;
    padding: 0 12px;
  }
  .cue {
    padding: 10px 0;
    border-bottom: 1px solid var(--glass-outline);
  }
  .cue.new .desc::before {
    content: 'new · ';
    color: var(--locator);
    font-size: 10px;
  }
  .ch {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .desc {
    flex: 1;
    color: var(--text-2);
  }
  .status {
    font-size: 10px;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: var(--radius-pill);
    background: var(--glass-fill-3);
    color: var(--text-3);
  }
  .status.candidates {
    color: var(--locator);
  }
  .status.chosen {
    color: var(--warn);
  }
  .status.placed {
    background: var(--button-light-active);
    color: var(--text-on-light);
  }
  .meta {
    font-size: 11px;
    color: var(--text-4);
    margin: 3px 0 0 24px;
  }
  .cands {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 6px 0 0 24px;
  }
  .cand {
    font-size: 11px;
    padding: 3px 8px;
    border-radius: 8px;
    background: var(--glass-fill-3);
    cursor: grab;
  }
  .cand.chosen {
    box-shadow: inset 0 0 0 1px var(--warn);
  }
  .acts {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    margin: 6px 0 0 24px;
  }
  .acts button {
    font-size: 11px;
    color: var(--text-3);
  }
  .acts button:hover {
    color: var(--text-1);
  }
  .remote {
    margin: 6px 0 0 24px;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .rrow {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 11px;
  }
  .rrow .ellip {
    flex: 1;
    color: var(--text-2);
  }
  .rrow button {
    color: var(--locator);
    font-size: 11px;
  }
  .asset {
    display: flex;
    flex-direction: column;
    padding: 8px 10px;
    margin-bottom: 4px;
    border-radius: 10px;
    background: var(--glass-fill-2);
    cursor: grab;
  }
</style>
