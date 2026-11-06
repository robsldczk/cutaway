<script lang="ts">
  // New episode and New show: a name, the show it belongs to, a cover. What's made is selected
  // on the shelf and stays closed until Edit is chosen.
  import { app, remember } from '../lib/app.svelte';
  import { call, choosePath } from '../lib/bridge';
  import { coverUrl } from '../lib/covers.svelte';
  import { fileDrag } from '../lib/filedrop.svelte';
  import { rgba } from '../theme';
  import { IMAGE_EXTENSIONS, showColor, type ShowCard } from '../lib/welcome';
  import Sheet from './Sheet.svelte';

  export interface Ask {
    kind: 'episode' | 'show';
    name: string;
    /** The show it's pre-selected into, by its key. */
    show: string | null;
    /** What was dropped to start it. */
    scripts: string[];
    audio: string[];
  }

  let { ask, shows, folder, oncreated, onclose }: { ask: Ask; shows: ShowCard[]; folder: string; oncreated: (key: string) => void; onclose: () => void } = $props();

  const isShow = $derived(ask.kind === 'show');
  // the sheet is made for one question: what it starts from doesn't change while it's up
  // svelte-ignore state_referenced_locally
  let name = $state(ask.name);
  // svelte-ignore state_referenced_locally
  let show = $state(ask.show);
  let cover = $state<string | null>(null);
  /** The folder picked: a show's own, or where an episode goes. */
  let picked = $state<string | null>(null);
  let working = $state(false);

  const trimmed = $derived(name.trim());
  const home = $derived(picked ?? folder);
  const where = $derived(isShow && picked ? picked : `${home}/${trimmed || '…'}`);
  const coverImage = $derived(cover ? coverUrl({ path: cover, stamp: '' }, 240) : null);
  const zone = 'cover:sheet';

  export function setCover(path: string) {
    cover = path;
  }

  export function create() {
    void make();
  }

  async function chooseCover() {
    const p = await choosePath({ title: 'Cover image', filters: [{ name: 'Image', extensions: IMAGE_EXTENSIONS }] });
    if (p) cover = p;
  }

  async function chooseFolder() {
    const p = await choosePath({ directory: true, title: isShow ? 'The show’s folder: a production folder stays as it is' : 'Folder for the new episode' });
    if (!p) return;
    picked = p;
    if (isShow && !trimmed) name = p.split(/[/\\]/).filter(Boolean).pop() ?? '';
  }

  /** The files dropped to start the episode go in now, while it's open for a moment. */
  async function bring(file: string) {
    try {
      for (const s of ask.scripts) await call('import_script', { path: s });
      if (ask.audio.length) {
        await call('import_audio', { paths: ask.audio, copy: true });
        await call('match_files');
      }
    } finally {
      await call('close_show');
    }
    return file;
  }

  async function makeEpisode() {
    const filled = ask.scripts.length + ask.audio.length > 0;
    const r = await app.run(() => call<{ file: string }>('create_episode', { name: trimmed, ...(show ? { show } : { parent: home }), open: filled }));
    if (!r) return null;
    if (filled) await app.run(() => bring(r.file));
    if (cover) await app.run(() => call('set_cover', { target: r.file, image: cover }));
    const owner = shows.find((s) => s.key === show);
    remember({ name: owner ? `${owner.name} · ${trimmed}` : trimmed, path: r.file });
    return r.file;
  }

  async function makeShow() {
    const r = await app.run(() => call<{ root: string }>('create_show', { name: trimmed, ...(picked ? { root: picked } : { parent: folder }), open: false }));
    if (!r) return null;
    if (cover) await app.run(() => call('set_cover', { target: r.root, image: cover }));
    remember({ name: trimmed, path: r.root, show: true });
    return r.root;
  }

  async function make() {
    if (!trimmed || working) return;
    working = true;
    const key = await (isShow ? makeShow() : makeEpisode());
    working = false;
    if (key) oncreated(key);
  }

  const focus = (el: HTMLInputElement) => {
    el.focus();
    el.select();
  };
</script>

