<script lang="ts">
  // Where the characters stand, from the transport. A scene inherits the room's values; moving a
  // slider here gives the scene one of its own, and Whole room sets what its scenes inherit.
  // One undo step per release; the slider holds its value until the document has it.
  import { app } from '../lib/app.svelte';
  import { charColor } from '../lib/colors';
  import type { VoicePan } from '../lib/types';
  import Icon from './Icon.svelte';

  let open = $state(false);
  let room = $state(false);
  let draft = $state<{ id: string; pan: number } | null>(null);
  let wrap = $state<HTMLDivElement>();
  let button = $state<HTMLButtonElement>();

  const pans = $derived(app.view?.pans ?? []);
  const place = $derived(app.view?.scene.place || 'this room');
  const stored = (p: VoicePan) => (room ? p.room : p.pan);
  const shown = (p: VoicePan) => (draft?.id === p.character ? draft.pan : stored(p));
  const reading = (x: number) => (Math.abs(x) < 0.03 ? 'C' : `${x < 0 ? 'L' : 'R'} ${Math.round(Math.abs(x) * 100)}`);
  /** A soft catch at the centre. */
  const caught = (x: number) => (Math.abs(x) < 0.04 ? 0 : x);

  const send = (character: string, pan: number | null) => app.cmd({ cmd: 'set_character_pan', scene_id: app.sceneId, character_id: character, pan, room });

  async function commit(p: VoicePan) {
    const d = draft;
    if (d?.id !== p.character) return;
    try {
      if (d.pan !== stored(p)) await send(p.character, d.pan);
    } finally {
      if (draft === d) draft = null;
    }
  }

  function close() {
    open = false;
    button?.focus();
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

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="wrap" bind:this={wrap} onkeydown={keys} data-own-keys>
  <button class="icb" class:on={open} bind:this={button} aria-expanded={open} aria-label="Where the characters stand" title="Pan of the characters" onmousedown={(e) => e.preventDefault()} onclick={() => (open = !open)} data-testid="pan-button"><Icon name="pan" size={20} /></button>
  {#if open}
    <div class="glass pop" role="group" aria-label="Pan of the characters" data-testid="pan-pop">
      <div class="scope" role="radiogroup" aria-label="What the sliders set">
        <button role="radio" aria-checked={!room} class:on={!room} onclick={() => (room = false)} data-testid="pan-scene">This scene</button>
        <button role="radio" aria-checked={room} class:on={room} onclick={() => (room = true)} data-testid="pan-room">Whole room</button>
      </div>
      {#each pans as p (p.character)}
        <div class="row" data-testid="pan-row">
          <span class="who ellip" style="color: {charColor(app.view, p.character).name}">{p.label}</span>
          <!-- after a drag Space must play again: the slider gives the keys back -->
          <input type="range" min="-1" max="1" step="0.01" value={shown(p)} aria-label="Pan of {p.label}" aria-valuetext={reading(shown(p))} oninput={(e) => (draft = { id: p.character, pan: caught(+e.currentTarget.value) })} onchange={() => commit(p)} onpointerup={(e) => e.currentTarget.blur()} />
          <span class="val tn" data-testid="pan-value">{reading(shown(p))}</span>
          {#if !room && p.custom}
            <button class="back" aria-label="Back to the room's value for {p.label}" title="Back to the room's value ({reading(p.room)})" onclick={() => send(p.character, null)} data-testid="pan-reset"><Icon name="cycle" size={12} /></button>
          {:else}
            <span class="back"></span>
          {/if}
        </div>
      {:else}
        <div class="muted small">Nobody speaks in this scene.</div>
      {/each}
      <div class="muted small">{room ? `Every scene in ${place} without its own value.` : `Starts from ${place}; a change stays in this scene.`}</div>
      <button class="go" onclick={() => app.openRooms()} data-testid="room-settings">Room settings…</button>
    </div>
  {/if}
</div>

<style>
  .wrap {
    position: relative;
    flex: none;
    align-self: center;
  }
  .icb.on {
    color: var(--text-strong);
    background: var(--glass-fill-4);
  }
  .icb:focus-visible,
  .go:focus-visible,
  .scope button:focus-visible,
  button.back:focus-visible {
    outline: 2px solid var(--playhead);
    outline-offset: 1px;
  }
  .pop {
    position: absolute;
    right: 0;
    bottom: calc(100% + 12px);
    width: 300px;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    border-radius: 14px;
    background: var(--panel);
    z-index: 12;
  }
  .scope {
    display: flex;
    gap: 2px;
    padding: 2px;
    border-radius: 9px;
    background: var(--glass-fill-3);
  }
  .scope button {
    flex: 1;
    height: 24px;
    border-radius: 7px;
    font-size: var(--fs-small);
    color: var(--text-3);
  }
  .scope button.on {
    background: var(--glass-active);
    color: var(--text-1);
    font-weight: 600;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .who {
    flex: none;
    width: 76px;
    font-size: var(--fs-small);
    font-weight: 600;
  }
  .row input {
    flex: 1;
    min-width: 0;
    accent-color: var(--text-strong);
  }
  .val {
    flex: none;
    width: 34px;
    text-align: right;
    font-size: var(--fs-small);
    color: var(--text-2);
  }
  .back {
    flex: none;
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
    color: var(--text-4);
  }
  button.back:hover {
    background: var(--glass-hover);
    color: var(--text-1);
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
