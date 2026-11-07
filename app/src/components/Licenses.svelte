<script lang="ts">
  // Settings > Licenses (D161): the app, its copyright, and every work by others it ships, each
  // with its license text. The list is 600 KB, so it's read only when the tab opens.
  import { onMount } from 'svelte';
  import Icon from './Icon.svelte';
  import { LEGAL, matching, type LicensedPackage, type LicenseList } from '../lib/legal';

  let list = $state<LicenseList | 'failed' | null>(null);
  let query = $state('');
  let open = $state<string | null>(null);

  onMount(() => {
    let live = true;
    import('../legal/third-party-licenses.json?raw')
      .then((m) => live && (list = JSON.parse(m.default) as LicenseList))
      .catch(() => live && (list = 'failed'));
    return () => {
      live = false;
    };
  });

  const shown = $derived(list && list !== 'failed' ? matching(list.packages, query) : []);
  const keyOf = (p: LicensedPackage) => `${p.kind}:${p.name}@${p.version}`;
  const KIND = { other: 'Model or library', npm: 'Interface', rust: 'Core' } as const;
</script>

<div class="lic" data-testid="licenses">
  <div class="hero">
    <p class="full">{LEGAL.fullName}</p>
    <p>{LEGAL.copyright}</p>
    <p>{LEGAL.proprietary}</p>
  </div>
  <label class="search">
    <Icon name="search" size={13} />
    <input class="field" type="search" placeholder="Search {list && list !== 'failed' ? list.packages.length : ''} packages…" aria-label="Search the licenses" bind:value={query} data-testid="license-search" />
  </label>
  {#if list === 'failed'}
    <p class="err" role="alert">The license list could not be loaded.</p>
  {:else if list}
    <ul class="pkgs" data-testid="license-list">
      {#each shown as p (keyOf(p))}
        {@const k = keyOf(p)}
        <li>
          <button class="pk" aria-expanded={open === k} onclick={() => (open = open === k ? null : k)} data-testid="license-row">
            <span class="chev" class:down={open === k}><Icon name="chevron-right" size={11} stroke={2.4} /></span>
            <span class="pname ellip">{p.name}</span>
            <span class="vr tn">{p.version}</span>
            <span class="li">{p.license}</span>
          </button>
          {#if open === k}
            <div class="more">
              <div class="meta">
                <span>{KIND[p.kind]}</span>
                {#if p.authors}<span>{p.authors}</span>{/if}
                {#if p.repository}<span class="url">{p.repository}</span>{/if}
                {#if p.declared !== p.license}<span>Offered as {p.declared}</span>{/if}
                {#if p.note}<span>{p.note}</span>{/if}
              </div>
              {#each p.texts as t (t)}<pre>{list.texts[t]}</pre>{/each}
            </div>
          {/if}
        </li>
      {:else}
        <li class="none">No matches.</li>
      {/each}
    </ul>
  {:else}
    <p class="none">Loading…</p>
  {/if}
</div>

<style>
  .lic {
    display: flex;
    flex-direction: column;
    gap: 12px;
    height: 100%;
    min-height: 0;
  }
  .hero {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 14px;
    border-radius: 14px;
    background: linear-gradient(115deg, rgb(var(--accent-rgb) / 0.2) 0%, rgb(var(--accent-rgb) / 0.06) 45%, var(--glass-fill-2) 100%);
    box-shadow:
      inset 0 1px 0 rgb(var(--highlight-rgb) / 0.12),
      inset 0 0 0 1px var(--glass-outline);
    user-select: text;
    color: var(--text-3);
  }
  .hero p {
    margin: 0;
  }
  .hero .full {
    font-size: 15px;
    font-weight: 700;
    color: var(--text-strong);
  }
  .search {
    position: relative;
    display: flex;
    align-items: center;
    color: var(--text-5);
  }
  .search :global(svg) {
    position: absolute;
    left: 11px;
    pointer-events: none;
  }
  .search .field {
    width: 100%;
    padding-left: 30px;
    border-radius: var(--radius-pill);
  }
  .pkgs {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .pkgs li {
    position: relative;
  }
  .pkgs li::after {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 1px;
    background: linear-gradient(90deg, transparent, var(--glass-outline) 10%, var(--glass-outline) 90%, transparent);
  }
  .pkgs li:last-child::after {
    display: none;
  }
  .pk {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    height: 30px;
    padding: 0 6px;
    border-radius: 8px;
    text-align: left;
    color: var(--text-1);
  }
  .pk:hover {
    background: var(--glass-hover);
  }
  .chev {
    display: inline-flex;
    color: var(--text-5);
    transition: transform 0.15s var(--ease-out);
  }
  .chev.down {
    transform: rotate(90deg);
  }
  .pname {
    font-weight: 500;
    min-width: 0;
  }
  .vr {
    flex: none;
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 11px;
    color: var(--text-5);
  }
  .li {
    flex: none;
    margin-left: auto;
    padding-left: 8px;
    font-size: 11.5px;
    color: var(--text-3);
  }
  .more {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 2px 6px 12px 24px;
    user-select: text;
  }
  .meta {
    display: flex;
    flex-direction: column;
    font-size: 11.5px;
    color: var(--text-4);
  }
  .url {
    word-break: break-all;
  }
  pre {
    margin: 0;
    max-height: 260px;
    overflow: auto;
    padding: 10px 12px;
    border-radius: 10px;
    background: rgb(var(--night-rgb) / 0.28);
    box-shadow: inset 0 0 0 1px var(--glass-outline);
    font-family: ui-monospace, monospace;
    font-size: 11px;
    line-height: 1.5;
    white-space: pre-wrap;
    color: var(--text-3);
  }
  .none,
  .err {
    padding: 8px 6px;
    color: var(--text-5);
  }
  .err {
    color: var(--danger);
  }
</style>
