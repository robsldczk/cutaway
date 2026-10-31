<script lang="ts">
  import { app, type Page } from '../lib/app.svelte';
  import { brand } from '../lib/brand';
  import Icon from './Icon.svelte';

  const pages: { id: Page; label: string; key: string }[] = [
    { id: 'import', label: 'Import', key: '' },
    { id: 'options', label: 'Options', key: '1' },
    { id: 'arrange', label: 'Arrange', key: '2' },
    { id: 'edit', label: 'Edit', key: '3' },
  ];
  const sc = $derived(app.view?.scene);
  const saving = $derived(app.busy);
  /** Import and Edit have no scene rail: the header starts at the edge (Round 2). */
  const wide = $derived(app.page === 'import' || app.page === 'edit');
  const ready = $derived(!!app.view);
  let menu = $state(false);
</script>

<header class="hdr" class:wide>
  <div class="title">
    <div class="epm">
      <button class="ep" aria-haspopup="menu" aria-expanded={menu} title={app.status?.path ?? brand.displayName} onclick={() => (menu = !menu)}>{app.view?.episode ?? app.status?.name ?? brand.displayName}<Icon name="chevron" size={10} stroke={2.4} /></button>
      {#if menu}
        <div class="scrim0" role="presentation" onclick={() => (menu = false)}></div>
        <div class="glass pop" role="menu" aria-label="Episode">
          <div class="muted ellip pth">{app.status?.path ?? ''}</div>
          <button role="menuitem" onclick={() => { menu = false; app.overlay = 'history'; }}>Versions and history…</button>
          <button role="menuitem" onclick={() => { menu = false; void app.closeProject(); }}>Back to start screen</button>
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
    {:else if sc && app.page !== 'import'}<span class="muted2">Sc. {sc.number} · {sc.place}{sc.time_of_day ? ` · ${sc.time_of_day}` : ''}</span>{/if}
    <span class="dot" class:saving title={saving ? 'Saving…' : 'Saved'}></span>
  </div>
  <nav class="glass switch" aria-label="Pages">
    {#each pages as p (p.id)}
      <button class:active={app.page === p.id} disabled={p.id !== 'import' && !ready} onclick={() => app.setPage(p.id)}>{p.label}{#if p.key}<span class="kb">{p.key}</span>{/if}</button>
    {/each}
  </nav>
  <div class="right">
    {#if app.page === 'import'}
      <button class="pillbtn" onclick={() => app.toggleFullBucket()} disabled={!ready}>{#if app.importTab === 'sounds'}<Icon name="back" size={13} />Back to Edit{:else}Edit{/if} <span class="kb">⇧B</span></button>
    {:else if app.page !== 'edit'}
      <label class="size">Size<input type="range" min="0.8" max="1.3" step="0.05" bind:value={app.scale} aria-label="Box size" /></label>
    {/if}
    <button class="icb round" onclick={() => (app.overlay = 'settings')} aria-label="Settings" title="Settings">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" /></svg>
    </button>
    <button class="icb round" onclick={() => (app.overlay = 'help')} aria-label="Keyboard shortcuts">?</button>
    <button class="pillbtn" onclick={() => (app.overlay = 'history')}>History</button>
    <button class="pillbtn" onclick={() => (app.overlay = 'export')} disabled={!ready}>Export</button>
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
