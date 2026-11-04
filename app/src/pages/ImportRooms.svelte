<script lang="ts">
  // Import › Rooms: where a scene's room is set up. A room at a time: its space, how much of it
  // is heard, and the one ambience looping under it. The voices sit beside it, because how
  // much room a voice takes goes with the character, not with one room. A slider previews
  // here and sends one command on release (one undo step).
  import { tick } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { call, inTauri, pickPath } from '../lib/bridge';
  import { fileDrag, AMBIENCE_ZONE } from '../lib/filedrop.svelte';
  import { len, shortName } from '../lib/format';
  import { placesOf } from '../lib/sounds/room';
  import type { RoomRow, SceneRoom, VoiceRoom } from '../lib/types';
  import { colorOf, solid } from '../theme';
  import Icon from '../components/Icon.svelte';
  import ImportTabs from '../components/ImportTabs.svelte';
  import PreviewPlayer from '../components/PreviewPlayer.svelte';

  interface AmbienceSound {
    id: string;
    name: string;
    duration_s: number;
  }
  interface AmbienceGroup {
    name: string;
    here: boolean;
    sounds: AmbienceSound[];
  }

  let rows = $state<RoomRow[]>([]);
  let voices = $state<VoiceRoom[]>([]);
  let groups = $state<AmbienceGroup[]>([]);
  let picked = $state<string | null>(null);
  let night = $state(false);
  let find = $state('');
  let draft = $state<{ key: string; value: number } | null>(null);
  let sizeEl = $state<HTMLInputElement>();

  const places = $derived(placesOf(rows));
  const place = $derived(places.find((p) => p.id === picked) ?? places[0] ?? null);
  /** Scenes of a place share one room: this one speaks for them. */
  const room = $derived(place?.scenes[0] ?? null);
  /** Which ambience is being set: a mixed place has one for night too. */
  const showNight = $derived(!!place?.mixed && night);
  const slot = $derived(place?.mixed ? showNight : undefined);
  const tone = $derived(showNight ? (place?.night ?? null) : (place?.day ?? null));
  const words = $derived(find.toLowerCase().split(/\s+/).filter(Boolean));
  const choices = $derived(
    groups
      .map((g) => ({ ...g, sounds: g.sounds.filter((s) => s.id !== tone?.asset_id && words.every((w) => s.name.toLowerCase().includes(w))) }))
      .filter((g) => g.sounds.length),
  );

  async function load() {
    rows = (await app.run(() => call<RoomRow[]>('scene_rooms'))) ?? rows;
    voices = (await app.run(() => call<VoiceRoom[]>('voice_rooms'))) ?? voices;
    if (!picked) picked = rows.find((r) => r.scene_id === app.sceneId)?.location_id ?? rows[0]?.location_id ?? null;
  }
  $effect(() => {
    void app.status?.rev;
    void load();
  });
  $effect(() => {
    const scene = room?.scene_id;
    if (scene) void app.run(() => call<AmbienceGroup[]>('ambience_sounds', { scene })).then((g) => (groups = g ?? groups));
  });
  // a drop from the Finder goes to what's open here
  $effect(() => {
    app.roomPick = room ? { scene: room.scene_id, night: slot } : null;
  });

  // the shortcut in the transport lands on the current scene's Size
  $effect(() => {
    if (!app.roomsAsked || !rows.length) return;
    app.roomsAsked = false;
    picked = rows.find((r) => r.scene_id === app.sceneId)?.location_id ?? picked;
    void tick().then(() => sizeEl?.focus());
  });

  const value = (key: string, stored: number) => (draft?.key === key ? draft.value : stored);
  const preview = (key: string, e: Event) => (draft = { key, value: +(e.currentTarget as HTMLInputElement).value });

  /** One release, one command; the slider holds its value until the document has it. */
  async function commit(key: string, send: (v: number) => Promise<unknown>) {
    const d = draft;
    if (!d || d.key !== key) return;
    await send(d.value);
    await load();
    if (draft === d) draft = null;
  }

  const setRoom = (r: RoomRow, part: keyof SceneRoom) => (v: number) => app.cmd({ cmd: 'set_scene_room', scene_id: r.scene_id, room: { ...r.room, [part]: v } });

  async function useTone(r: RoomRow, asset_id?: string, gain_db?: number) {
    await app.method('set_scene_tone', { scene: r.scene_id, asset_id, gain_db, night: slot });
    await load();
  }

  async function addFile(r: RoomRow) {
    const got = inTauri ? await pickPath({ title: 'An ambience for this room', filters: [{ name: 'Audio', extensions: ['wav', 'aif', 'aiff', 'mp3', 'flac', 'm4a'] }] }) : [prompt('Path of the sound') ?? ''];
    if (!got?.[0]) return;
    await app.method('import_ambience', { path: got[0], scene: r.scene_id, night: slot });
    await load();
  }

  const face = (s: { name: string; duration_s: number }) => ({ name: s.name, duration_s: s.duration_s, peaks: [] });
  const shortOf = shortName;
