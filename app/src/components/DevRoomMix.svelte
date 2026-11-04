<script lang="ts">
  // Dev › Room mix: how wet everything is. Each scene's room level, and how much of it each
  // voice takes. A slider previews here and sends one command on release (one undo step).
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import type { RoomRow, VoiceRoom } from '../lib/types';
  import { colorOf, solid } from '../theme';

  let rooms = $state<RoomRow[]>([]);
  let voices = $state<VoiceRoom[]>([]);
  let draft = $state<{ id: string; wet: number } | null>(null);

  async function load() {
    rooms = (await app.run(() => call<RoomRow[]>('scene_rooms'))) ?? rooms;
    voices = (await app.run(() => call<VoiceRoom[]>('voice_rooms'))) ?? voices;
  }
  $effect(() => {
    void app.status?.rev;
    void load();
  });

  const shown = (id: string, wet: number) => (draft?.id === id ? draft.wet : wet);
  const preview = (id: string, e: Event) => (draft = { id, wet: +(e.currentTarget as HTMLInputElement).value });

  async function commit(id: string, send: (wet: number) => Promise<unknown>) {
    const d = draft;
    if (!d || d.id !== id) return;
    await send(d.wet);
    await load();
    if (draft === d) draft = null;
  }
</script>

<div class="mix" data-testid="room-mix">
  <section aria-label="Voices">
    <h3>Voices</h3>
    <p class="muted">How much of its scene's room each voice takes. The direct sound stays as it is.</p>
    {#each voices as v (v.id)}
      <label class="row" data-testid="voice-wet">
        <span class="who ellip" style="color: {solid(colorOf(v.color_index).rgb)}">{v.label}</span>
        <input type="range" min="0" max="1" step="0.01" value={shown(v.id, v.wet)} aria-label="Wet of {v.label}" oninput={(e) => preview(v.id, e)} onchange={() => commit(v.id, (wet) => app.cmd({ cmd: 'set_character_wet', character_id: v.id, wet }))} />
        <span class="tn val">{Math.round(shown(v.id, v.wet) * 100)}</span>
      </label>
    {/each}
  </section>
  <section aria-label="Scenes">
    <h3>Scenes</h3>
    <p class="muted">How loud each scene's room is, under everything that plays in it.</p>
    {#each rooms as r (r.scene_id)}
      <label class="row">
        <span class="who ellip" title={r.heading}><b class="tn">{r.number}</b> {r.place || r.heading}</span>
        <input type="range" min="0" max="1" step="0.01" value={shown(r.scene_id, r.room.wet)} aria-label="Wet of scene {r.number}" oninput={(e) => preview(r.scene_id, e)} onchange={() => commit(r.scene_id, (wet) => app.cmd({ cmd: 'set_scene_room', scene_id: r.scene_id, room: { ...r.room, wet } }))} />
        <span class="tn val">{Math.round(shown(r.scene_id, r.room.wet) * 100)}</span>
      </label>
    {/each}
  </section>
</div>

<style>
  .mix {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 32px;
    padding: 12px 4px;
    align-content: start;
  }
  h3 {
    margin: 0;
    font-size: 14px;
    color: var(--text-strong);
  }
  p {
    margin: 4px 0 10px;
    font-size: 12px;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(120px, 1fr) minmax(120px, 2fr) 30px;
    gap: 12px;
    align-items: center;
    padding: 5px 0;
    font-size: 12.5px;
  }
  input {
    min-width: 0;
    accent-color: var(--text-strong);
  }
  .val {
    text-align: right;
    color: var(--text-2);
  }
</style>
