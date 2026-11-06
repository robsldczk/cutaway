<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from './lib/app.svelte';
  import { call, inTauri, pickPath } from './lib/bridge';
  import { brand } from './lib/brand';
  import { fileEnd, fileEnter, fileOver } from './lib/filedrop.svelte';
  import { onKey } from './lib/keys';
  import { flushSpot } from './lib/resume';
  import Header from './components/Header.svelte';
  import SceneRail from './components/SceneRail.svelte';
  import Transport from './components/Transport.svelte';
  import HelpOverlay from './components/HelpOverlay.svelte';
  import BoxEditor from './components/BoxEditor.svelte';
  import Inspector from './components/Inspector.svelte';
  import Unmatched from './components/Unmatched.svelte';
  import Settings from './components/Settings.svelte';
  import History from './components/History.svelte';
  import LockBanner from './components/LockBanner.svelte';
  import UnlockSheet from './components/UnlockSheet.svelte';
  import Welcome from './components/Welcome.svelte';
  import ShowHome from './components/ShowHome.svelte';
  import Bucket from './components/Bucket.svelte';
  import DragGhost from './components/DragGhost.svelte';
  import FileSheet from './components/FileSheet.svelte';
  import Options from './pages/Options.svelte';
  import Arrange from './pages/Arrange.svelte';
  import Edit from './pages/Edit.svelte';
  import Import from './pages/Import.svelte';
  import Export from './pages/Export.svelte';
  import Dev from './pages/Dev.svelte';
  import Review from './pages/Review.svelte';
  import ReviewHistory from './components/ReviewHistory.svelte';
  import ReviewerSheet from './components/ReviewerSheet.svelte';

  let root: HTMLDivElement | undefined = $state();
  let innerWidth = $state(1440);
  $effect(() => {
    app.narrow = innerWidth <= 1100;
  });
  let wAcc = 0;
  let wLast = 0;

  function onWheel(e: WheelEvent) {
    if (app.overlay || app.page === 'edit' || app.page === 'import' || app.page === 'export' || app.page === 'dev') return;
    const t = e.target as HTMLElement;
    if (t.closest('.rail') || t.closest('[role="listbox"], [role="menu"]')) return; // these scroll themselves
    // the opened sound's handles are sliders for the keyboard; the wheel still scrubs there
    if (t.closest('[role="slider"]:not([data-part])') || t.closest('aside') || t.tagName === 'INPUT') return;
    e.preventDefault();
    if (app.page === 'arrange' && app.soundsUi?.wheel(e)) return;
    if (app.page === 'review' && app.reviewUi?.wheel(e)) return;
    const now = performance.now();
    // macOS turns a Shift wheel into a horizontal one
    let d = e.shiftKey && Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
    if (e.deltaMode === 1) d *= 40;
    if ((wAcc > 0) !== (d > 0)) wAcc = 0;
    wAcc += d;
    if (e.shiftKey) {
      // fast: a line per 30 px with no rate limit; the column's slide glides through them
      const n = Math.trunc(wAcc / 30);
      if (n) {
        app.go(n);
        wAcc -= n * 30;
      }
      return;
    }
    if (Math.abs(wAcc) >= 40 && now - wLast > 120) {
      app.go(wAcc > 0 ? 1 : -1);
      wAcc = 0;
      wLast = now;
    }
  }

  const AUDIO = /\.(wav|bwf|aif|aiff|mp3|flac|m4a)$/i;

  /** Inside a show (its home or one of its episodes) the stage glows in the show's colours. */
  const tint = $derived.by(() => {
    const c = app.status?.show?.colors ?? (app.noProject ? app.show?.theme.colors : undefined) ?? [];
    if (!c.length) return '';
    const rgba = (h: string, a: number) => {
      const n = parseInt(h.slice(1), 16);
      return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${a})`;
    };
    return `--stage-glow-1: ${rgba(c[0], 0.34)}; --stage-glow-2: ${rgba(c[1] ?? c[0], 0.12)}; --stage-glow-3: ${rgba(c[2] ?? c[0], 0.1)};`;
  });

  /** The native menu (File › Open Show… and friends). */
  async function menu(id: string) {
    if (id === 'open_show' || id === 'new_show') {
      const r = await pickPath({ directory: true, title: id === 'new_show' ? 'The production folder to make a show' : 'A show folder' });
      if (r && r.length) await app.openShow(r[0], id === 'new_show');
    } else if (id === 'open_episode') {
      const r = await pickPath({ title: 'Open an episode', filters: [{ name: `${brand.displayName} episode`, extensions: [brand.episodeExt] }] });
      if (r && r.length) {
        if (!app.noProject) await app.closeProject();
        await app.run(() => call('open_project', { path: r[0] }));
        await app.boot();
      }
    } else if (id === 'open_review') {
      const r = await pickPath({ title: 'Open a project package for review', filters: [{ name: `${brand.displayName} episode or show`, extensions: [brand.episodeExt, brand.showExt] }] });
      if (r && r.length) await app.openForReview(r[0]);
    } else if (id === 'show_home' && app.status?.show) {
      await app.showHome();
    } else if (id === 'close') {
      if (app.show) await app.closeShow();
      else if (!app.noProject) await app.closeProject();
    }
  }

  /** `x`, `y`: on macOS Tauri's "physical" drop position is really in points, so CSS pixels. */
  async function dropped(paths: string[], x: number, y: number) {
    const reviews = paths.filter((p) => p.endsWith(`.${brand.reviewExt}`));
    for (const r of reviews) await addReview(r);
    paths = paths.filter((p) => !reviews.includes(p));
    const scripts = paths.filter((p) => /\.(fountain|thrl|txt)$/i.test(p));
    const audio = paths.filter((p) => AUDIO.test(p));
    for (const s of scripts) await app.method('import_script', { path: s });
    if (!audio.length) return;
    if (app.page === 'import' && app.importTab === 'rooms') {
      // a room has one: the first is its ambience, any others wait among the choices
      for (const [i, path] of audio.entries()) await app.method('import_ambience', { path, ...(i === 0 ? app.roomPick : {}) }, false);
      await app.refresh();
      app.say('Ambience of this room');
    } else if (filingHere()) {
      app.filing = { paths: audio, x, y };
    } else if (app.drawer === 'bucket' || (app.page === 'import' && app.importTab === 'sounds')) {
      await app.method('import_sounds', { paths: audio });
      app.say(`${audio.length} sounds added to the library`);
    } else {
      // recordings and director room files: match them, then check the cast on Import › Files
      await app.method('import_audio', { paths: audio, copy: true }, false);
      await app.method('match_files');
      app.importTab = 'files';
      app.setPage('import');
    }
  }

  /** A review file on the window: the same as Add review. */
  async function addReview(path: string) {
    if (app.noProject) return app.say('Open the episode first, then add the review');
    const r = await app.run(() => call<{ who: string; notes: number; replies: number; notice: string | null }>('add_review', { path }));
    if (!r) return;
    await app.refresh();
    app.say(`${r.who}: ${r.notes} new ${r.notes === 1 ? 'note' : 'notes'}, ${r.replies} ${r.replies === 1 ? 'reply' : 'replies'}${r.notice ? `. ${r.notice}` : ''}`);
    app.overlay = 'review-history';
  }

  /** Pages where a sound from the Finder is filed into a bucket. */
  const filingHere = () => app.page === 'edit' || (app.page === 'arrange' && app.soundsOn) || (app.page === 'import' && app.importTab === 'sounds');

  // the spot is kept as it changes: a quit gives the page no last word
  $effect(() => app.noteSpot(app.spot));

  onMount(() => {
    void app.boot();
    window.addEventListener('pagehide', flushSpot);
    // the browser tests have no Finder: they call what Tauri's events would
    if (import.meta.env.DEV) {
      Object.assign(window, { cutawayFileDrag: { enter: fileEnter, over: fileOver, drop: (paths: string[], x: number, y: number) => { fileEnd(); return dropped(paths, x, y); } } });
    }
    window.addEventListener('keydown', onKey);
    const el = root;
    el?.addEventListener('wheel', onWheel, { passive: false });
    let unlisten: (() => void) | undefined;
    let unmenu: (() => void) | undefined;
    if (inTauri) {
      void import('@tauri-apps/api/event').then(async ({ listen }) => {
        unmenu = await listen<string>('menu', (e) => void menu(e.payload));
      });
      void import('@tauri-apps/api/webview').then(async ({ getCurrentWebview }) => {
        unlisten = await getCurrentWebview().onDragDropEvent((ev) => {
          const p = ev.payload;
          if (p.type === 'enter') fileEnter(p.paths);
          else if (p.type === 'over') fileOver(p.position.x, p.position.y);
          else if (p.type === 'leave') fileEnd();
          else if (p.type === 'drop') {
            fileEnd();
            void dropped(p.paths, p.position.x, p.position.y);
          }
        });
      });
    }
    return () => {
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('pagehide', flushSpot);
      el?.removeEventListener('wheel', onWheel);
      unlisten?.();
      unmenu?.();
      void call('stop').catch(() => {});
    };
  });
</script>

<svelte:window bind:innerWidth onpointerdowncapture={(e) => (app.pointer = { x: e.clientX, y: e.clientY, at: performance.now() })} />

<div class="ui stage" bind:this={root} style={tint}>
  <div class="paper" aria-hidden="true"></div>
  {#if app.page === 'options' || app.page === 'arrange'}<div class="hairline" aria-hidden="true"></div>{/if}

  {#if app.noProject && app.show}
    <ShowHome />
  {:else if app.noProject}
    <Welcome />
  {:else if app.page === 'review' && app.view}
    <Review />
    <SceneRail />
    <Transport />
  {:else if app.page === 'import' && app.status}
    <Import />
    <Header />
  {:else if app.page === 'dev' && app.status}
    <Dev />
    <Header />
  {:else if app.page === 'export' && app.status}
    <Export />
    <Header />
  {:else if app.view}
    {#if app.page === 'options'}<Options />{:else if app.page === 'arrange'}<Arrange />{:else}<Edit />{/if}
    <Header />
    {#if app.sceneLocked}<LockBanner />{/if}
    {#if app.page !== 'edit'}
      <SceneRail />
      <Transport />
    {/if}
  {:else if app.status}
    <Header />
  {/if}

  {#if app.drawer === 'inspector'}<Inspector />{/if}
  {#if app.drawer === 'unmatched'}<Unmatched />{/if}
  {#if app.drawer === 'bucket'}<Bucket />{/if}

  {#if app.overlay === 'editor' && app.cur}<BoxEditor />{/if}
  {#if app.overlay === 'help'}<HelpOverlay />{/if}
  {#if app.overlay === 'settings'}<Settings />{/if}
  {#if app.overlay === 'history'}<History />{/if}
  {#if app.overlay === 'review-history'}<ReviewHistory />{/if}
  {#if app.asking}<ReviewerSheet />{/if}
  {#if app.unlocking}<UnlockSheet />{/if}

  {#if app.toast}<div class="toast" role="status">{app.toast}</div>{/if}
  {#if app.blocked}
    <div class="blocked glass" role="alert" data-testid="blocked" style="left: {Math.min(app.blocked.x, innerWidth - 180)}px; top: {app.blocked.y + 14}px">{app.blocked.text}</div>
  {/if}
  <DragGhost />
  <FileSheet />
</div>

<style>
  .stage {
    position: relative;
    width: 100%;
    height: 100vh;
    overflow: hidden;
    outline: none;
    user-select: none;
    -webkit-user-select: none;
    color: var(--text-1);
    background:
      radial-gradient(ellipse 55% 45% at 50% 50%, var(--stage-glow-1), transparent 70%),
      radial-gradient(ellipse 35% 30% at 30% 78%, var(--stage-glow-2), transparent 70%),
      radial-gradient(ellipse 30% 30% at 75% 22%, var(--stage-glow-3), transparent 70%),
      var(--stage-bg);
  }
  .paper {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 50%;
    transform: translateX(-50%);
    width: min(720px, 94%);
    background: repeating-linear-gradient(0deg, var(--stage-paper-line) 0 1px, transparent 1px 7px);
    -webkit-mask-image: linear-gradient(90deg, transparent, #000 18%, #000 82%, transparent);
    mask-image: linear-gradient(90deg, transparent, #000 18%, #000 82%, transparent);
  }
  .hairline {
    position: absolute;
    left: 84px;
    right: 24px;
    top: 50%;
    height: 1px;
    background: linear-gradient(90deg, transparent, var(--stage-hairline) 10%, var(--stage-hairline) 90%, transparent);
  }
  /* bottom centre, above the transport: the top bars (Edit control bar, Sounds' Spot
     button) stay visible while it shows */
  .blocked {
    position: absolute;
    z-index: 60;
    transform: translateX(-50%);
    max-width: 320px;
    padding: 9px 14px;
    border-radius: var(--radius-btn);
    background: var(--panel-solid);
    font-size: 12.5px;
    line-height: 1.4;
    color: var(--text-1);
    pointer-events: none;
  }
  .toast {
    position: absolute;
    left: 50%;
    bottom: 146px;
    transform: translateX(-50%);
    z-index: 50;
    max-width: min(640px, 90%);
    pointer-events: none;
    padding: 8px 14px;
    border-radius: var(--radius-pill);
    background: var(--panel-solid);
    font-size: 12.5px;
    box-shadow: var(--glass-shadow);
  }
</style>
