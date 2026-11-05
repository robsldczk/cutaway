<script lang="ts">
  import { app, type Page } from '../lib/app.svelte';
  import { brand } from '../lib/brand';
  import Icon from './Icon.svelte';

  const base: { id: Page; label: string; key: string }[] = [
    { id: 'import', label: 'Import', key: '' },
    { id: 'options', label: 'Options', key: '1' },
    { id: 'arrange', label: 'Arrange', key: '2' },
    { id: 'edit', label: 'Edit', key: '3' },
    { id: 'export', label: 'Export', key: '4' },
  ];
  const pages = $derived(app.dev ? [...base, { id: 'dev' as Page, label: 'Dev', key: '' }] : base);
  const sc = $derived(app.view?.scene);
  const saving = $derived(app.busy);
  /** Import and Edit have no scene rail: the header starts at the edge (Round 2). */
  const wide = $derived(app.page === 'import' || app.page === 'edit' || app.page === 'export' || app.page === 'dev');
  const ready = $derived(!!app.view);
  let menu = $state(false);
  /** The show the open episode lives in (studio mode). */
  const show = $derived(app.status?.show ?? null);
</script>

<header class="hdr" class:wide class:fit={app.page === 'arrange'} data-keeps-sound>
  <div class="title">
    <div class="epm">
      <button class="ep" aria-haspopup="menu" aria-expanded={menu} title={app.status?.path ?? brand.displayName} onclick={() => (menu = !menu)}>{#if show}<span class="showname">{show.name}</span>{/if}{app.view?.episode ?? app.status?.name ?? brand.displayName}<Icon name="chevron" size={10} stroke={2.4} /></button>
      {#if menu}
        <div class="scrim0" role="presentation" onclick={() => (menu = false)}></div>
        <div class="glass pop" role="menu" aria-label="Episode">
          <div class="muted ellip pth">{app.status?.path ?? ''}</div>
          {#if show && app.show}
            <div class="muted sec">{show.name}</div>
            {#each app.show.episodes as e (e.folder)}
              <button role="menuitemradio" aria-checked={e.folder === show.episode} class:cur={e.folder === show.episode} onclick={() => { menu = false; void app.openEpisode(e.folder); }}>{e.label}{#if !e.file}<span class="muted"> · new</span>{/if}</button>
            {/each}
            <div class="sep"></div>
            <button role="menuitem" onclick={() => { menu = false; void app.showHome(); }}>Show home</button>
          {/if}
          <button role="menuitem" onclick={() => { menu = false; app.overlay = 'history'; }}>Versions and history…</button>
          <button role="menuitem" onclick={() => { menu = false; void (show ? app.closeShow() : app.closeProject()); }}>Back to start screen</button>
        </div>
      {/if}
    </div>
    {#if sc && app.page === 'edit'}
      <label class="scene muted2">
        <select aria-label="Scene" value={sc.id} onchange={(e) => app.loadScene((e.currentTarget as HTMLSelectElement).value)}>
          {#each app.view?.scenes ?? [] as s (s.id)}<option value={s.id}>Sc. {s.number} · {s.place}</option>{/each}
        </select>
        Sc. {sc.number} · {sc.place}<Icon name="chevron" size={10} stroke={2.4} />
      </label>
      <button class="icb lock" aria-label={app.sceneLocked ? `Unlock scene ${sc.number}` : `Lock scene ${sc.number}`} aria-pressed={!!app.sceneLocked} title={app.sceneLocked ? 'Unlock this scene' : "Lock this scene's timing"} onclick={() => (app.sceneLocked ? (app.unlocking = sc.id) : app.lockScene(sc.id))} data-testid="header-lock"><Icon name={app.sceneLocked ? 'lock' : 'unlock'} size={13} /></button>
    {:else if sc && app.page !== 'import' && app.page !== 'dev' && app.page !== 'export'}<span class="muted2 where" title={sc.heading}>Sc. {sc.number} · {sc.place}{sc.time_of_day ? ` · ${sc.time_of_day}` : ''}</span>{/if}
    <span class="dot" class:saving title={saving ? 'Saving…' : 'Saved'}></span>
  </div>
  <nav class="glass switch" aria-label="Pages">
    {#each pages as p (p.id)}
      <button class:active={app.page === p.id} disabled={p.id !== 'import' && !ready} onclick={() => app.setPage(p.id)}>{p.label}{#if p.key}<span class="kb">{p.key}</span>{/if}</button>
    {/each}
  </nav>
  <div class="right">
    {#if app.page === 'import'}
      <button class="pillbtn" onclick={() => app.toggleFullBucket()} disabled={!ready}>{#if app.importTab !== 'files'}<Icon name="back" size={13} />Back to {app.importBack === 'arrange' ? 'Arrange' : 'Edit'}{:else}Edit{/if} <span class="kb">⇧B</span></button>
    {/if}
    {#if app.page === 'arrange'}
      <button class="pillbtn sounds" class:on={app.soundsOn} aria-pressed={app.soundsOn} onclick={() => app.toggleSounds()} title="Sounds beside the dialogue" data-testid="sounds-toggle">
        <svg width="22" height="14" viewBox="0 0 22 14" aria-hidden="true"><circle cx="4" cy="9" r="3.5" fill="rgb(var(--bucket-1-rgb))" /><circle cx="11" cy="5" r="4" fill="rgb(var(--bucket-2-rgb))" /><circle cx="18" cy="9" r="3" fill="rgb(var(--bucket-3-rgb))" /></svg>
        Sounds <span class="kb">S</span>
      </button>
    {/if}
    {#if app.page === 'arrange' && app.soundsOn}
      <button class="pillbtn follow" class:on={app.soundSnap} aria-pressed={app.soundSnap} onclick={() => app.setSoundSnap(!app.soundSnap)} title={app.soundSnap ? 'Sounds catch on lines, pauses and directions. Hold ⌥ while dragging to let go for one move, or click to turn it off.' : 'Sounds go exactly where you put them. Click to catch on lines, pauses and directions.'} data-testid="snap-toggle">Snap</button>
    {/if}
    {#if app.page === 'options' || app.page === 'arrange' || app.page === 'edit'}
      <button class="pillbtn follow" class:on={app.follow} aria-pressed={app.follow} onclick={() => app.setFollow(!app.follow)} title={app.follow ? 'The current box moves with the playhead. Click to keep it where you put it.' : 'The current box stays where you put it. Click to move it with the playhead.'} data-testid="follow">Follow</button>
    {/if}
    {#if app.page === 'options' || app.page === 'arrange'}
      <label class="size">Size<input type="range" min="0.8" max="1.3" step="0.05" bind:value={app.scale} aria-label="Box size" /></label>
    {/if}
    <button class="icb round" onclick={() => (app.overlay = 'settings')} aria-label="Settings" title="Settings">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" /></svg>
    </button>
    <button class="icb round" onclick={() => (app.overlay = 'help')} aria-label="Keyboard shortcuts">?</button>
    <button class="pillbtn" onclick={() => (app.overlay = 'history')}>History</button>
  </div>
</header>

<style>
  .hdr {
    position: absolute;
    top: 16px;
    left: 96px;
    right: 24px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
    z-index: 10;
  }
  .hdr.wide {
    left: 28px;
    right: 28px;
  }
  /* Arrange has the most buttons: the scene's name gives way, not the row */
  @media (min-width: 1180px) {
    .hdr.fit {
      flex-wrap: nowrap;
    }
    .hdr.fit .title {
      flex: 1 1 0;
      min-width: 0;
    }
    .hdr.fit .switch,
    .hdr.fit .right {
      flex: none;
    }
    .hdr.fit .ep {
      white-space: nowrap;
    }
    .hdr.fit .where {
      min-width: 0;
      overflow: hidden;
      white-space: nowrap;
      text-overflow: ellipsis;
    }
  }
  .scene {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    cursor: pointer;
  }
  .scene select {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }
  .switch button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .title {
    display: flex;
    align-items: baseline;
    gap: 10px;
    font-size: var(--fs-ui);
  }
  .epm {
    position: relative;
  }
  .ep {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-weight: 600;
    padding: 4px 8px;
    margin: -4px -8px;
    border-radius: 8px;
  }
  .ep:hover {
    background: var(--glass-hover);
  }
  .scrim0 {
    position: fixed;
    inset: 0;
    z-index: 19;
  }
  .pop {
    position: absolute;
    top: 30px;
    left: -8px;
    z-index: 20;
    min-width: 240px;
    max-width: 380px;
    padding: 6px;
    border-radius: 14px;
    background: var(--panel-solid);
    display: flex;
    flex-direction: column;
  }
  .pop button {
    text-align: left;
    padding: 8px 10px;
    border-radius: 9px;
    font-size: 13px;
  }
  .pop button:hover {
    background: var(--glass-hover);
  }
  .pth {
    font-size: 11px;
    padding: 6px 10px 4px;
  }
  .sec {
    font-size: 10.5px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    padding: 8px 10px 2px;
  }
  .pop button.cur {
    background: var(--glass-active);
    font-weight: 600;
  }
  .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--glass-outline);
  }
  .showname {
    color: var(--text-3);
    font-weight: 600;
    margin-right: 8px;
  }
  .muted2 {
    color: var(--text-3);
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-5);
    align-self: center;
  }
  .dot.saving {
    background: var(--warn);
  }
  .switch {
    display: flex;
    padding: 4px;
    border-radius: var(--radius-pill);
    background: var(--glass-fill-2);
  }
  .switch button {
    height: 36px;
    padding: 0 18px;
    border-radius: var(--radius-pill);
    font-size: var(--fs-ui);
    color: var(--text-3);
    font-weight: 500;
  }
  .switch button.active {
    background: var(--button-light-active);
    color: var(--text-on-light);
    font-weight: 600;
  }
  .switch .kb {
    margin-left: 4px;
    background: rgba(127, 127, 127, 0.18);
    color: inherit;
  }
  .sounds {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .right {
    display: flex;
    gap: 12px;
    align-items: center;
  }
  .size {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text-3);
  }
  .size input {
    width: 110px;
    accent-color: var(--text-strong);
  }
  .round {
    width: 40px;
    height: 40px;
    font-size: 15px;
    font-weight: 700;
    background: var(--glass-fill-3);
  }
  .pillbtn:disabled {
    opacity: 0.4;
  }
  .follow::before {
    content: '';
    width: 7px;
    height: 7px;
    border-radius: 50%;
    box-shadow: inset 0 0 0 1.5px currentColor;
  }
  .follow.on::before {
    background: var(--playhead);
    box-shadow: 0 0 6px var(--playhead-glow);
  }
  .follow:not(.on) {
    color: var(--text-3);
  }
  .pillbtn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 40px;
    padding: 0 18px;
    border-radius: var(--radius-pill);
    font-size: var(--fs-ui);
    font-weight: 600;
    background: var(--glass-fill-4);
    box-shadow: inset 0 0 0 1px var(--glass-outline);
  }
</style>