<Sheet title={isShow ? 'New show' : 'New episode'} width={460} {onclose}>
  <div class="head">
    <button class="well" class:has={!!coverImage} class:drop={fileDrag.over === zone} data-file-zone={zone} style={coverImage ? `background-image: url("${coverImage}")` : ''} onclick={chooseCover} aria-label="Cover image" data-testid="sheet-cover">
      {#if !coverImage}
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="5" width="18" height="14" rx="3" /><path d="M3 16l5-5 4 4 3-3 6 6" /></svg>
        <span>Cover</span>
      {/if}
    </button>
    <input class="inp" use:focus bind:value={name} placeholder={isShow ? 'Show name' : 'Episode title'} aria-label={isShow ? 'Show name' : 'Episode title'} autocomplete="off" spellcheck="false" />
  </div>

  {#if !isShow}
    <div class="chips" role="radiogroup" aria-label="Show">
      {#each shows as s (s.key)}
        {@const c = showColor(s)}
        <button class="chip" class:on={show === s.key} role="radio" aria-checked={show === s.key} style="--chip: {rgba(c, 0.9)}" onclick={() => (show = s.key)}><span style="background: {rgba(c, 1)}"></span>{s.name}</button>
      {/each}
      <button class="chip" class:on={show === null} role="radio" aria-checked={show === null} onclick={() => (show = null)}><span class="none"></span>No show</button>
    </div>
  {/if}

  {#if isShow || !show}
    <div class="where">
      <span class="ellip" title={where} data-testid="sheet-where">{where}</span>
      <button class="link" onclick={chooseFolder}>Choose…</button>
    </div>
  {/if}

  {#if ask.scripts.length + ask.audio.length}
    <div class="files">Starts with {ask.scripts.length + ask.audio.length} {ask.scripts.length + ask.audio.length === 1 ? 'file' : 'files'}</div>
  {/if}

  <div class="foot">
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn primary" onclick={make} disabled={!trimmed || working}>{working ? 'Creating…' : 'Create'} <span class="kb on">↩</span></button>
  </div>
</Sheet>

<style>
  .head {
    display: flex;
    gap: 16px;
    align-items: center;
  }
  .well {
    width: 92px;
    height: 92px;
    flex: none;
    border-radius: 16px;
    border: 1.5px dashed var(--text-dim);
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    font-size: 10.5px;
    color: var(--text-5);
    background-size: cover;
    background-position: center;
  }
  .well:hover {
    color: var(--text-3);
  }
  .well.has {
    border: 0;
  }
  .well.drop {
    border: 1.5px solid var(--accent);
  }
  .inp {
    flex: 1;
    min-width: 0;
    height: 44px;
    box-sizing: border-box;
    padding: 0 14px;
    border: 0;
    outline: none;
    border-radius: 12px;
    font-size: 15px;
    font-weight: 500;
    background: var(--glass-fill-4);
    box-shadow: inset 0 0 0 1.5px rgb(var(--accent-rgb) / 0.8);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 16px;
  }
  .chip {
    height: 28px;
    padding: 0 11px;
    border-radius: var(--radius-pill);
    display: inline-flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-3);
    box-shadow: inset 0 0 0 1px var(--glass-outline);
  }
  .chip span {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  .chip .none {
    background: var(--text-5);
  }
  .chip.on {
    color: var(--text-strong);
    background: var(--glass-active);
    box-shadow: inset 0 0 0 1.5px var(--chip, var(--text-4));
  }
  .where {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin-top: 14px;
    font-size: 12px;
    color: var(--text-5);
  }
  .where .ellip {
    flex: 1;
    min-width: 0;
    direction: rtl;
    text-align: left;
  }
  .link {
    flex: none;
    font-weight: 600;
    color: var(--text-3);
  }
  .link:hover {
    color: var(--text-1);
  }
  .files {
    margin-top: 10px;
    font-size: 12px;
    color: var(--text-4);
  }
  .foot {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 20px;
  }
  .kb.on {
    background: rgb(var(--shadow-rgb) / 0.12);
    color: inherit;
    margin-left: 6px;
  }
</style>
