<script lang="ts">
  // Import › Sounds: each scene's room (its wet is in Dev › Room mix). All sounds of a scene play in it, so they sit in the
  // same space; a sound's depth says how far back. A slider previews here and sends one
  // command on release (one undo step), holding its value until the document has it.
  // Each scene also picks the room tone that loops under it, from the sounds marked Loop.
  import { tick } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import type { Bucket, RoomRow, SceneRoom } from '../lib/types';

  const CONTROLS: { key: keyof SceneRoom; label: string; low: string; high: string }[] = [
    { key: 'size', label: 'Size', low: 'Small', high: 'Large' },
    { key: 'damping', label: 'Damping', low: 'Bright', high: 'Dark' },
  ];

  let { buckets = [] }: { buckets?: Bucket[] } = $props();

  let rows = $state<RoomRow[]>([]);
  let toneDraft = $state<{ id: string; gain: number } | null>(null);

  /** What can loop under a scene, a bucket at a time; room tone buckets come first. */
  const tones = $derived(
    buckets
      .filter((b) => !b.character)
      .map((b) => ({ id: b.id, name: b.name, sounds: b.kept.filter((s) => s.loop) }))
      .filter((b) => b.sounds.length)
      .sort((a, b) => +/room.?tone/i.test(b.name) - +/room.?tone/i.test(a.name)),
  );
  const offered = $derived(new Set(tones.flatMap((b) => b.sounds.map((s) => s.id))));

  async function pickTone(r: RoomRow, e: Event) {
    const asset_id = (e.currentTarget as HTMLSelectElement).value || undefined;
    await app.method('set_scene_tone', { scene: r.scene_id, asset_id });
    await load();
  }

  const gainOf = (r: RoomRow) => (toneDraft?.id === r.scene_id ? toneDraft.gain : (r.tone?.gain_db ?? -6));

  async function commitGain(r: RoomRow) {
    const d = toneDraft;
    if (!d || d.id !== r.scene_id || !r.tone) return;
    await app.method('set_scene_tone', { scene: r.scene_id, asset_id: r.tone.asset_id, gain_db: d.gain });
    await load();
    if (toneDraft === d) toneDraft = null;
  }
  let draft = $state<{ id: string; room: SceneRoom } | null>(null);
  let box = $state<HTMLDetailsElement>();

  async function load() {
    rows = (await app.run(() => call<RoomRow[]>('scene_rooms'))) ?? rows;
  }
  $effect(() => {
    void app.status?.rev;
    void load();
  });

  // the shortcut in the transport lands on the current scene's sliders
  $effect(() => {
    if (!app.roomsAsked || !box || !rows.length) return;
    app.roomsAsked = false;
    box.open = true;
    void tick().then(() => {
      const here = box?.querySelector<HTMLElement>('.here');
      here?.scrollIntoView({ block: 'center' });
      here?.querySelector('input')?.focus();
    });
  });

  const roomOf = (r: RoomRow) => (draft?.id === r.scene_id ? draft.room : r.room);

  function preview(r: RoomRow, key: keyof SceneRoom, e: Event) {
    const value = +(e.currentTarget as HTMLInputElement).value;
    draft = { id: r.scene_id, room: { ...roomOf(r), [key]: value } };
  }

  async function commit(r: RoomRow) {
    const d = draft;
    if (!d || d.id !== r.scene_id) return;
    await app.cmd({ cmd: 'set_scene_room', scene_id: d.id, room: { ...d.room } });
    await load();
    if (draft === d) draft = null;
  }
</script>

<details class="rooms glass" bind:this={box} data-testid="scene-rooms">
  <summary>
    <span class="nm">Rooms</span>
    <span class="muted">One per scene. Its sounds share the room, depth sets how far back each one sits, and its room tone loops under the whole scene.</span>
  </summary>
  <div class="list">
    {#each rows as r (r.scene_id)}
      {@const room = roomOf(r)}
      <section class="row" class:here={r.scene_id === app.sceneId} aria-label="Room of scene {r.number}" data-testid="scene-room">
        <div class="who">
          <span class="ellip" title={r.heading}><b class="tn">{r.number}</b> {r.place || r.heading}</span>
          <span class="muted tn" data-testid="room-reading">{r.decay_s.toFixed(1)} s decay · {r.sounds} sound{r.sounds === 1 ? '' : 's'}</span>
        </div>
        {#each CONTROLS as c (c.key)}
          <label class="ctl" title="{c.low} to {c.high}">
            <span class="muted">{c.label}</span>
            <input type="range" min="0" max="1" step="0.01" value={room[c.key]} aria-label="{c.label} of scene {r.number}" oninput={(e) => preview(r, c.key, e)} onchange={() => commit(r)} />
            <span class="tn val">{Math.round(room[c.key] * 100)}</span>
          </label>
        {/each}
        <div class="tone">
          <label class="ctl">
            <span class="muted">Room tone</span>
            <select class="pick" aria-label="Room tone of scene {r.number}" value={r.tone?.asset_id ?? ''} onchange={(e) => pickTone(r, e)} data-testid="room-tone">
              <option value="">{r.place_tone ? 'As the location' : 'None'}</option>
              {#if r.tone && !offered.has(r.tone.asset_id)}<option value={r.tone.asset_id}>{r.tone.name}</option>{/if}
              {#each tones as b (b.id)}
                <optgroup label={b.name}>
                  {#each b.sounds as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
                </optgroup>
              {/each}
            </select>
          </label>
          {#if r.tone}
            <label class="ctl lvl" title="Level of the room tone">
              <input type="range" min="-48" max="6" step="1" value={gainOf(r)} aria-label="Room tone level of scene {r.number}" oninput={(e) => (toneDraft = { id: r.scene_id, gain: +(e.currentTarget as HTMLInputElement).value })} onchange={() => commitGain(r)} />
              <span class="tn val db">{Math.round(gainOf(r))} dB</span>
            </label>
          {/if}
        </div>
      </section>
    {/each}
  </div>
</details>

<style>
  .rooms {
    grid-column: 1 / -1;
    border-radius: 12px;
    background: var(--glass-fill-2);
    padding: 10px 14px;
  }
  summary {
    display: flex;
    align-items: baseline;
    gap: 10px;
    cursor: pointer;
  }
  .nm {
    color: var(--text-strong);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-top: 10px;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(150px, 1.2fr) repeat(2, minmax(120px, 1fr)) minmax(240px, 1.6fr);
    gap: 18px;
    align-items: center;
    padding: 6px 8px;
    border-radius: 8px;
  }
  .row.here {
    background: var(--glass-fill-3);
  }
  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
    gap: 2px;
  }
  .who .muted {
    font-size: 11px;
  }
  .ctl {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .ctl input {
    flex: 1;
    min-width: 0;
    accent-color: var(--text-strong);
  }
  .tone {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .pick {
    flex: 1;
    min-width: 0;
    height: 24px;
    font-size: 11.5px;
    color: var(--text-2);
    background: transparent;
    border: 0;
    box-shadow: inset 0 0 0 1px rgb(var(--ink-rgb) / 0.16);
    border-radius: 7px;
  }
  .val.db {
    width: 52px;
    white-space: nowrap;
  }
  .val {
    width: 26px;
    text-align: right;
    color: var(--text-2);
  }
</style>
