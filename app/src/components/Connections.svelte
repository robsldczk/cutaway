<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';

  interface Conn { service: string; label: string; connected: boolean; source: string | null }
  let conns = $state<Conn[]>([]);
  let keys = $state<Record<string, string>>({});
  let busy = $state<string | null>(null);

  async function load() {
    conns = (await app.run(() => call<Conn[]>('connections'))) ?? [];
  }
  onMount(load);

  async function connect(c: Conn) {
    const key = (keys[c.service] ?? '').trim();
    if (!key) return;
    busy = c.service;
    const r = await app.run(() => call<Conn>('connect_service', { service: c.service, api_key: key }));
    keys[c.service] = '';
    busy = null;
    if (r) app.say(`${c.label} connected`);
    await load();
  }

  async function disconnect(c: Conn) {
    await app.run(() => call('disconnect_service', { service: c.service }));
    await load();
  }
</script>

<div class="conns">
  {#each conns as c (c.service)}
    <div class="row">
      <span class="name">{c.label}</span>
      {#if c.connected}
        <span class="ok">Connected{c.source === 'environment' ? ' (environment variable)' : ''}</span>
        {#if c.source === 'keychain'}<button class="btn" onclick={() => disconnect(c)}>Disconnect</button>{/if}
      {:else}
        <input class="field" type="password" autocomplete="off" spellcheck="false" placeholder="API key" bind:value={keys[c.service]} onkeydown={(e) => e.key === 'Enter' && connect(c)} aria-label="{c.label} API key" />
        <button class="btn" onclick={() => connect(c)} disabled={busy === c.service}>{busy === c.service ? 'Checking…' : 'Connect'}</button>
      {/if}
    </div>
  {/each}
  <p class="note">Keys are checked, then stored in the system keychain. They are never saved in projects, logs or exports, and never shown again.</p>
</div>

<style>
  .conns {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .name {
    width: 110px;
    color: var(--text-2);
  }
  .ok {
    flex: 1;
    color: var(--ok);
  }
  .row .field {
    flex: 1;
  }
  .note {
    margin: 2px 0 0;
    font-size: 11px;
    color: var(--text-4);
  }
</style>
