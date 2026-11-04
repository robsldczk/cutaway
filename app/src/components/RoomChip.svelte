<script lang="ts">
  // The current scene's room, in the transport: a read-out and a way in. The room lives in
  // the document and its settings on Import › Sounds; the size here is the same command, one
  // undo step per release, and holds its value until the document has it.
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import { roomLabel, roomOfScene, roomText, sizeCommand } from '../lib/sounds/room';
  import type { RoomRow } from '../lib/types';

  let rows = $state<RoomRow[]>([]);
  let open = $state(false);
  let draft = $state<number | null>(null);
  let wrap = $state<HTMLDivElement>();
  let chip = $state<HTMLButtonElement>();

  async function load() {
    rows = (await app.run(() => call<RoomRow[]>('scene_rooms'))) ?? rows;
  }
  $effect(() => {
    void app.status?.rev;
    void app.sceneId;
    void load();
  });

  const row = $derived(roomOfScene(rows, app.sceneId));
  const room = $derived(row ? { ...row.room, size: draft ?? row.room.size } : null);

  async function commit() {
    const size = draft;
    if (size == null || !row) return;
    if (size !== row.room.size) {
      await app.cmd(sizeCommand(row, size));
      await load();
    }
    if (draft === size) draft = null;
  }

  function close() {
    open = false;
    chip?.focus();
  }

  // a click elsewhere closes
  $effect(() => {
    if (!open) return;
    const down = (e: MouseEvent) => {
      if (!wrap?.contains(e.target as Node)) open = false;
    };
    window.addEventListener('mousedown', down, true);
    return () => window.removeEventListener('mousedown', down, true);
  });

  function keys(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !open) return;
    e.stopPropagation();
    close();
  }
</script>

{#if row && room}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="wrap" bind:this={wrap} onkeydown={keys} data-own-keys>
    <button class="chip tn" bind:this={chip} aria-expanded={open} aria-label={roomLabel(room, row.decay_s)} title="This scene's room" onmousedown={(e) => e.preventDefault()} onclick={() => (open = !open)} data-testid="room-chip">{roomText(room)}</button>
    {#if open}
      <div class="glass pop" role="group" aria-label="Room of this scene" data-testid="room-pop">
        <label class="size">
          <span class="muted">Size</span>
          <!-- after a drag Space must play again: the slider gives the keys back -->
          <input type="range" min="0" max="1" step="0.01" value={room.size} aria-label="Size of this scene's room" oninput={(e) => (draft = +e.currentTarget.value)} onchange={commit} onpointerup={(e) => e.currentTarget.blur()} />
          <span class="tn val">{Math.round(room.size * 100)}</span>
        </label>
        <div class="muted tn small">{row.decay_s.toFixed(1)} s decay · {row.sounds} sound{row.sounds === 1 ? '' : 's'}</div>
        <button class="go" onclick={() => app.openRooms()} data-testid="room-settings">Room settings…</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .wrap {
    position: relative;
    flex: none;
    align-self: center;
  }
  .chip {
    padding: 5px 10px;
    border-radius: var(--radius-pill);
    font-size: var(--fs-small);
    white-space: nowrap;
    color: var(--text-3);
    background: var(--glass-fill-3);
  }
  .chip:hover,
  .chip[aria-expanded='true'] {
    color: var(--text-strong);
  }
  .chip:focus-visible,
  .go:focus-visible {
    outline: 2px solid var(--playhead);
    outline-offset: 1px;
  }
  .pop {
    position: absolute;
    right: 0;
    bottom: calc(100% + 12px);
    width: 240px;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    border-radius: 14px;
    background: var(--panel);
    z-index: 12;
  }
  .size {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .size input {
    flex: 1;
    min-width: 0;
    accent-color: var(--text-strong);
  }
  .val {
    width: 26px;
    text-align: right;
    color: var(--text-2);
  }
  .small {
    font-size: 11px;
  }
  .go {
    align-self: flex-start;
    padding: 0;
    font-size: var(--fs-small);
    color: var(--text-strong);
    text-decoration: underline;
    text-underline-offset: 3px;
  }
</style>
