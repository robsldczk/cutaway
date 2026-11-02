<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from './lib/app.svelte';
  import { call, inTauri, pickPath } from './lib/bridge';
  import { brand } from './lib/brand';
  import { onKey } from './lib/keys';
  import Header from './components/Header.svelte';
  import SceneRail from './components/SceneRail.svelte';
  import Transport from './components/Transport.svelte';
  import HelpOverlay from './components/HelpOverlay.svelte';
  import BoxEditor from './components/BoxEditor.svelte';
  import Inspector from './components/Inspector.svelte';
  import Unmatched from './components/Unmatched.svelte';
  import Settings from './components/Settings.svelte';
  import History from './components/History.svelte';
  import ExportPanel from './components/ExportPanel.svelte';
  import Welcome from './components/Welcome.svelte';
  import ShowHome from './components/ShowHome.svelte';
  import Bucket from './components/Bucket.svelte';
  import Options from './pages/Options.svelte';
  import Arrange from './pages/Arrange.svelte';
  import Edit from './pages/Edit.svelte';
  import Import from './pages/Import.svelte';
  import Dev from './pages/Dev.svelte';

  let root: HTMLDivElement | undefined = $state();
  let wAcc = 0;
  let wLast = 0;

  function onWheel(e: WheelEvent) {
    if (app.overlay || app.page === 'edit' || app.page === 'import' || app.page === 'dev') return;
    const t = e.target as HTMLElement;
    if (t.closest('[role="slider"]') || t.closest('aside') || t.tagName === 'INPUT') return;
    e.preventDefault();
    const now = performance.now();
    if ((wAcc > 0) !== (e.deltaY > 0)) wAcc = 0;
    wAcc += e.deltaY;
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
    } else if (id === 'show_home' && app.status?.show) {
      await app.showHome();
    } else if (id === 'close') {
      if (app.show) await app.closeShow();
      else if (!app.noProject) await app.closeProject();
    }
  }

  async function dropped(paths: string[]) {
    const scripts = paths.filter((p) => /\.(fountain|thrl|txt)$/i.test(p));
    const audio = paths.filter((p) => AUDIO.test(p));
    for (const s of scripts) await app.method('import_script', { path: s });
    if (!audio.length) return;
    if (app.page === 'edit') {
      // onto the timeline: import into the library and place the first sound at the playhead
      await app.method('import_sounds', { paths: audio, scene: app.sceneId, at_s: +app.pos.toFixed(3) });
      await app.loadEdit();
      app.say(`${audio.length} sound${audio.length > 1 ? 's' : ''} imported, placed at the playhead`);
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

  onMount(() => {
    void app.boot();
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
          if (ev.payload.type === 'drop') void dropped(ev.payload.paths);
        });
      });
    }
    return () => {
      window.removeEventListener('keydown', onKey);
      el?.removeEventListener('wheel', onWheel);
      unlisten?.();
      unmenu?.();
      void call('stop').catch(() => {});
    };
  });
</script>

<div class="ui stage" bind:this={root} style={tint}>
  <div class="paper" aria-hidden="true"></div>
  {#if app.page === 'options' || app.page === 'arrange'}<div class="hairline" aria-hidden="true"></div>{/if}

  {#if app.noProject && app.show}
    <ShowHome />
  {:else if app.noProject}
    <Welcome />
  {:else if app.page === 'import' && app.status}
    <Import />
    <Header />
  {:else if app.page === 'dev' && app.status}
    <Dev />
    <Header />
  {:else if app.view}
    {#if app.page === 'options'}<Options />{:else if app.page === 'arrange'}<Arrange />{:else}<Edit />{/if}
    <Header />
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
  {#if app.overlay === 'export'}<ExportPanel />{/if}

  {#if app.toast}<div class="toast" role="status">{app.toast}</div>{/if}
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
  .toast {
    position: absolute;
    left: 50%;
    bottom: 120px;
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
