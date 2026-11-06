<script lang="ts">
  // The first screen: a hero for what's selected, recent episodes, shows. A click selects; only
  // Edit, Review, ↩ and ⌥↩ open anything (D144).
  import { onDestroy, onMount, tick } from 'svelte';
  import { fade } from 'svelte/transition';
  import { app, forgetRecent, recentEpisodes, relinkRecent, remember } from '../lib/app.svelte';
  import { brand } from '../lib/brand';
  import { call, choosePath } from '../lib/bridge';
  import { coverUrl, HERO_PX, TILE_PX } from '../lib/covers.svelte';
  import { AUDIO, fileDrag, zoneAt } from '../lib/filedrop.svelte';
  import { rgba } from '../theme';
  import { back, episodesText, first, IMAGE_EXTENSIONS, itemColor, itemOf, kicker, legible, metaOf, move, nameFrom, notesOf, notesText, reach, rows, select, settle, SINGLE, wash, type EpisodeCard, type Item, type Selection, type Tile, type WelcomeView } from '../lib/welcome';
  import Icon from './Icon.svelte';
  import NewSheet, { type Ask } from './NewSheet.svelte';
  import ShelfMenu from './ShelfMenu.svelte';
  import TakeDeck from './TakeDeck.svelte';

  const IMAGE = new RegExp(`\\.(${IMAGE_EXTENSIONS.join('|')})$`, 'i');
  const SCRIPT = /\.(fountain|thrl|txt)$/i;
  const isReview = (p: string) => p.endsWith(`.${brand.reviewExt}`);

  let view = $state<WelcomeView | null>(null);
  let sel = $state<Selection>({ scope: null, row: 0, key: 'new-episode', item: null });
  let ask = $state<Ask | null>(null);
  let sheet = $state<NewSheet>();
  let menu = $state<{ x: number; y: number; item: Item } | null>(null);
  let lists = $state<(HTMLDivElement | undefined)[]>([]);
  let loads = 0;

  const both = $derived(view ? rows(view, sel.scope) : null);
  const item = $derived(view ? itemOf(view, sel) : null);
  const lost = $derived(item?.kind === 'episode' ? item.episode.missing : item?.kind === 'show' ? item.show.missing : false);
  const canEdit = $derived(!!item && item.kind !== 'single' && !lost);
  const canReview = $derived(canEdit && (item?.kind === 'show' || !!(item?.kind === 'episode' && item.episode.file)));
  const notes = $derived(item ? notesOf(item) : 0);
  const scoped = $derived(view?.shows.find((s) => s.key === sel.scope));
  const title = $derived(sel.scope === SINGLE ? 'Single episodes' : (scoped?.name ?? null));

  const coverOf = (i: Item) => (i.kind === 'episode' ? i.episode.cover : i.kind === 'show' ? i.show.cover : null);
  const titleOf = (i: Item) => (i.kind === 'episode' ? i.episode.title : i.kind === 'show' ? i.show.name : 'Single episodes');

  /** The small line on a tile. */
  function smallOf(i: Item): string {
    if (i.kind === 'episode') return kicker(i.episode);
    return episodesText(i.kind === 'show' ? i.show.episodes.length : i.episodes.length);
  }

  /** A CSS background: the cover, or the colour wash. */
  function face(i: Item, px: number): string {
    const ref = coverOf(i);
    const url = ref ? (coverUrl(ref, px) ?? coverUrl(ref, TILE_PX)) : null;
    return url ? `url("${url}") center / cover` : wash(view ? itemColor(view, i) : [200, 200, 210]);
  }

  const heroKicker = $derived.by(() => {
    if (!item || item.kind === 'single') return '';
    return item.kind === 'show' ? 'Show' : kicker(item.episode);
  });
  const heroTint = $derived(view && item ? rgba(legible(itemColor(view, item), app.theme), 1) : '');

  async function load(then?: string) {
    const n = ++loads;
    const recents = recentEpisodes();
    const v = await app.run(() => call<WelcomeView>('welcome_view', { recents }));
    if (!v || n !== loads) return;
    const fresh = !view;
    view = v;
    sel = then ? reach(v, sel, then) : fresh ? first(v, recents) : settle(v, sel, recents);
  }

  function pick(row: 0 | 1, t: Tile) {
    sel = select(sel, row, t);
    if (t.kind === 'new-episode') newEpisode();
    else if (t.kind === 'new-show') newShow();
  }

  function showBack() {
    if (view) sel = back(view, sel, recentEpisodes());
  }

  /** What a new episode belongs to: the show that's selected, or the selected episode's. */
  function currentShow(): string | null {
    const key = item?.kind === 'show' ? item.key : item?.kind === 'episode' ? item.episode.show : sel.scope;
    return view?.shows.find((s) => s.key === key && !s.missing)?.key ?? null;
  }

  function newEpisode(name = '', scripts: string[] = [], audio: string[] = []) {
    ask = { kind: 'episode', name, show: currentShow(), scripts, audio };
  }

  function newShow() {
    ask = { kind: 'show', name: '', show: null, scripts: [], audio: [] };
  }

  function closeSheet() {
    ask = null;
    (document.activeElement as HTMLElement | null)?.blur?.();
  }

  function created(key: string) {
    closeSheet();
    void load(key);
  }

  /** Open… puts it on the shelf and selects it: how to open it is still to be chosen. */
  async function openFile() {
    const p = await choosePath({ title: 'Open an episode or a show', filters: [{ name: `${brand.displayName} episode or show`, extensions: [brand.episodeExt, brand.showExt] }] });
    if (!p) return;
    const show = p.endsWith(`.${brand.showExt}`);
    const path = show ? p.replace(/[/\\][^/\\]+$/, '') : p;
    remember({ name: nameFrom(p), path, show });
    await load(path);
  }

  /** Takes it over from the shelf, for what opens next. */
  function keepOnShelf(i: Exclude<Item, { kind: 'single' }>) {
    if (i.kind === 'show') remember({ name: i.show.name, path: i.show.root, show: true });
    else if (i.episode.file) remember({ name: i.episode.show_name ? `${i.episode.show_name} · ${i.episode.title}` : i.episode.title, path: i.episode.file });
  }

  async function launch(e: EpisodeCard, mode: 'edit' | 'review') {
    if (mode === 'review') return e.file ? app.openForReview(e.file) : false;
    if (e.file) return app.openFile(e.file);
    if (!e.show || !e.folder) return false;
    await app.openShow(e.show);
    await app.openEpisode(e.folder);
    return !app.noProject;
  }

  async function open(mode: 'edit' | 'review') {
    const i = item;
    if (!i || i.kind === 'single' || app.opening) return;
    if (lost) return void locate(i);
    if (mode === 'review' ? !canReview : !canEdit) return;
    app.opening = { title: titleOf(i), kicker: i.kind === 'show' ? 'Show' : kicker(i.episode), mode, image: face(i, HERO_PX) };
    let ok: boolean | undefined;
    try {
      if (i.kind === 'episode') ok = await launch(i.episode, mode);
      else if (mode === 'review') ok = await app.openForReview(i.show.root);
      else ok = await app.openShow(i.show.root);
    } finally {
      app.opening = null;
    }
    if (ok && mode === 'review') keepOnShelf(i);
    if (!ok) await load();
  }

  async function locate(i: Item) {
    if (i.kind === 'single') return;
    const show = i.kind === 'show';
    const p = await choosePath({ title: `Locate ${titleOf(i)}`, filters: [{ name: `${brand.displayName} ${show ? 'show' : 'episode'}`, extensions: [show ? brand.showExt : brand.episodeExt] }] });
    if (!p) return;
    const path = show ? p.replace(/[/\\][^/\\]+$/, '') : p;
    relinkRecent(i.key, path);
    await load(path);
  }

  async function setCover(i: Item, image: string) {
    if (i.kind === 'single') return;
    const target = i.kind === 'show' ? i.show.root : (i.episode.file ?? i.episode.dir);
    if (await app.run(() => call('set_cover', { target, image }))) await load();
  }

  async function chooseCover(i: Item) {
    const p = await choosePath({ title: `Cover for ${titleOf(i)}`, filters: [{ name: 'Image', extensions: IMAGE_EXTENSIONS }] });
    if (p) await setCover(i, p);
  }

  /** Off the list, and with a show its episodes, which would bring it straight back. */
  async function remove(i: Item) {
    if (i.kind === 'single') return;
    if (i.kind === 'show') forgetRecent([i.show.root, ...i.show.episodes.map((e) => e.key)]);
    else forgetRecent([i.key]);
    await load();
  }

  function openMenu(e: MouseEvent, i: Item) {
    e.preventDefault();
    if (i.kind !== 'single') menu = { x: e.clientX, y: e.clientY, item: i };
  }

  const menuItems = $derived.by(() => {
    const i = menu?.item;
    if (!i || i.kind === 'single') return [];
    const gone = i.kind === 'episode' ? i.episode.missing : i.show.missing;
    const listed = i.kind === 'show' || view?.episodes.some((e) => e.key === i.key);
    return [...(gone ? [{ label: 'Locate…', run: () => void locate(i) }] : [{ label: 'Set cover…', run: () => void chooseCover(i) }]), ...(listed ? [{ label: 'Remove from recents', run: () => void remove(i) }] : [])];
  });

  const dropping = $derived.by(() => {
    const p = fileDrag.all;
    if (!p.length || ask) return null;
    if (p.every((x) => IMAGE.test(x))) return 'cover';
    return p.some(isReview) ? 'review' : 'episode';
  });

  /** The tile or hero an image is over. */
  function zoneItem(x: number, y: number): Item | null {
    const key = zoneAt(x, y)?.replace(/^cover:/, '');
    if (!key || !view) return null;
    if (key === 'hero') return item;
    return itemOf(view, { ...sel, item: key });
  }

  async function addReview(path: string) {
    const r = await app.run(() => call<{ who: string; notes: number; replies: number; notice: string | null; episodes: string[]; files: string[] }>('add_review_to', { path, recents: recentEpisodes() }));
    if (!r) return;
    app.say(`${r.who}: ${r.notes} new ${r.notes === 1 ? 'note' : 'notes'}, ${r.replies} ${r.replies === 1 ? 'reply' : 'replies'} on ${r.episodes.join(', ')}${r.notice ? `. ${r.notice}` : ''}`);
    await load(r.files[0]);
  }

  async function drop(paths: string[], x: number, y: number) {
    const images = paths.filter((p) => IMAGE.test(p));
    if (ask) {
      if (images[0]) sheet?.setCover(images[0]);
      return;
    }
    for (const r of paths.filter(isReview)) await addReview(r);
    const scripts = paths.filter((p) => SCRIPT.test(p));
    const audio = paths.filter((p) => AUDIO.test(p));
    if (scripts.length || audio.length) newEpisode(nameFrom(scripts[0] ?? audio[0]), scripts, audio);
    else if (images[0]) {
      const on = zoneItem(x, y);
      if (on && on.kind !== 'single') await setCover(on, images[0]);
      else app.say('Drop the image on a tile or the hero');
    }
  }

  function key(e: KeyboardEvent): boolean {
    if (app.opening) return true;
    if (app.overlay) return false;
    const k = e.key;
    const t = e.target as HTMLElement | null;
    // a button reached with Tab does what it says
    const tabbed = t?.tagName === 'BUTTON' && t.matches(':focus-visible');
    if (menu) {
      if (k === 'Escape') menu = null;
      else if (k === 'ArrowDown' || k === 'ArrowUp') {
        const all = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')];
        const i = all.indexOf(document.activeElement as HTMLElement);
        all[(i + (k === 'ArrowDown' ? 1 : all.length - 1)) % all.length]?.focus();
      } else return k === 'Enter' ? false : true;
      return true;
    }
    if (ask) {
      if (k === 'Escape') closeSheet();
      else if (k === 'Enter' && !tabbed) sheet?.create();
      else return false;
      return true;
    }
    if (e.metaKey || e.ctrlKey) {
      const c = k.toLowerCase();
      if (c === 'n' && e.shiftKey) newShow();
      else if (c === 'n') newEpisode();
      else if (c === 'o' && !e.shiftKey) void openFile();
      else return false;
      return true;
    }
    if (!view) return false;
    if (k === 'ArrowLeft') sel = move(view, sel, -1, 0);
    else if (k === 'ArrowRight') sel = move(view, sel, 1, 0);
    else if (k === 'ArrowUp') sel = move(view, sel, 0, -1);
    else if (k === 'ArrowDown') sel = move(view, sel, 0, 1);
    else if (k === 'Escape') showBack();
    else if (k === 'Enter' && !tabbed) {
      if (sel.key === 'new-episode') newEpisode();
      else if (sel.key === 'new-show') newShow();
      else void open(e.altKey ? 'review' : 'edit');
    } else return false;
    return true;
  }

  // the keys walk past what fits in the window
  $effect(() => {
    const row = lists[sel.row];
    void sel.key;
    void tick().then(() => row?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest', inline: 'nearest' }));
  });

  onMount(() => {
    app.welcomeUi = { key, drop };
    void load();
  });
  onDestroy(() => {
    app.welcomeUi = null;
  });
</script>

{#snippet card(t: Item, row: 0 | 1)}
  {@const e = t.kind === 'episode' ? t.episode : null}
  {@const count = notesOf(t)}
  {@const gone = e ? e.missing : t.kind === 'show' && t.show.missing}
  {@const small = smallOf(t)}
  <button class="tile" class:on={sel.key === t.key} class:home={sel.key !== t.key && sel.scope === t.key} class:gone class:drop={fileDrag.over === `cover:${t.key}`} data-file-zone={t.kind === 'single' ? undefined : `cover:${t.key}`} role="option" aria-selected={sel.key === t.key} aria-label="{titleOf(t)}, {small}{count ? `, ${notesText(count)}` : ''}" tabindex="-1" onclick={(ev) => { pick(row, t); ev.currentTarget.blur(); }} oncontextmenu={(ev) => openMenu(ev, t)}>
    <span class="cov" style="background: {face(t, TILE_PX)}"></span>
    <span class="tshade"></span>
    {#if count}<span class="note-count cnt tn">{count}</span>{/if}
    <span class="tx"><span class="nm kick ellip">{small}</span><span class="ti ellip">{titleOf(t)}</span></span>
  </button>
{/snippet}

{#snippet plus(t: Tile, row: 0 | 1, text: string)}
  <button class="tile new" class:on={sel.key === t.key} role="option" aria-selected={sel.key === t.key} tabindex="-1" onclick={(ev) => { pick(row, t); ev.currentTarget.blur(); }}>
    <Icon name="plus" size={22} /><span>{text}</span>
  </button>
{/snippet}

<div class="shelf" data-testid="welcome">
  <section class="hero" class:lost class:drop={fileDrag.over === 'cover:hero'} data-file-zone={canEdit ? 'cover:hero' : undefined} aria-label="Selected" oncontextmenu={(e) => item && openMenu(e, item)}>
    {#key sel.item}
      <div class="cov" style="background: {item ? face(item, HERO_PX) : 'none'}" in:fade={{ duration: 320 }}></div>
    {/key}
    <div class="shade veil"></div>
    <div class="shade low"></div>
    <div class="shade high"></div>
    {#if item}
      <div class="copy">
        {#if heroKicker}<div class="nm hk" style="color: {heroTint}">{heroKicker}</div>{/if}
        <h1 class="ellip" title={titleOf(item)}>{titleOf(item)}</h1>
        <div class="meta">
          {#if item.kind === 'episode' && !lost && item.episode.file}
            <span class="dots" aria-hidden="true">{#each [0, 1, 2, 3, 4] as i (i)}<i class:done={i < item.episode.stage} class:now={i === item.episode.stage}></i>{/each}</span>
          {/if}
          <span class="ellip">{metaOf(item)}</span>
        </div>
        <div class="go">
          {#if lost}
            <button class="hbtn edit" onclick={() => locate(item)}>Locate… <span class="kb">↩</span></button>
          {:else}
            <button class="hbtn edit" disabled={!canEdit} onclick={() => open('edit')}>Edit <span class="kb">↩</span></button>
            <button class="hbtn review" disabled={!canReview} onclick={() => open('review')}>Review{#if notes}<span class="n tn">{notesText(notes)}</span>{/if}<span class="kb">⌥↩</span></button>
          {/if}
        </div>
      </div>
    {:else if view}
      <div class="copy">
        <h1>{brand.displayName}</h1>
        <div class="meta"><span>{brand.tagline}</span></div>
        <div class="go"><button class="hbtn plain" disabled={app.busy} onclick={() => app.demo()}>{app.busy ? 'Building…' : 'Demo session'}</button></div>
      </div>
    {/if}
  </section>

  <div class="brand"><TakeDeck /><span>{brand.displayName}</span></div>
  <div class="top">
    <button class="btn" onclick={() => newEpisode()}>New episode <span class="kb">⌘N</span></button>
    <button class="btn" onclick={newShow}>New show <span class="kb">⇧⌘N</span></button>
    <button class="btn" onclick={openFile}>Open… <span class="kb">⌘O</span></button>
  </div>

  {#if both}
    <div class="rows">
      <div class="rowhead">
        {#if title}
          <button class="backto" onclick={showBack}><Icon name="chevron-left" size={12} stroke={2.4} />Recent episodes</button>
          <h2 data-testid="row-title">{title}</h2>
        {:else}
          <h2 data-testid="row-title">Recent episodes</h2>
        {/if}
      </div>
      <div class="row" role="listbox" aria-label={title ?? 'Recent episodes'} aria-orientation="horizontal" tabindex="-1" bind:this={lists[0]}>
        {#each both[0] as t (t.key)}
          {#if t.kind === 'new-episode'}{@render plus(t, 0, 'New episode')}{:else if t.kind !== 'new-show'}{@render card(t, 0)}{/if}
        {/each}
      </div>
      <div class="rowhead"><h2>Shows</h2></div>
      <div class="row" role="listbox" aria-label="Shows" aria-orientation="horizontal" tabindex="-1" bind:this={lists[1]}>
        {#each both[1] as t (t.key)}
          {#if t.kind === 'new-show'}{@render plus(t, 1, 'New show')}{:else if t.kind !== 'new-episode'}{@render card(t, 1)}{/if}
        {/each}
      </div>
    </div>
  {/if}

  <div class="foot">← → ↑ ↓ move · ↩ Edit · ⌥↩ Review · drop files to start</div>

  {#if dropping === 'episode' || dropping === 'review'}
    <div class="dropov" aria-hidden="true">
      <div class="dropbox">
        <TakeDeck size={70} />
        <div class="dt">{dropping === 'review' ? 'Drop to add the review' : 'Drop to start an episode'}</div>
        {#if dropping === 'episode'}<div class="dm">Script or recordings</div>{/if}
      </div>
    </div>
  {/if}

  {#if menu}<ShelfMenu x={menu.x} y={menu.y} items={menuItems} onclose={() => (menu = null)} />{/if}
  {#if ask && view}<NewSheet bind:this={sheet} {ask} shows={view.shows.filter((s) => !s.missing)} folder={view.folder} oncreated={created} onclose={closeSheet} />{/if}
</div>

<style>
  .shelf {
    --side: clamp(24px, 4.4vw, 64px);
    --tile-h: clamp(84px, 13.8vh, 124px);
    position: absolute;
    inset: 0;
    z-index: 5;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: var(--stage-bg);
    color: var(--text-1);
  }
  .hero {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  .hero.drop::after {
    content: '';
    position: absolute;
    inset: 10px;
    border-radius: var(--radius-panel);
    box-shadow: inset 0 0 0 2px var(--accent);
    pointer-events: none;
  }
  .cov {
    position: absolute;
    inset: 0;
  }
  .hero.lost .cov {
    opacity: 0.35;
  }
  .shade {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  /* always between the image and the words, in either theme */
  .veil {
    background: linear-gradient(90deg, rgb(var(--night-rgb) / 0.92) 0%, rgb(var(--night-rgb) / 0.55) 45%, rgb(var(--night-rgb) / 0.1) 100%);
  }
  .low {
    background: linear-gradient(180deg, transparent 55%, var(--stage-bg) 100%);
  }
  /* the brand and the buttons sit on the image too */
  .high {
    background: linear-gradient(180deg, rgb(var(--night-rgb) / 0.72) 0, transparent 130px);
  }
  .copy {
    position: absolute;
    left: var(--side);
    bottom: clamp(14px, 4vh, 44px);
    width: min(620px, calc(100% - 2 * var(--side)));
    color: var(--text-strong);
  }
  .hk {
    font-size: 11px;
  }
  h1 {
    margin: 8px 0 0;
    font-size: clamp(34px, 6.2vh, 56px);
    font-weight: 800;
    letter-spacing: -0.035em;
    line-height: 1.05;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 18px;
    margin-top: 12px;
    font-size: 13.5px;
    color: var(--text-3);
  }
  .dots {
    display: flex;
    flex: none;
    gap: 3px;
  }
  .dots i {
    display: block;
    width: 12px;
    height: 3px;
    border-radius: 2px;
    background: rgb(var(--ink-rgb) / 0.18);
  }
  .dots i.done {
    background: rgb(var(--ink-rgb) / 0.55);
  }
  .dots i.now {
    background: var(--text-strong);
  }
  .go {
    display: flex;
    gap: 10px;
    margin-top: clamp(14px, 2.9vh, 26px);
  }
  .hbtn {
    height: 44px;
    padding: 0 18px;
    border-radius: var(--radius-btn);
    display: inline-flex;
    align-items: center;
    gap: 9px;
    font-size: 14.5px;
    font-weight: 700;
    transition: filter 0.15s;
  }
  .hbtn:hover:not(:disabled) {
    filter: brightness(1.08);
  }
  .hbtn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .hbtn .kb {
    background: rgb(var(--shadow-rgb) / 0.14);
    color: inherit;
  }
  .edit {
    background: var(--button-light);
    color: var(--text-on-light);
  }
  .review {
    background: var(--accent);
    color: var(--on-accent);
  }
  .plain {
    background: var(--glass-active);
    color: var(--text-strong);
  }
  .n {
    height: 20px;
    padding: 0 7px;
    border-radius: var(--radius-pill);
    display: inline-flex;
    align-items: center;
    font-size: 11.5px;
    background: rgb(var(--shadow-rgb) / 0.18);
  }
  .brand {
    position: absolute;
    left: var(--side);
    top: 30px;
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 19px;
    font-weight: 800;
    letter-spacing: -0.02em;
    color: var(--text-strong);
  }
  .top {
    position: absolute;
    right: var(--side);
    top: 30px;
    display: flex;
    gap: 8px;
  }
  .top .btn {
    color: var(--text-strong);
    background: rgb(var(--night-rgb) / 0.5);
    box-shadow: inset 0 0 0 1px var(--glass-outline);
    backdrop-filter: var(--glass-blur);
    -webkit-backdrop-filter: var(--glass-blur);
  }
  .top .btn:hover {
    background: rgb(var(--night-rgb) / 0.68);
  }
  .top .kb {
    margin-left: 6px;
  }
  .rows {
    flex: none;
  }
  .rowhead {
    display: flex;
    align-items: baseline;
    gap: 12px;
    margin: 4px var(--side) 0;
  }
  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
  }
  .backto {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-4);
  }
  .backto:hover {
    color: var(--text-1);
  }
  .row {
    display: flex;
    gap: 16px;
    /* room for the lifted tile and its ring, which the scroller would otherwise clip */
    padding: 12px var(--side) 26px;
    margin-bottom: -10px;
    overflow-x: auto;
    scroll-padding-inline: var(--side);
    scrollbar-width: none;
    outline: none;
  }
  .row::-webkit-scrollbar {
    display: none;
  }
  .tile {
    position: relative;
    flex: none;
    width: calc(var(--tile-h) * 1.61);
    height: var(--tile-h);
    border-radius: var(--radius-snip);
    overflow: hidden;
    text-align: left;
    color: var(--text-strong);
    box-shadow: inset 0 0 0 1px var(--glass-outline);
    transition:
      transform 0.3s cubic-bezier(0.25, 0.8, 0.25, 1),
      box-shadow 0.25s ease;
  }
  .tile:hover {
    box-shadow: inset 0 0 0 1.5px var(--text-5);
  }
  .tile.home {
    box-shadow: inset 0 0 0 1.5px var(--text-4);
  }
  .tile.on {
    transform: translateY(-4px);
    box-shadow:
      0 0 0 2px var(--text-strong),
      0 8px 16px rgb(var(--shadow-rgb) / 0.35);
  }
  .tile.drop {
    box-shadow: 0 0 0 2px var(--accent);
  }
  .tile .cov {
    border-radius: inherit;
  }
  .tile.gone .cov,
  .tile.gone .tx {
    opacity: 0.45;
  }
  .tshade {
    position: absolute;
    inset: 0;
    background: linear-gradient(180deg, transparent 18%, rgb(var(--night-rgb) / 0.66) 58%, rgb(var(--night-rgb) / 0.94) 100%);
  }
  .cnt {
    position: absolute;
    right: 10px;
    top: 10px;
  }
  .tx {
    position: absolute;
    left: 14px;
    right: 12px;
    bottom: 11px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .kick {
    font-size: 9px;
    color: var(--text-3);
  }
  .ti {
    font-size: 15px;
    font-weight: 700;
  }
  .new {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-3);
    box-shadow: inset 0 0 0 1.5px var(--glass-outline);
  }
  .foot {
    flex: none;
    padding: 0 var(--side) 14px;
    font-size: 12px;
    color: var(--text-5);
  }
  .dropov {
    position: absolute;
    inset: 0;
    z-index: 20;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgb(var(--night-rgb) / 0.6);
    backdrop-filter: var(--glass-blur-soft);
    -webkit-backdrop-filter: var(--glass-blur-soft);
    pointer-events: none;
  }
  .dropbox {
    width: min(520px, 80%);
    height: 260px;
    border-radius: var(--radius-box);
    border: 2px dashed rgb(var(--accent-rgb) / 0.8);
    background: rgb(var(--accent-rgb) / 0.08);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
  }
  .dt {
    font-size: 20px;
    font-weight: 700;
  }
  .dm {
    font-size: 13px;
    color: var(--text-3);
  }
</style>
