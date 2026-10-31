<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import Sheet from './Sheet.svelte';

  interface Backup { id: string; kind: string; name: string | null; file: string; created_at: string }
  interface Log { seq: number; ts: string; client: string; name: string; state: string }
  let backups = $state<Backup[]>([]);
  let log = $state<Log[]>([]);
  let name = $state('');

  async function load() {
    backups = (await app.run(() => call<Backup[]>('list_versions'))) ?? [];
    log = (await app.run(() => call<Log[]>('history', { limit: 30 }))) ?? [];
  }
  onMount(load);

  async function save() {
    if (!name.trim()) return;
    await app.method('save_version', { name: name.trim() }, false);
    name = '';
    app.say('Version saved');
    await load();
  }
  async function revert(b: Backup) {
    if (!confirm(`Revert to “${b.name ?? b.kind}” from ${new Date(b.created_at).toLocaleString()}? The current state is saved first.`)) return;
    await app.method('revert', { backup_id: b.id, confirm: true });
    app.say('Reverted');
    await load();
  }
</script>

<Sheet title="Save and history" width={640}>
  <div class="save"><input class="field" placeholder="Version name, e.g. Director cut 1" bind:value={name} onkeydown={(e) => e.key === 'Enter' && save()} /><button class="btn primary" onclick={save}>Save version</button></div>
  <div class="cols">
    <div>
      <div class="label">Versions and backups</div>
      {#each backups as b (b.id)}
        <div class="row"><span><b>{b.name ?? (b.kind === 'auto' ? 'Autosave' : 'Before revert')}</b> <span class="muted">{new Date(b.created_at).toLocaleString()}</span></span><button class="btn small" onclick={() => revert(b)}>Revert</button></div>
      {:else}<p class="muted">No backups yet. Autosave snapshots every 10 minutes while you work.</p>{/each}
    </div>
    <div>
      <div class="label">Recent changes</div>
      {#each log as l (l.seq)}
        <div class="row"><span class:undone={l.state === 'undone'}>{l.name.replaceAll('_', ' ')}</span><span class="muted">{l.client}</span></div>
      {/each}
    </div>
  </div>
</Sheet>

<style>
  .save {
    display: flex;
    gap: 10px;
    margin-bottom: 16px;
  }
  .save .field {
    flex: 1;
  }
  .cols {
    display: grid;
    grid-template-columns: 1.3fr 1fr;
    gap: 20px;
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
    padding: 6px 0;
    border-bottom: 1px solid var(--glass-outline);
    font-size: 12px;
  }
  .small {
    height: 26px;
    padding: 0 10px;
  }
  .undone {
    text-decoration: line-through;
    color: var(--text-4);
  }
</style>