</script>

{#snippet slider(label: string, low: string, high: string, key: string, stored: number, send: (v: number) => Promise<unknown>, aria: string)}
  <label class="ctl">
    <span class="lb">{label}</span>
    <span class="end muted">{low}</span>
    <input type="range" min="0" max="1" step="0.01" value={value(key, stored)} aria-label={aria} oninput={(e) => preview(key, e)} onchange={() => commit(key, send)} />
    <span class="end muted right">{high}</span>
    <span class="tn val">{Math.round(value(key, stored) * 100)}</span>
  </label>
{/snippet}

<div class="bar"><ImportTabs /></div>

<main class="page" data-testid="rooms">
  <nav class="list" aria-label="Rooms">
    {#each places as p (p.id)}
      <button class="row" class:on={p.id === place?.id} aria-current={p.id === place?.id} onclick={() => { picked = p.id; find = ''; night = false; }} data-testid="scene-room">
        <span class="who">
          <span class="ellip" title={p.scenes[0].heading}>{p.name}</span>
          <span class="muted tn sub ellip" data-testid="room-reading">{p.scenes[0].decay_s.toFixed(1)} s · Sc. {p.numbers.join(', ')}</span>
          <span class="muted sub ellip amb-name">{p.day ? shortOf(p.day.name) : 'No ambience'}</span>
        </span>
      </button>
    {/each}
  </nav>

  {#if room && place}
    <section class="room" aria-label="Room of {place.name}" data-testid="room-detail">
      <header>
        <h2>{place.name}</h2>
        <span class="muted">{place.numbers.length > 1 ? 'Scenes' : 'Scene'} {place.numbers.join(', ')}</span>
      </header>

      <div class="group">
        <h3>Space <span class="muted tn">{room.decay_s.toFixed(1)} s decay</span></h3>
        <label class="ctl">
          <span class="lb">Size</span>
          <span class="end muted">Small</span>
          <input bind:this={sizeEl} type="range" min="0" max="1" step="0.01" value={value('size', room.room.size)} aria-label="Size of scene {room.number}" oninput={(e) => preview('size', e)} onchange={() => commit('size', setRoom(room, 'size'))} />
          <span class="end muted right">Large</span>
          <span class="tn val">{Math.round(value('size', room.room.size) * 100)}</span>
        </label>
        {@render slider('Damping', 'Bright', 'Dark', 'damping', room.room.damping, setRoom(room, 'damping'), `Damping of scene ${room.number}`)}
        {@render slider('Wet', 'Dry', 'Full', 'wet', room.room.wet, setRoom(room, 'wet'), `Wet of scene ${room.number}`)}
      </div>

      <div class="group">
        <h3>Ambience <span class="muted">one per room, shared by its scenes</span></h3>
        {#if place.mixed}
          <div class="seg" role="tablist" aria-label="Time of day">
            <button role="tab" aria-selected={!night} class:on={!night} onclick={() => (night = false)}>Day</button>
            <button role="tab" aria-selected={night} class:on={night} onclick={() => (night = true)}>Night</button>
          </div>
        {/if}
        {#if tone}
          <div class="now" data-testid="room-ambience">
            <button class="pl" aria-label="Play {tone.name}" onclick={() => app.toggleSound({ asset_id: tone.asset_id }, face({ name: tone.name, duration_s: 0 }))}><Icon name={app.previewing(tone.asset_id) ? 'stop' : 'play'} size={11} /></button>
            <span class="ellip nm" title={tone.name}>{shortOf(tone.name)}</span>
            <button class="btn sm" onclick={() => useTone(room)}>Remove</button>
          </div>
          <label class="ctl lvl">
            <span class="lb">Level</span>
            <input type="range" min="-60" max="6" step="1" value={value('gain', tone.gain_db)} aria-label="Ambience level of scene {room.number}" oninput={(e) => preview('gain', e)} onchange={() => commit('gain', (v) => useTone(room, tone.asset_id, v))} />
            <span class="tn val db">{Math.round(value('gain', tone.gain_db))} dB</span>
          </label>
        {:else}
          <p class="muted none">{showNight ? 'Same as by day.' : room.place_tone ? "Plays its location's room tone." : 'No ambience yet.'}</p>
        {/if}
        <div class="dropz" class:live={fileDrag.paths.length > 0} class:over={fileDrag.over === AMBIENCE_ZONE} data-file-zone={AMBIENCE_ZONE} data-testid="ambience-drop">
          <span>Drop a sound here for this room</span>
          <button class="btn sm" onclick={() => addFile(room)}>Add a file…</button>
        </div>
        <input class="field findb" type="search" placeholder="Find an ambience" aria-label="Find an ambience" bind:value={find} />
        <div class="choices">
          {#each choices as g (g.name)}
            <div class="gname muted">{g.name}{#if g.here}<span class="here"> · asked for here</span>{/if}</div>
            {#each g.sounds as s (s.id)}
              <div class="choice" data-testid="ambience-choice">
                <button class="pl ghost" aria-label="Play {s.name}" onclick={() => app.toggleSound({ asset_id: s.id }, face(s))}><Icon name={app.previewing(s.id) ? 'stop' : 'play'} size={10} /></button>
                <span class="ellip nm" title={s.name}>{shortOf(s.name)}</span>
                <span class="muted tn d">{len(s.duration_s)}</span>
                <button class="btn sm" aria-label="Use {s.name}" onclick={() => useTone(room, s.id)}>Use</button>
              </div>
            {/each}
          {:else}
            <p class="muted none">{find ? 'Nothing by that name.' : 'No ambience in the library yet. Add a file.'}</p>
          {/each}
        </div>
      </div>
      <PreviewPlayer />
    </section>
  {/if}

  <aside class="voices" aria-label="Voices">
    <h3>Voices</h3>
    <p class="muted">How much of the room each character takes, in every scene. Their direct sound stays as it is.</p>
    {#each voices as v (v.id)}
      <label class="vrow" data-testid="voice-wet">
        <span class="ellip vn" style="color: {solid(colorOf(v.color_index).rgb)}">{v.label}</span>
        <input type="range" min="0" max="1" step="0.01" value={value(v.id, v.wet)} aria-label="Wet of {v.label}" oninput={(e) => preview(v.id, e)} onchange={() => commit(v.id, (wet) => app.cmd({ cmd: 'set_character_wet', character_id: v.id, wet }))} />
        <span class="tn val">{Math.round(value(v.id, v.wet) * 100)}</span>
      </label>
    {/each}
  </aside>
</main>

<style>
  .bar {
    position: absolute;
    top: 84px;
    left: 28px;
    right: 28px;
    display: flex;
    z-index: 3;
  }
  .page {
    position: absolute;
    top: 140px;
    left: 28px;
    right: 28px;
    bottom: 24px;
    display: grid;
    grid-template-columns: 240px minmax(0, 1fr) 280px;
    gap: 32px;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-height: 0;
    overflow: auto;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    flex: none;
    padding: 8px 10px;
    border-radius: 10px;
    text-align: left;
    transition: background var(--dur-fast);
  }
  .row:hover {
    background: var(--glass-fill-1);
  }
  .row.on {
    background: var(--glass-fill-3);
  }
  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
    gap: 1px;
    font-size: 13px;
  }
  .sub {
    font-size: 11px;
  }
  .room {
    display: flex;
    flex-direction: column;
    gap: 26px;
    min-height: 0;
    max-width: 640px;
    width: 100%;
    justify-self: center;
    overflow: auto;
    padding-right: 6px;
  }
  header {
    display: flex;
    align-items: baseline;
    gap: 12px;
  }
  h2 {
    margin: 0;
    font-size: 20px;
    font-weight: 600;
    color: var(--text-strong);
  }
  h3 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-strong);
  }
  h3 .muted {
    margin-left: 8px;
    font-weight: 400;
    font-size: 12px;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 10px;
    flex: none;
  }
  .ctl {
    display: grid;
    grid-template-columns: 64px 44px minmax(0, 1fr) 44px 30px;
    gap: 10px;
    align-items: center;
    font-size: 12.5px;
  }
  .ctl.lvl {
    grid-template-columns: 64px minmax(0, 1fr) 52px;
  }
  .lb {
    color: var(--text-2);
  }
  .end {
    font-size: 11px;
  }
  .right {
    text-align: right;
  }
  input[type='range'] {
    min-width: 0;
    width: 100%;
    accent-color: var(--text-strong);
  }
  .val {
    text-align: right;
    color: var(--text-2);
  }
  .now,
  .choice {
    display: grid;
    grid-template-columns: 28px minmax(0, 1fr) auto;
    gap: 10px;
    align-items: center;
    font-size: 12.5px;
  }
  .choice {
    grid-template-columns: 28px minmax(0, 1fr) 34px auto;
    padding: 3px 0;
  }
  .now {
    padding: 8px 10px;
    border-radius: 12px;
    background: rgb(var(--sfx-rgb) / 0.1);
  }
  .now .nm {
    font-weight: 600;
  }
  .pl {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--sfx);
    color: var(--on-sfx);
  }
  .pl.ghost {
    background: none;
    color: var(--text-strong);
    box-shadow: inset 0 0 0 1.5px rgb(var(--ink-rgb) / 0.35);
  }
  .seg {
    display: inline-flex;
    align-self: flex-start;
    gap: 2px;
    padding: 2px;
    border-radius: var(--radius-pill);
    background: var(--glass-fill-1);
  }
  .seg button {
    height: 24px;
    padding: 0 12px;
    border-radius: var(--radius-pill);
    font-size: 11.5px;
    color: var(--text-3);
  }
  .seg button.on {
    color: var(--text-strong);
    background: var(--glass-fill-3);
  }
  .dropz {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 14px;
    border-radius: 12px;
    border: 1px dashed rgb(var(--ink-rgb) / 0.25);
    font-size: 12.5px;
    color: var(--text-3);
    transition:
      background var(--dur-fast),
      border-color var(--dur-fast),
      color var(--dur-fast);
  }
  .dropz.live {
    border-color: var(--sfx);
    color: var(--text-strong);
  }
  .dropz.over {
    border-style: solid;
    background: rgb(var(--sfx-rgb) / 0.16);
  }
  .findb {
    height: 30px;
    font-size: 12.5px;
  }
  .amb-name {
    color: var(--text-4);
  }
  .btn.sm {
    height: 28px;
    padding: 0 12px;
    font-size: 11.5px;
    white-space: nowrap;
  }
  .choices {
    display: flex;
    flex-direction: column;
  }
  .gname {
    margin-top: 8px;
    padding-bottom: 2px;
    font-size: 11px;
  }
  .here {
    color: var(--sfx);
  }
  .d {
    font-size: 10.5px;
    text-align: right;
  }
  .none {
    margin: 0;
    font-size: 12px;
  }
  .voices {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
    overflow: auto;
  }
  .voices p {
    margin: 0 0 4px;
    font-size: 12px;
    line-height: 1.4;
  }
  .vrow {
    display: grid;
    grid-template-columns: 84px minmax(0, 1fr) 30px;
    gap: 10px;
    align-items: center;
    font-size: 12.5px;
  }
  @media (max-width: 1100px) {
    .page {
      grid-template-columns: 180px minmax(0, 1fr);
    }
    .voices {
      grid-column: 2;
    }
  }
</style>
