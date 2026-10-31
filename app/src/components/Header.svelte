<script lang="ts">
  import { app, type Page } from '../lib/app.svelte';
  import { brand } from '../lib/brand';

  const pages: { id: Page; label: string; key: string }[] = [
    { id: 'options', label: 'Options', key: '1' },
    { id: 'arrange', label: 'Arrange', key: '2' },
    { id: 'edit', label: 'Edit', key: '3' },
  ];
  const sc = $derived(app.view?.scene);
  const saving = $derived(app.busy);
</script>

<header class="hdr">
  <div class="title">
    <span class="ep" title={brand.displayName}>{app.view?.episode ?? app.status?.name ?? brand.displayName}</span>
    {#if sc}<span class="muted2">Sc. {sc.number} · {sc.place}{sc.time_of_day ? ` · ${sc.time_of_day}` : ''}</span>{/if}
    <span class="dot" class:saving title={saving ? 'Saving…' : 'Saved'}></span>
  </div>
  <div class="glass switch" role="group" aria-label="View">
    {#each pages as p (p.id)}
      <button class:active={app.page === p.id} onclick={() => app.setPage(p.id)}>{p.label} <span class="kb">{p.key}</span></button>
    {/each}
  </div>
  <div class="right">
    <label class="size">Size<input type="range" min="0.8" max="1.3" step="0.05" bind:value={app.scale} aria-label="Box size" /></label>
    <button class="icb round" onclick={() => (app.overlay = 'settings')} aria-label="Settings" title="Settings">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" /></svg>
    </button>
    <button class="icb round" onclick={() => (app.overlay = 'help')} aria-label="Keyboard shortcuts">?</button>
    <button class="pillbtn" onclick={() => (app.overlay = 'history')}>History</button>
    <button class="pillbtn" onclick={() => (app.overlay = 'export')}>Export</button>
  </div>
</header>

<style>
  .hdr {
    position: absolute;
    top: 20px;
    left: 96px;
    right: 24px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
    z-index: 10;
  }
  .title {
    display: flex;
    align-items: baseline;
    gap: 10px;
    font-size: var(--fs-ui);
  }
  .ep {
    font-weight: 600;
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
  .pillbtn {
    height: 40px;
    padding: 0 18px;
    border-radius: var(--radius-pill);
    font-size: var(--fs-ui);
    font-weight: 600;
    background: var(--glass-fill-4);
    box-shadow: inset 0 0 0 1px var(--glass-outline);
  }
</style>
